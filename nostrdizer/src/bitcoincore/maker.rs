use crate::bitcoincore::utils::{get_eligible_balance, sign_tx_hex};
use crate::errors::Error;
use crate::types::{BitcoinCoreCreditals, Fill, IoAuth, Maker, MakerConfig};
use crate::utils;
use nostr_rust::{keys::get_random_secret_key, nostr_client::Client as NostrClient, Identity};

use log::debug;

use bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::SignRawTransactionResult;

impl Maker {
    #[cfg(feature = "bitcoincore")]
    pub fn new(
        priv_key: Option<String>,
        relay_urls: Vec<&str>,
        config: &mut MakerConfig,
        bitcoin_core_creds: BitcoinCoreCreditals,
    ) -> Result<Self, Error> {
        use std::str::FromStr;

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
    #[cfg(feature = "bitcoincore")]
    pub fn sign_tx_hex(
        &mut self,
        unsigned_tx_hex: &str,
    ) -> Result<SignRawTransactionResult, Error> {
        sign_tx_hex(unsigned_tx_hex, &self.rpc_client)
    }

    /// Send signed tx back to taker
    #[cfg(feature = "bitcoincore")]
    pub fn send_signed_tx(
        &mut self,
        peer_pub_key: &str,
        signed_tx: &SignRawTransactionResult,
    ) -> Result<(), Error> {
        utils::send_signed_tx(
            &self.identity,
            peer_pub_key,
            signed_tx.clone(),
            &mut self.nostr_client,
        )?;
        Ok(())
    }

    /// Gets maker input for CJ
    #[cfg(feature = "bitcoincore")]
    pub fn get_inputs(&mut self, fill_offer: &Fill) -> Result<IoAuth, Error> {
        let unspent = self.rpc_client.list_unspent(None, None, None, None, None)?;
        let mut inputs = vec![];
        let mut value: Amount = Amount::ZERO;
        for utxo in unspent {
            let input = (utxo.txid, utxo.vout);

            inputs.push(input);
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
}