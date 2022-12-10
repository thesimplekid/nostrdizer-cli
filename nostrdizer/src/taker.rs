use crate::{
    errors::Error,
    types::{
        BitcoinCoreCreditals, CJFee, FillOffer, MakerInput, MaxMineingFee, NostrdizerMessage,
        NostrdizerMessageKind, NostrdizerMessages, Offer, Psbt, VerifyCJInfo,
    },
    utils,
};

use bitcoin::{Amount, Denomination, XOnlyPublicKey};

use bitcoincore_rpc_json::{
    CreateRawTransactionInput, FinalizePsbtResult, ListUnspentResultEntry,
    WalletCreateFundedPsbtResult, WalletProcessPsbtResult,
};
use nostr_rust::{
    events::Event, nips::nip4::decrypt, nostr_client::Client as NostrClient, req::ReqFilter,
    Identity,
};

use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use log::debug;
use log::info;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;

struct Config {
    cj_fee: CJFee,
    mining_fee: MaxMineingFee, // max_rel_fee: f64,
                               // max_abs_fee: Amount,
}

pub struct Taker {
    pub identity: Identity,
    config: Config,
    nostr_client: NostrClient,
    rpc_client: RPCClient,
}

impl Taker {
    pub fn new(
        priv_key: &str,
        relay_urls: Vec<&str>,
        bitcoin_core_creds: BitcoinCoreCreditals,
    ) -> Result<Self, Error> {
        let identity = Identity::from_str(priv_key)?;
        let nostr_client = NostrClient::new(relay_urls)?;
        let rpc_client = RPCClient::new(
            &bitcoin_core_creds.rpc_url,
            Auth::UserPass(
                bitcoin_core_creds.rpc_username,
                bitcoin_core_creds.rpc_password,
            ),
        )?;
        let config = Config {
            // TODO: Get this from config
            cj_fee: CJFee {
                rel_fee: 0.30,
                abs_fee: Amount::from_sat(5000),
            },
            mining_fee: MaxMineingFee {
                abs_fee: Amount::from_sat(5000),
                rel_fee: 0.20,
            },
        };
        let taker = Self {
            identity,
            config,
            nostr_client,
            rpc_client,
        };
        Ok(taker)
    }

    /// Get balance eligible (2 confirmations) for CJ
    pub fn get_eligible_balance(&mut self) -> Result<Amount, Error> {
        utils::get_eligible_balance(&self.rpc_client)
    }

    /// Get unspent UTXOs
    pub fn get_unspent(&mut self) -> Result<Vec<ListUnspentResultEntry>, Error> {
        utils::get_unspent(&self.rpc_client)
    }

    pub fn get_input_psbt(
        &mut self,
        send_amount: Amount,
        fee_rate: Option<Amount>,
    ) -> Result<WalletCreateFundedPsbtResult, Error> {
        utils::get_input_psbt(send_amount, fee_rate, &self.rpc_client)
    }

    /// Gets signed peer psbts
    pub fn get_signed_peer_psbts(&mut self, peer_count: usize) -> Result<String, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![4]),
            e: None,
            p: Some(vec![self.identity.public_key_str.clone()]),
            since: None,
            until: None,
            limit: None,
        };

        let subcription_id = self.nostr_client.subscribe(vec![filter])?;

        let mut peer_signed_psbts = HashMap::new();
        loop {
            let data = self.nostr_client.next_data()?;
            for (_, message) in data {
                let event: Value = serde_json::from_str(&message.to_string())?;

                if event[0] == "EOSE" && event[1].as_str() == Some(&subcription_id) {
                    break;
                }

                if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                    if event.kind == 4 && event.tags[0].contains(&self.identity.public_key_str) {
                        // TODO: This can prob be collapsed
                        let x = XOnlyPublicKey::from_str(&event.pub_key)?;
                        let decrypted_content =
                            decrypt(&self.identity.secret_key, &x, &event.content)?;
                        let j_event: NostrdizerMessage = serde_json::from_str(&decrypted_content)?;
                        if let NostrdizerMessages::SignedCJ(signed_psbt) = j_event.event {
                            // Close subscription to relay
                            peer_signed_psbts.insert(event.pub_key.to_string(), signed_psbt);

                            if peer_signed_psbts.len() >= peer_count {
                                let psbts: Vec<String> =
                                    peer_signed_psbts.values().map(|p| p.psbt.clone()).collect();

                                let combined_psbt = self.rpc_client.combine_psbt(&psbts)?;

                                return Ok(combined_psbt);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Gets peer maker inputs from relay
    pub fn get_peer_inputs(
        &mut self,
        peer_count: usize,
    ) -> Result<Vec<(String, MakerInput)>, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![4]),
            e: None,
            p: Some(vec![self.identity.public_key_str.clone()]),
            since: None,
            until: None,
            limit: None,
        };

        let subcription_id = &self.nostr_client.subscribe(vec![filter])?;

        let mut peer_inputs = vec![];
        loop {
            let data = &self.nostr_client.next_data()?;
            for (_, message) in data {
                let event: Value = serde_json::from_str(&message.to_string())?;

                if event[0] == "EOSE" && event[1].as_str() == Some(subcription_id) {
                    break;
                }

                if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                    if event.kind == 4 && event.tags[0].contains(&self.identity.public_key_str) {
                        // TODO: This can prob be collapsed
                        let x = XOnlyPublicKey::from_str(&event.pub_key)?;
                        let decrypted_content =
                            decrypt(&self.identity.secret_key, &x, &event.content)?;
                        let j_event: NostrdizerMessage = serde_json::from_str(&decrypted_content)?;
                        if let NostrdizerMessages::MakerInputs(maker_input) = j_event.event {
                            peer_inputs.push((event.pub_key.clone(), maker_input));
                        }
                    }
                }
            }

            if peer_inputs.len() >= peer_count {
                return Ok(peer_inputs.clone());
            }
        }
    }

    /// Gets the taker inputs for CJ transaction
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
    pub fn create_cj(
        &mut self,
        send_amount: Amount,
        maker_inputs: Vec<(String, MakerInput)>,
        maker_offers: HashMap<String, Offer>,
    ) -> Result<String, Error> {
        let mut inputs = vec![];
        let mut outputs = HashMap::new();
        let mut total_maker_fees = Amount::ZERO;
        for (maker, maker_input) in maker_inputs {
            for input in maker_input.inputs.clone() {
                let raw_input = CreateRawTransactionInput {
                    txid: input.0,
                    vout: input.1,
                    sequence: None,
                };
                inputs.push(raw_input);
            }

            let maker_input_val = maker_input.inputs.iter().fold(Amount::ZERO, |val, input| {
                val + self
                    .rpc_client
                    .get_tx_out(&input.0, input.1, Some(false))
                    .unwrap()
                    .unwrap()
                    .value
            });
            outputs.insert(maker_input.cj_out_address.to_string(), send_amount);

            let maker_offer = maker_offers.get(&maker).unwrap();
            let maker_fee = Amount::from_sat(
                (send_amount.to_float_in(Denomination::Satoshi) * maker_offer.rel_fee).ceil()
                    as u64,
            ) + maker_offer.abs_fee;
            total_maker_fees += maker_fee;
            let change_value = maker_input_val - send_amount + maker_fee;

            outputs.insert(maker_input.change_address.to_string(), change_value);
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
        let taker_change_out = self.rpc_client.get_raw_change_address(None)?;
        outputs.insert(taker_change_out.to_string(), Amount::from_sat(1000));
        let transaction = self
            .rpc_client
            .create_raw_transaction(&inputs, &outputs, None, None)?;

        // Calc change maker should get
        // REVIEW: Not sure this fee calc is correct
        // don't think it included sig size
        let mining_fee = match utils::get_mining_fee(&self.rpc_client) {
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
        debug!("Mining fee: {:?} sats", mining_fee.to_sat());
        let taker_change = taker_inputs.0 - send_amount - total_maker_fees - mining_fee;
        outputs.insert(taker_change_out.to_string(), taker_change);

        debug!("inputs {:?}", inputs);
        debug!("Outputs: {:?}", outputs);
        let psbt = self
            .rpc_client
            .create_psbt(&inputs, &outputs, None, None)
            .unwrap();

        Ok(psbt)
    }

    /// Send fill offer from taker to maker
    pub fn send_fill_offer_message(
        &mut self,
        fill_offer: FillOffer,
        peer_pub_key: &str,
    ) -> Result<(), Error> {
        let message = &NostrdizerMessage {
            event_type: NostrdizerMessageKind::FillOffer,
            event: NostrdizerMessages::FillOffer(fill_offer),
        };

        self.nostr_client.send_private_message(
            &self.identity,
            peer_pub_key,
            &serde_json::to_string(&message)?,
            0,
        )?;

        Ok(())
    }

    /// Get offers that match send sorted for lowest fee first
    pub fn get_matching_offers(
        &mut self,
        send_amount: Amount,
    ) -> Result<Vec<(String, Offer)>, Error> {
        // TODO: match should also be based on fee rate
        let offers = self.get_offers()?;
        let mut matching_offers: Vec<(String, Offer)> = offers
            .into_iter()
            .filter(|(_k, offer)| {
                offer.maxsize > send_amount
                    && offer.minsize < send_amount
                    && offer.rel_fee < self.config.cj_fee.rel_fee
                    && offer.abs_fee < self.config.cj_fee.abs_fee
            })
            .collect();

        // Sorts so lowest fee maker is first
        // Not sure how efficient this is
        matching_offers.sort_by(|a, b| {
            (a.1.rel_fee * send_amount.to_float_in(Denomination::Satoshi)
                + a.1.abs_fee.to_float_in(Denomination::Satoshi))
            .partial_cmp(
                &(b.1.rel_fee * send_amount.to_float_in(Denomination::Satoshi)
                    + b.1.abs_fee.to_float_in(Denomination::Satoshi)),
            )
            .unwrap()
        });

        Ok(matching_offers)
    }

    /// Gets current offers
    pub fn get_offers(&mut self) -> Result<Vec<(String, Offer)>, Error> {
    utils::get_offers(&mut self.nostr_client)
    }

    /// Publish unsigned psbt to relay
    pub fn send_unsigned_psbt(
        &mut self,
        peer_pub_key: &str,
        offer_id: u32,
        psbt: &str,
    ) -> Result<(), Error> {
        let message = NostrdizerMessage {
            event_type: NostrdizerMessageKind::UnsignedCJ,
            event: NostrdizerMessages::UnsignedCJ(Psbt {
                offer_id,
                psbt: psbt.to_string(),
            }),
        };

        self.nostr_client.send_private_message(
            &self.identity,
            peer_pub_key,
            &serde_json::to_string(&message)?,
            0,
        )?;

        Ok(())
    }

    pub fn join_psbt(&mut self, psbts: Vec<String>) -> Result<String, Error> {
        Ok(self.rpc_client.join_psbt(&psbts)?)
    }

    pub fn finalize_psbt(&mut self, psbt: &str) -> Result<FinalizePsbtResult, Error> {
        Ok(self.rpc_client.finalize_psbt(psbt, None)?)
    }

    /// Verify that taker does not pay more the set fee for CJ
    pub fn verify_psbt(
        &mut self,
        send_amount: Amount,
        unsigned_psbt: &str,
    ) -> Result<VerifyCJInfo, Error> {
        let cj_fee = CJFee {
            abs_fee: self.config.cj_fee.abs_fee,
            rel_fee: self.config.cj_fee.rel_fee,
        };

        let mining_fee = MaxMineingFee {
            abs_fee: self.config.mining_fee.abs_fee,
            rel_fee: self.config.mining_fee.rel_fee,
        };

        utils::verify_psbt(
            unsigned_psbt,
            send_amount,
            utils::Role::Taker(cj_fee, mining_fee),
            &self.rpc_client,
        )
    }

    /// Sign psbt
    pub fn sign_psbt(&mut self, unsigned_psbt: &str) -> Result<WalletProcessPsbtResult, Error> {
        utils::sign_psbt(unsigned_psbt, &self.rpc_client)
    }

    /// Broadcast transaction
    pub fn broadcast_transaction(
        &mut self,
        final_psbt: FinalizePsbtResult,
    ) -> Result<bitcoin::Txid, Error> {
        if let Some(final_hex) = final_psbt.hex {
            Ok(self.rpc_client.send_raw_transaction(&final_hex)?)
        } else {
            Err(Error::FailedToBrodcast)
        }
    }
}
