use super::utils::new_wallet;

use crate::{
    errors::Error,
    maker::Maker,
    types::BlockchainConfig,
    types::{Fill, IoAuth, MakerConfig, VerifyCJInfo},
    utils::send_signed_psbt,
};

use bdk::{
    bitcoin::{psbt::PartiallySignedTransaction, Amount, Denomination},
    wallet::AddressIndex,
    SignOptions,
};
use nostr_rust::{keys::get_random_secret_key, nostr_client::Client as NostrClient, Identity};

use log::debug;
use std::str::FromStr;

use super::utils::{get_input_value, get_output_value, new_rpc_blockchain};

impl Maker {
    pub fn new(
        priv_key: Option<String>,
        relay_urls: Vec<&str>,
        config: &mut MakerConfig,
        blockchain_config: BlockchainConfig,
    ) -> Result<Self, Error> {
        // Nostr config
        let priv_key = match priv_key {
            Some(key) => key,
            None => {
                let (sk, _) = get_random_secret_key();
                hex::encode(sk.as_ref())
            }
        };
        let identity = Identity::from_str(&priv_key)?;

        let nostr_client = NostrClient::new(relay_urls)?;

        // Wallet config
        let blockchain = match blockchain_config {
            BlockchainConfig::RPC(info) => new_rpc_blockchain(info)?,
        };
        let wallet = new_wallet(&blockchain, ("wpkh([8fa88d24/84'/1'/0'/0]tprv8hFqpTAwkZfayVk1bLc65H4Y3qcdcGJfCTntmVS9xnRa3BNXG7k5R6JK75c6z9L8LWUuUzq9kKF3uUaNQJK6gMvCLX4YHYrqcx1Gmd7k5fV/*)".to_string(), "wpkh([8fa88d24/84'/1'/0'/1]tprv8hFqpTAwkZfb1qP4H9AyEUXZzWwGSBDXRSZLrbAyv2UZZYFx2CQftd3aMXW1yLtqNqtM9gut1P5vY86AGJ2EgacpGPWWtCwTFoz3kYmWbBQ/*)".to_string()))?;

        if config.maxsize.is_none() {
            let bal = Amount::from_sat(wallet.get_balance()?.confirmed);
            config.maxsize = Some(bal);
        }

        let maker = Self {
            identity,
            config: config.clone(),
            nostr_client,
            wallet,
            fill_commitment: None,
        };
        Ok(maker)
    }

    pub fn get_eligible_balance(&mut self) -> Result<Amount, Error> {
        let balance = self.wallet.get_balance()?;
        Ok(Amount::from_sat(balance.confirmed))
    }

    pub fn get_inputs(&mut self, fill_offer: &Fill) -> Result<IoAuth, Error> {
        let unspent = self.wallet.list_unspent()?;

        let mut inputs = vec![];
        let mut value: Amount = Amount::ZERO;

        for utxo in &unspent {
            inputs.push((
                utxo.outpoint,
                Some(self.wallet.get_psbt_input(utxo.clone(), None, false)?),
            ));

            value += Amount::from_sat(utxo.txout.value);

            if value >= fill_offer.amount {
                break;
            }
        }

        let coinjoin_address = self.wallet.get_address(AddressIndex::New)?.address;
        let change_address = self.wallet.get_internal_address(AddressIndex::New)?.address;

        let maker_input = IoAuth {
            utxos: inputs,
            coinjoin_address,
            change_address,
            maker_auth_pub: "".to_string(),
            bitcoin_sig: "".to_string(),
        };

        Ok(maker_input)
    }

    pub fn verify_transaction(
        &mut self,
        psbt: &PartiallySignedTransaction,
        send_amount: &Amount,
    ) -> Result<VerifyCJInfo, Error> {
        let (input_value, my_input_value) = get_input_value(&psbt.inputs, &self.wallet)?;
        debug!("Input {}: {}", input_value, my_input_value);
        let tx = psbt.clone().extract_tx();
        let (output_value, my_output_value) = get_output_value(&tx.output, &self.wallet)?;
        debug!("Output: {} {}", output_value, my_output_value);
        let mining_fee = (input_value - output_value).to_signed()?;
        let maker_fee = my_output_value.to_signed()? - my_input_value.to_signed()?;
        debug!("MF: {}", maker_fee);
        let abs_fee_check = maker_fee.ge(&self.config.abs_fee.to_signed()?);
        let fee_as_percent = maker_fee.to_float_in(Denomination::Satoshi)
            / send_amount.to_float_in(Denomination::Satoshi);

        // Verify maker gets >= set fee
        let rel_fee_check = fee_as_percent.ge(&self.config.rel_fee);

        // Max send amount check
        let max_amount_check = match &self.config.maxsize {
            Some(max_size) => send_amount <= max_size,
            None => true,
        };
        debug!("ABD: {}", abs_fee_check);
        debug!("MAX: {}", max_amount_check);
        debug!("rel: {}", rel_fee_check);

        Ok(VerifyCJInfo {
            mining_fee,
            maker_fee,
            verifyed: abs_fee_check
                && rel_fee_check
                && max_amount_check
                && send_amount.ge(&self.config.minsize),
        })
    }
    pub fn sign_psbt(
        &mut self,
        psbt: PartiallySignedTransaction,
    ) -> Result<PartiallySignedTransaction, Error> {
        let mut psbt = psbt;

        self.wallet.sign(&mut psbt, SignOptions::default())?;

        Ok(psbt)
    }

    pub fn publish_signed_psbt(
        &mut self,
        peer_pub_key: &str,
        psbt: PartiallySignedTransaction,
    ) -> Result<(), Error> {
        send_signed_psbt(&self.identity, peer_pub_key, psbt, &mut self.nostr_client)
    }
}
