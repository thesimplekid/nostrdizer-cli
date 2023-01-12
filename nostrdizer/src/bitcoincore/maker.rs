use crate::bitcoincore::{
    types::BitcoinCoreCreditals,
    utils::{get_eligible_balance, sign_tx_hex},
};
use crate::errors::Error;
use crate::types::{Fill, IoAuth, MakerConfig, VerifyCJInfo};
use crate::utils;
use nostr_rust::{keys::get_random_secret_key, nostr_client::Client as NostrClient, Identity};

use log::debug;

use bitcoin::blockdata::transaction::OutPoint;
use bitcoin::psbt::PartiallySignedTransaction;
use bitcoin::{Amount, SignedAmount};
use bitcoin_hashes::sha256;
use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::SignRawTransactionResult;
use std::str::FromStr;

pub struct Maker {
    pub identity: Identity,
    pub config: MakerConfig,
    pub nostr_client: NostrClient,
    pub rpc_client: RPCClient,
    pub fill_commitment: Option<sha256::Hash>,
}

impl Maker {
    pub fn new(
        priv_key: Option<String>,
        relay_urls: Vec<&str>,
        config: &mut MakerConfig,
        bitcoin_core_creds: BitcoinCoreCreditals,
    ) -> Result<Self, Error> {
        let priv_key = match priv_key {
            Some(key) => key,
            None => {
                let (sk, _) = get_random_secret_key();
                hex::encode(sk.as_ref())
            }
        };
        let identity = Identity::from_str(&priv_key)?;

        let nostr_client = NostrClient::new(relay_urls)?;

        let rpc_client = RPCClient::new(
            &bitcoin_core_creds.rpc_url,
            Auth::UserPass(
                bitcoin_core_creds.rpc_username,
                bitcoin_core_creds.rpc_password,
            ),
        )?;

        if config.maxsize.is_none() {
            let bal = get_eligible_balance(&rpc_client)?;
            config.maxsize = Some(bal);
        }

        let maker = Self {
            identity,
            config: config.clone(),
            nostr_client,
            rpc_client,
            fill_commitment: None,
        };
        Ok(maker)
    }

    /// Sign tx hex
    pub fn sign_tx_hex(
        &mut self,
        unsigned_tx_hex: &str,
    ) -> Result<SignRawTransactionResult, Error> {
        sign_tx_hex(unsigned_tx_hex, &self.rpc_client)
    }

    /// Send signed tx back to taker
    pub fn send_signed_tx(
        &mut self,
        peer_pub_key: &str,
        psbt: &PartiallySignedTransaction,
    ) -> Result<(), Error> {
        utils::send_signed_tx(
            &self.identity,
            peer_pub_key,
            psbt.clone(),
            &mut self.nostr_client,
        )?;
        Ok(())
    }

    /// Gets maker input for CJ
    pub fn get_inputs(&mut self, fill_offer: &Fill) -> Result<IoAuth, Error> {
        let unspent = self.rpc_client.list_unspent(None, None, None, None, None)?;
        let mut inputs = vec![];
        let mut value: Amount = Amount::ZERO;
        for utxo in unspent {
            let input = OutPoint::new(utxo.txid, utxo.vout);

            inputs.push((input, None));
            value += utxo.amount;

            if value >= fill_offer.amount {
                break;
            }
        }

        let coinjoin_address = self.rpc_client.get_new_address(Some("CJ out"), None)?;
        debug!("Maker cj out: {}", coinjoin_address);

        let change_address = self.rpc_client.get_raw_change_address(None).unwrap();
        debug!("Maker change out: {}", change_address);

        let maker_input = IoAuth {
            utxos: inputs,
            coinjoin_address,
            change_address,
            maker_auth_pub: "".to_string(),
            bitcoin_sig: "".to_string(),
        };

        Ok(maker_input)
    }

    #[cfg(feature = "bitcoincore")]
    pub fn get_eligible_balance(&mut self) -> Result<Amount, Error> {
        get_eligible_balance(&self.rpc_client)
    }

    pub fn verify_transaction(
        &mut self,
        psbt: PartiallySignedTransaction,
        send_amount: &Amount,
    ) -> Result<VerifyCJInfo, Error> {
        // let decoded_transaction = self.rpc_client.decode_psbt(psbt).unwrap();
        // let tx = decoded_transaction.tx;
        //let (input_value, my_input_value) = get_input_value(tx.vin, &self.rpc_client)?;
        //let (output_value, my_output_value) = get_output_value(tx.vout, &self.rpc_client)?;
        /*
        let mining_fee = decoded_transaction
            .fee
            .unwrap_or(Amount::ZERO)
            .to_signed()?;
            */

        // TODO: this obviously does nothing
        Ok(VerifyCJInfo {
            mining_fee: SignedAmount::ZERO,
            maker_fee: SignedAmount::ZERO,
            verifyed: true,
        })
    }
    /// Maker sign psbt
    pub fn sign_psbt(&mut self, unsigned_psbt: &str) -> Result<PartiallySignedTransaction, Error> {
        let signed_psbt =
            self.rpc_client
                .wallet_process_psbt(unsigned_psbt, Some(true), None, None)?;
        Ok(PartiallySignedTransaction::from_str(&signed_psbt.psbt).unwrap())
    }
}
