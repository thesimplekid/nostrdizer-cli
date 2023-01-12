use crate::bitcoincore::{
    types::BitcoinCoreCreditals,
    utils::{
        get_eligible_balance, get_input_value, get_mining_fee, get_output_value, get_unspent,
        sign_tx_hex,
    },
};
use crate::errors::Error;
use crate::podle;
use crate::types::{
    AuthCommitment, CJFee, IoAuth, MaxMineingFee, NostrdizerOffer, TakerConfig, VerifyCJInfo,
};

use bitcoin::{Amount, Denomination, SignedAmount};
use nostr_rust::{keys::get_random_secret_key, nostr_client::Client as NostrClient, Identity};

use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::{
    CreateRawTransactionInput, ListUnspentResultEntry, SignRawTransactionResult,
};

use log::debug;
use std::collections::HashMap;
use std::str::FromStr;
#[cfg(feature = "bitcoincore")]
pub struct Taker {
    pub identity: Identity,
    pub config: TakerConfig,
    pub nostr_client: NostrClient,
    pub rpc_client: RPCClient,
}

impl Taker {
    #[cfg(feature = "bitcoincore")]
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
    ) -> Result<String, Error> {
        let mut outputs = HashMap::new();
        let mut total_maker_fees = Amount::ZERO;
        // REVIEW: Must be a better way to avoid nested map
        let mut inputs = maker_inputs
            .iter()
            .flat_map(|(_offer, input)| {
                input
                    .utxos
                    .iter()
                    .map(|(txid, vout)| CreateRawTransactionInput {
                        txid: *txid,
                        vout: *vout,
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
                    .get_tx_out(&input.0, input.1, Some(false))
                    .unwrap()
                    .unwrap()
                    .value
            });
            outputs.insert(maker_input.coinjoin_address.to_string(), send_amount);

            // Gets a makers offer from Hashmap in order to calculate their required fee
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
        let tx = self
            .rpc_client
            .create_raw_transaction_hex(&inputs, &outputs, None, None)
            .unwrap();

        Ok(tx)
    }

    /// Get unspent UTXOs
    #[cfg(feature = "bitcoincore")]
    pub fn get_unspent(&mut self) -> Result<Vec<ListUnspentResultEntry>, Error> {
        get_unspent(&self.rpc_client)
    }
    /// Sign tx
    #[cfg(feature = "bitcoincore")]
    // TODO: Rework this to a diffrent type
    pub fn sign_transaction(
        &mut self,
        unsigned_tx: &str,
    ) -> Result<SignRawTransactionResult, Error> {
        sign_tx_hex(unsigned_tx, &self.rpc_client)
    }

    /// Broadcast transaction
    #[cfg(feature = "bitcoincore")]
    pub fn broadcast_transaction(
        &mut self,
        final_hex: SignRawTransactionResult,
    ) -> Result<bitcoin::Txid, Error> {
        Ok(self.rpc_client.send_raw_transaction(&final_hex.hex)?)
    }

    /// Taker generate podle
    #[cfg(feature = "bitcoincore")]
    pub fn generate_podle(&self) -> Result<AuthCommitment, Error> {
        // TODO: Get address somewhere else
        let unspent = self.rpc_client.list_unspent(None, None, None, None, None)?;
        let address = unspent[0].clone().address.unwrap();

        let priv_key = self.rpc_client.dump_private_key(&address)?;
        // let priv_key = PrivateKey::from_slice( b"\xf00\x1aD3R\xba\xa9&\xce$\xe3\xf6,\xf3j\xden\x87\x85\xee\xe8\xd4c\xd4C\x80\x1f\x81\x02j\xe9", bitcoin::Network::Regtest).unwrap();

        podle::generate_podle(0, priv_key)
    }

    #[cfg(feature = "bitcoincore")]
    pub fn combine_raw_transaction(&mut self, txs: &[String]) -> Result<String, Error> {
        Ok(self.rpc_client.combine_raw_transaction(txs)?)
    }

    #[cfg(feature = "bitcoincore")]
    pub fn get_eligible_balance(&mut self) -> Result<Amount, Error> {
        get_eligible_balance(&self.rpc_client)
    }

    #[cfg(feature = "bitcoincore")]
    pub fn verify_transaction(
        &mut self,
        unsigned_tx: &str,
        send_amount: &Amount,
    ) -> Result<VerifyCJInfo, Error> {
        let decoded_transaction = &self
            .rpc_client
            .decode_raw_transaction(unsigned_tx, None)
            .unwrap();
        let (input_value, my_input_value) =
            get_input_value(&decoded_transaction.vin, &self.rpc_client)?;
        let (output_value, my_output_value) =
            get_output_value(&decoded_transaction.vout, &self.rpc_client)?;

        let mining_fee = (input_value - output_value).to_signed()?;

        let maker_fee: SignedAmount =
            my_input_value.to_signed()? - my_output_value.to_signed()? - mining_fee;
        let abs_fee_check = maker_fee.lt(&self.config.cj_fee.abs_fee.to_signed()?);
        let fee_as_percent = maker_fee.to_float_in(Denomination::Satoshi)
            / send_amount.to_float_in(Denomination::Satoshi);

        let rel_fee_check = fee_as_percent.lt(&self.config.cj_fee.rel_fee);
        Ok(VerifyCJInfo {
            mining_fee,
            maker_fee,
            verifyed: abs_fee_check
                && rel_fee_check
                && mining_fee.lt(&self.config.mining_fee.abs_fee.to_signed()?),
        })
    }
}
