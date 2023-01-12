use super::utils::sign_psbt;
use crate::bitcoincore::{
    types::BitcoinCoreCreditals,
    utils::{get_eligible_balance, get_mining_fee, get_unspent},
};
use crate::errors::Error;
use crate::podle;
use crate::types::{
    AuthCommitment, CJFee, IoAuth, MaxMineingFee, NostrdizerOffer, TakerConfig, VerifyCJInfo,
};

use bitcoin::psbt::PartiallySignedTransaction;
use bitcoin::{Amount, SignedAmount};
use bitcoincore_rpc_json::FinalizePsbtResult;
use nostr_rust::{keys::get_random_secret_key, nostr_client::Client as NostrClient, Identity};

use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::WalletProcessPsbtResult;
use bitcoincore_rpc_json::{CreateRawTransactionInput, ListUnspentResultEntry};

use log::debug;
use std::collections::HashMap;
use std::str::FromStr;

pub struct Taker {
    pub identity: Identity,
    pub config: TakerConfig,
    pub nostr_client: NostrClient,
    pub rpc_client: RPCClient,
}

impl Taker {
    pub fn new(
        priv_key: Option<String>,
        relay_urls: Vec<&str>,
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
            rpc_client,
        };
        Ok(taker)
    }

    /// Gets the taker inputs for CJ transaction
    #[cfg(feature = "bitcoincore")]
    pub fn get_inputs(
        &mut self,
        amount: Amount,
    ) -> Result<(Amount, Vec<CreateRawTransactionInput>), Error> {
        let unspent = self.rpc_client.list_unspent(None, None, None, None, None)?;
        let mut inputs = vec![];
        let mut value: Amount = Amount::ZERO;
        for utxo in unspent {
            let input = CreateRawTransactionInput {
                txid: utxo.txid,
                vout: utxo.vout,
                sequence: None,
            };

            inputs.push(input);
            value += utxo.amount;

            if value >= amount {
                break;
            }
        }

        Ok((value, inputs))
    }

    /// Creates CJ transaction
    #[cfg(feature = "bitcoincore")]
    // Rework this to not use btcocre types
    pub fn create_cj(
        &mut self,
        send_amount: Amount,
        maker_inputs: &Vec<(NostrdizerOffer, IoAuth)>,
    ) -> Result<PartiallySignedTransaction, Error> {
        let mut outputs = HashMap::new();
        let mut total_maker_fees = Amount::ZERO;
        // REVIEW: Must be a better way to avoid nested map
        let mut inputs = maker_inputs
            .iter()
            .flat_map(|(_offer, input)| {
                input
                    .utxos
                    .iter()
                    .map(|outpoint| CreateRawTransactionInput {
                        txid: outpoint.0.txid,
                        vout: outpoint.0.vout,
                        sequence: None,
                    })
                    .collect::<Vec<CreateRawTransactionInput>>()
            })
            .collect::<Vec<CreateRawTransactionInput>>();

        for (offer, maker_input) in maker_inputs {
            // Sums up total value of a makers input UTXOs
            let maker_input_val = maker_input.utxos.iter().fold(Amount::ZERO, |val, input| {
                val + self
                    .rpc_client
                    .get_tx_out(&input.0.txid, input.0.vout, Some(false))
                    .unwrap()
                    .unwrap()
                    .value
            });
            outputs.insert(maker_input.coinjoin_address.to_string(), send_amount);

            let maker_fee = offer.cjfee; // Amount::from_sat(
            let change_value = maker_input_val - send_amount + maker_fee;
            outputs.insert(maker_input.change_address.to_string(), change_value);

            total_maker_fees += maker_fee;
        }
        // Taker inputs
        // TODO: calc fee
        let mining_fee = Amount::from_sat(500);
        let mut taker_inputs = self.get_inputs(send_amount + total_maker_fees + mining_fee)?;
        inputs.append(&mut taker_inputs.1);
        // Taker output
        let taker_cj_out = self.rpc_client.get_new_address(Some("Cj out"), None)?;
        outputs.insert(taker_cj_out.to_string(), send_amount);

        // Taker change output
        // REVIEW:
        // Right now taker change is added here with a dummy amount
        // Then replaced later, so that the fee can be calculated
        // Be better to not have to add then replace
        let taker_change_out = self.rpc_client.get_raw_change_address(None)?;
        outputs.insert(taker_change_out.to_string(), Amount::from_sat(1000));
        let transaction = self
            .rpc_client
            .create_raw_transaction(&inputs, &outputs, None, None)?;

        // Calc change maker should get
        // REVIEW: Not sure this fee calc is correct
        // don't think it included sig size
        let mining_fee = match get_mining_fee(&self.rpc_client) {
            Ok(fee) => {
                let cal_fee =
                    Amount::from_sat((fee.to_sat() as usize * transaction.vsize()) as u64 / 1000);
                if cal_fee > Amount::from_sat(270) {
                    cal_fee
                } else {
                    Amount::from_sat(270)
                }
            }
            Err(_) => Amount::from_sat(500),
        };

        // Calculates taker change
        debug!("Mining fee: {:?} sats", mining_fee.to_sat());
        let taker_change = taker_inputs.0.to_signed()?
            - send_amount.to_signed()?
            - total_maker_fees.to_signed()?
            - mining_fee.to_signed()?;

        if taker_change < Amount::ZERO.to_signed()? {
            return Err(Error::InsufficientFunds);
        }
        // Replaces change output that has been added above
        outputs.insert(taker_change_out.to_string(), taker_change.to_unsigned()?);

        debug!("Inputs {:?}", inputs);
        debug!("Outputs: {:?}", outputs);

        let psbt = self.rpc_client.create_psbt(&inputs, &outputs, None, None)?;

        let psbt = PartiallySignedTransaction::from_str(&psbt).unwrap();

        Ok(psbt)
    }

    /// Get unspent UTXOs
    #[cfg(feature = "bitcoincore")]
    pub fn get_unspent(&mut self) -> Result<Vec<ListUnspentResultEntry>, Error> {
        get_unspent(&self.rpc_client)
    }
    /// Sign tx
    pub fn sign_transaction(
        &mut self,
        unsigned_psbt: &PartiallySignedTransaction,
    ) -> Result<WalletProcessPsbtResult, Error> {
        sign_psbt(unsigned_psbt, &self.rpc_client)
    }

    pub fn join_psbt(
        &mut self,
        psbts: Vec<PartiallySignedTransaction>,
    ) -> Result<PartiallySignedTransaction, Error> {
        let psbts: Vec<String> = psbts.into_iter().map(|p| p.to_string()).collect();
        let result: String;
        if psbts.len() > 1 {
            result = self.rpc_client.join_psbt(&psbts)?;
        } else {
            result = psbts[0].clone();
        }

        Ok(PartiallySignedTransaction::from_str(&result).unwrap())
    }
    pub fn finalize_psbt(&mut self, psbt: &str) -> Result<FinalizePsbtResult, Error> {
        Ok(self.rpc_client.finalize_psbt(psbt, None)?)
    }

    /// Broadcast transaction
    pub fn broadcast_transaction(
        &mut self,
        final_psbt: FinalizePsbtResult,
    ) -> Result<bitcoin::Txid, Error> {
        Ok(self
            .rpc_client
            .send_raw_transaction(&final_psbt.hex.unwrap())?)
    }

    /// Taker generate podle
    pub fn generate_podle(&self) -> Result<AuthCommitment, Error> {
        // TODO: Get address somewhere else
        let unspent = self.rpc_client.list_unspent(None, None, None, None, None)?;
        let address = unspent[0].clone().address.unwrap();

        let priv_key = self.rpc_client.dump_private_key(&address)?;
        // let priv_key = PrivateKey::from_slice( b"\xf00\x1aD3R\xba\xa9&\xce$\xe3\xf6,\xf3j\xden\x87\x85\xee\xe8\xd4c\xd4C\x80\x1f\x81\x02j\xe9", bitcoin::Network::Regtest).unwrap();

        podle::generate_podle(0, priv_key)
    }

    pub fn get_eligible_balance(&mut self) -> Result<Amount, Error> {
        get_eligible_balance(&self.rpc_client)
    }

    pub fn verify_transaction(
        &mut self,
        psbt: PartiallySignedTransaction,
        sign_amount: &Amount,
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
    pub fn sign_psbt(
        unsigned_psbt: &str,
        rpc_client: &RPCClient,
    ) -> Result<WalletProcessPsbtResult, Error> {
        let signed_psbt = rpc_client.wallet_process_psbt(unsigned_psbt, Some(true), None, None)?;
        Ok(signed_psbt)
    }
}
