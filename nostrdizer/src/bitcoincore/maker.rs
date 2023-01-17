use super::utils::{get_eligible_balance, get_input_value, get_output_value};

use crate::{
    errors::Error,
    types::{BlockchainConfig, Fill, IoAuth, MakerConfig, VerifyCJInfo},
    utils::send_signed_psbt,
};

use nostr_rust::{keys::get_random_secret_key, nostr_client::Client as NostrClient, Identity};

use log::debug;

use bitcoin::{
    blockdata::transaction::OutPoint, psbt::PartiallySignedTransaction, Amount, Denomination,
};
use bitcoin_hashes::sha256;
use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};

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
        bitcoin_core_creds: BlockchainConfig,
    ) -> Result<Self, Error> {
        let bitcoin_core_creds = match bitcoin_core_creds {
            BlockchainConfig::CoreRPC(creds) => creds,
            _ => return Err(Error::InvalidCredentials),
        };
        let priv_key = match priv_key {
            Some(key) => key,
            None => {
                let (sk, _) = get_random_secret_key();
                hex::encode(sk.as_ref())
            }
        };
        let identity = Identity::from_str(&priv_key)?;

        let nostr_client = NostrClient::new(relay_urls)?;
        let wallet_url = format!(
            "{}/wallet/{}",
            &bitcoin_core_creds.rpc_url, &bitcoin_core_creds.wallet_name
        );
        let rpc_client = RPCClient::new(
            &wallet_url,
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

    /// Publishes signed psbt to nostr
    pub fn publish_signed_psbt(
        &mut self,
        peer_pub_key: &str,
        psbt: PartiallySignedTransaction,
    ) -> Result<(), Error> {
        send_signed_psbt(&self.identity, peer_pub_key, psbt, &mut self.nostr_client)
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

    pub fn get_eligible_balance(&mut self) -> Result<Amount, Error> {
        get_eligible_balance(&self.rpc_client)
    }

    pub fn verify_transaction(
        &mut self,
        psbt: &PartiallySignedTransaction,
        send_amount: &Amount,
    ) -> Result<VerifyCJInfo, Error> {
        let decoded_transaction = self.rpc_client.decode_psbt(&psbt.to_string()).unwrap();
        let tx = decoded_transaction.tx;
        let (_input_value, my_input_value) = get_input_value(&tx.vin, &self.rpc_client)?;
        let (_output_value, my_output_value) = get_output_value(&tx.vout, &self.rpc_client)?;

        let maker_fee = my_output_value.to_signed()? - my_input_value.to_signed()?;
        debug!("Maker fee: {maker_fee}");

        let mining_fee = decoded_transaction
            .fee
            .unwrap_or(Amount::ZERO)
            .to_signed()?;

        let abs_fee_check = maker_fee.ge(&self.config.abs_fee.to_signed()?);
        debug!("abs value check {abs_fee_check}");
        let fee_as_percent = maker_fee.to_float_in(Denomination::Satoshi)
            / send_amount.to_float_in(Denomination::Satoshi);

        debug!("Fee as percent {:?}", fee_as_percent);
        let rel_fee_check = fee_as_percent.ge(&self.config.rel_fee);

        debug!("rel fee check {rel_fee_check}");
        // Max send amount check
        let max_amount_check = match &self.config.maxsize {
            Some(max_size) => send_amount <= max_size,
            None => true,
        };
        debug!("Max amount {max_amount_check}");
        Ok(VerifyCJInfo {
            mining_fee,
            maker_fee,
            verifyed: abs_fee_check
                && rel_fee_check
                && max_amount_check
                && send_amount.ge(&self.config.minsize),
        })
    }
    /// Maker sign psbt
    pub fn sign_psbt(
        &mut self,
        unsigned_psbt: &PartiallySignedTransaction,
    ) -> Result<PartiallySignedTransaction, Error> {
        let signed_psbt = self.rpc_client.wallet_process_psbt(
            &unsigned_psbt.to_string(),
            Some(true),
            None,
            None,
        )?;
        Ok(PartiallySignedTransaction::from_str(&signed_psbt.psbt).unwrap())
    }
}
