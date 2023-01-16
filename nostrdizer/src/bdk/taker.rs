use super::utils::{
    get_input_value, get_output_value, get_unspent, new_rpc_blockchain, new_wallet,
};
use crate::errors::Error;
use crate::types::{
    AuthCommitment, BlockchainConfig, CJFee, IoAuth, MaxMineingFee, NostrdizerOffer, TakerConfig,
    VerifyCJInfo, DUST, MAX_FEE,
};
use bdk::bitcoin::consensus::encode::{deserialize, serialize, serialize_hex};
use bdk::blockchain::{AnyBlockchain, Blockchain};
use bdk::miniscript::descriptor::Pkh;
use bdk::miniscript::Descriptor;
use bdk::wallet::{tx_builder::TxOrdering, AddressIndex};
use bdk::KeychainKind;
use bdk::{database::AnyDatabase, Wallet};
use bdk::{LocalUtxo, SignOptions};
use bitcoin::psbt::PartiallySignedTransaction;
use bitcoin::{Amount, Denomination, SignedAmount};
use log::info;
use nostr_rust::{keys::get_random_secret_key, nostr_client::Client as NostrClient, Identity};
use std::collections::HashMap;
use std::io;
use std::str::FromStr;

pub struct Taker {
    pub identity: Identity,
    pub config: TakerConfig,
    pub nostr_client: NostrClient,
    pub wallet: Wallet<AnyDatabase>,
    pub blockchain: AnyBlockchain,
}
impl Taker {
    pub fn new(
        priv_key: Option<String>,
        relay_urls: Vec<&str>,
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
        let wallet = new_wallet(&blockchain, ("wpkh([5515da09/84'/1'/0'/0]tprv8iaP6UkRRJHpphe7CX866hvMp9JzLtzPiYG9CvHb2opUWfPtQSwjLsMnYxc3YD9iScG6ENBQTBkBgwnwURUdb996ij5aDTWz91xC1iVLKbS/*)".to_string(), "wpkh([5515da09/84'/1'/0'/1]tprv8iaP6UkRRJHpsiKQ7xzapBNpWiwYbWh9RE1UUWGJL94RGtxtDXWZHF7WWcyDdYPmMJkYwTEXHGRTRynSBVdPKSkEN8GZJeaZpWqzcTnvPrU/*)".to_string()))?;

        let config = TakerConfig {
            // TODO: Get this from config
            cj_fee: CJFee {
                rel_fee: 0.30,
                abs_fee: Amount::from_sat(10000),
            },
            mining_fee: MaxMineingFee {
                abs_fee: Amount::from_sat(10000),
                rel_fee: 0.20,
            },
            minium_makers: 1,
        };
        let taker = Self {
            identity,
            config,
            nostr_client,
            wallet,
            blockchain,
        };
        Ok(taker)
    }

    pub fn get_eligible_balance(&self) -> Result<Amount, Error> {
        let balance = self.wallet.get_balance()?;
        Ok(Amount::from_sat(balance.confirmed))
    }
    pub fn get_unspent(&self) -> Result<Vec<LocalUtxo>, Error> {
        get_unspent(&self.wallet)
    }

    /// Taker genrate podle
    pub fn generate_podle(&self) -> Result<AuthCommitment, Error> {
        let unspent = self.wallet.list_unspent();

        //self.wallet.get_descriptor_for_keychain(keychain)
        todo!()
    }

    pub fn combine_psbts(
        &self,
        psbts: &[PartiallySignedTransaction],
    ) -> Result<PartiallySignedTransaction, Error> {
        /*
        Function is slightly modified from https://github.com/bitcoindevkit/bdk-cli/blob/master/src/handlers.rs
        Used under MIT License
        Copyright (c) 2020-2021 Bitcoin Dev Kit Developers
         */
        let mut psbts = psbts.to_vec();

        // TODO: Handle the error
        let init_psbt = psbts.pop().ok_or_else(|| panic!()).unwrap();
        let final_psbt = psbts
            .into_iter()
            .try_fold::<_, _, Result<PartiallySignedTransaction, Error>>(
                init_psbt,
                |mut acc, x| {
                    acc.combine(x).unwrap();
                    Ok(acc)
                },
            )?;
        Ok(final_psbt)
    }

    pub fn create_cj(
        &mut self,
        send_amount: Amount,
        maker_inputs: &[(NostrdizerOffer, IoAuth)],
    ) -> Result<PartiallySignedTransaction, Error> {
        let (psbt, details) = {
            let mut builder = self.wallet.build_tx();
            builder.ordering(TxOrdering::Untouched);
            // Add maker cj out
            builder.add_recipient(
                self.wallet
                    .get_address(AddressIndex::New)
                    .unwrap()
                    .address
                    .script_pubkey(),
                send_amount.to_sat(),
            );
            for (offer, io_auth) in maker_inputs {
                // Adds maker CJ out
                let script = io_auth.coinjoin_address.script_pubkey();

                // Checks that inputs are p2wpkh
                if !script.is_v0_p2wpkh() {
                    return Err(Error::BadInput);
                }
                builder.add_recipient(script, send_amount.to_sat());

                let mut maker_input_value = 0;
                // Add Maker inputs
                for (outpoint, input) in &io_auth.utxos {
                    // REVIEW: This really shouldn't be an option
                    // Its only an option to work with bitcoincore
                    // But that makes BDK and bitcoin core incompatible if done like this
                    if let Some(input) = input {
                        // Technically this should be done on the descriptor of the foreign utxo
                        // In this case where its a coinjoin where all are same descriptor i think its okay to do it here
                        let satisfaction_weight = self
                            .wallet
                            .get_descriptor_for_keychain(KeychainKind::External)
                            .max_satisfaction_weight()
                            .unwrap();
                        builder
                            .add_foreign_utxo(*outpoint, input.clone(), satisfaction_weight)
                            .unwrap();

                        maker_input_value += input.witness_utxo.as_ref().unwrap().value;
                    }
                }
                let maker_fee = offer.cjfee.to_sat();
                let change_value = maker_input_value - send_amount.to_sat() + maker_fee;

                // Add maker change
                if change_value.gt(&DUST) {
                    builder.add_recipient(io_auth.change_address.script_pubkey(), change_value);
                }
            }
            builder.finish().unwrap()
        };

        // Check transaction details to make sure not spending too much
        Ok(psbt)
    }

    pub fn verify_transaction(
        &mut self,
        psbt: &PartiallySignedTransaction,
        send_amount: &Amount,
    ) -> Result<VerifyCJInfo, Error> {
        let (input_value, my_input_value) = get_input_value(&psbt.inputs, &self.wallet)?;

        let tx = psbt.clone().extract_tx();
        let (output_value, my_output_value) = get_output_value(&tx.output, &self.wallet)?;
        let mining_fee = (input_value - output_value).to_signed()?;

        // Calculate total maker fee
        let maker_fee: SignedAmount =
            my_input_value.to_signed()? - my_output_value.to_signed()? - mining_fee;
        let abs_fee_check = maker_fee.lt(&self.config.cj_fee.abs_fee.to_signed()?);
        let fee_as_percent = maker_fee.to_float_in(Denomination::Satoshi)
            / send_amount.to_float_in(Denomination::Satoshi);

        info!("Spending: {}", my_input_value);
        info!("Receiving: {}", my_output_value);

        match input_value
            .checked_sub(output_value)
            .map(|val| {
                val.gt(&Amount::from_sat(
                    (send_amount.to_sat() as f32 * MAX_FEE).floor() as u64,
                ))
            })
            .unwrap_or(true)
        {
            true => return Err(Error::FeesTooHigh),
            false => (),
        }

        let rel_fee_check = fee_as_percent.lt(&self.config.cj_fee.rel_fee);
        Ok(VerifyCJInfo {
            mining_fee,
            maker_fee,
            verifyed: abs_fee_check
                && rel_fee_check
                && mining_fee.lt(&self.config.mining_fee.abs_fee.to_signed()?),
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

    pub fn broadcast_transaction(&mut self, psbt: PartiallySignedTransaction) -> Result<(), Error> {
        Ok(self.blockchain.broadcast(&psbt.extract_tx())?)
    }
}
