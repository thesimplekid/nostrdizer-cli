use crate::{
    errors::Error,
    types::{
        BitcoinCoreCreditals, CJFee, Fill, IoAuth, MaxMineingFee, NostrdizerMessage,
        NostrdizerMessageKind, NostrdizerMessages, NostrdizerOffer, Offer, Psbt, VerifyCJInfo,
    },
    utils::{self, decrypt_message},
};

use bitcoin::{Amount, Denomination};

use bitcoincore_rpc_json::{
    CreateRawTransactionInput, FinalizePsbtResult, ListUnspentResultEntry, WalletProcessPsbtResult,
};
use nostr_rust::{
    events::Event, nostr_client::Client as NostrClient, req::ReqFilter, utils::get_timestamp,
    Identity,
};

use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use log::{debug, info};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

struct Config {
    cj_fee: CJFee,
    mining_fee: MaxMineingFee,
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
                abs_fee: Amount::from_sat(10000),
            },
            mining_fee: MaxMineingFee {
                abs_fee: Amount::from_sat(10000),
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

    /// Gets signed peer psbts
    pub fn get_signed_peer_psbts(&mut self, peer_count: usize) -> Result<String, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![20128]),
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
                if let Ok(event) = serde_json::from_str::<Value>(&message.to_string()) {
                    if event[0] == "EOSE" && event[1].as_str() == Some(&subcription_id) {
                        break;
                    }

                    if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                        if event.kind == 20128
                            && event.tags[0].contains(&self.identity.public_key_str)
                        {
                            if let NostrdizerMessages::SignedCJ(signed_psbt) = decrypt_message(
                                &self.identity.secret_key,
                                &event.pub_key,
                                &event.content,
                            )?
                            .event
                            {
                                peer_signed_psbts.insert(event.pub_key.to_string(), signed_psbt);

                                if peer_signed_psbts.len() >= peer_count {
                                    let psbts: Vec<String> = peer_signed_psbts
                                        .values()
                                        .map(|p| p.psbt.clone())
                                        .collect();

                                    let combined_psbt = self.rpc_client.combine_psbt(&psbts)?;

                                    return Ok(combined_psbt);
                                }
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
        send_amount: Amount,
        peer_count: usize,
        matching_offers: &mut Vec<NostrdizerOffer>,
    ) -> Result<Vec<(NostrdizerOffer, IoAuth)>, Error> {
        // Sorts vec by lowest CJ fee
        matching_offers.sort_by_key(|o| o.cjfee);
        // Removes dupicate maker offers
        let unique_makers: HashSet<String> =
            matching_offers.iter().map(|o| o.clone().maker).collect();
        matching_offers.retain(|o| unique_makers.contains(&o.maker));

        let mut last_peer = 0;
        for peer in matching_offers.iter_mut() {
            //debug!("Peer: {:?} Offer: {:?}", peer.0, peer.1);
            self.send_fill_offer_message(
                Fill {
                    offer_id: peer.oid,
                    amount: send_amount,
                    tencpubkey: "".to_string(),
                    commitment: "".to_string(),
                    nick_signature: "".to_string(),
                },
                &peer.maker,
            )?;
            last_peer += 1;
            if last_peer >= peer_count {
                break;
            }
        }

        // subscribe to maker inputs
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![20126]),
            e: None,
            p: Some(vec![self.identity.public_key_str.clone()]),
            since: None,
            until: None,
            limit: None,
        };

        let subcription_id = &self.nostr_client.subscribe(vec![filter])?;

        let mut peer_inputs = vec![];
        // Get time stamp that waiting started
        let mut started_waiting = get_timestamp();
        loop {
            let data = &self.nostr_client.next_data()?;
            for (_, message) in data {
                if let Ok(event) = serde_json::from_str::<Value>(&message.to_string()) {
                    if event[0] == "EOSE" && event[1].as_str() == Some(subcription_id) {
                        break;
                    }

                    if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                        if event.kind == 20126
                            && event.tags[0].contains(&self.identity.public_key_str)
                        {
                            if let NostrdizerMessages::MakerInputs(maker_input) = decrypt_message(
                                &self.identity.secret_key,
                                &event.pub_key,
                                &event.content,
                            )?
                            .event
                            {
                                peer_inputs.push((
                                    matching_offers
                                        .clone()
                                        .iter()
                                        .find(|o| o.maker == event.pub_key)
                                        .unwrap()
                                        .clone(),
                                    maker_input,
                                ));
                            }
                        }
                    }
                }

                if peer_inputs.len() >= peer_count {
                    return Ok(peer_inputs);
                }
                // Check if time waited is more then set
                // Send fill offer to the next x peers
                // Where x is peers responded - peers required
                // Reset started waiting time
                if started_waiting + 15 > get_timestamp() {
                    // TODO: Check if there are any matching makers left
                    let num_failed_to_respond = peer_count - peer_inputs.len();
                    if num_failed_to_respond > matching_offers.len() - last_peer {
                        return Err(Error::NotEnoughMakers);
                    }

                    info!(
                        "{} makers did not respond, sending to new makers",
                        num_failed_to_respond
                    );

                    for _i in 0..num_failed_to_respond {
                        let peer = &matching_offers[last_peer];
                        self.send_fill_offer_message(
                            Fill {
                                offer_id: peer.oid,
                                amount: send_amount,
                                tencpubkey: "".to_string(),
                                commitment: "".to_string(),
                                nick_signature: "".to_string(),
                            },
                            &peer.maker,
                        )?;
                        last_peer += 1;
                    }
                    started_waiting = get_timestamp();
                }
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
        let psbt = self
            .rpc_client
            .create_psbt(&inputs, &outputs, None, None)
            .unwrap();

        Ok(psbt)
    }

    /// Send fill offer from taker to maker
    pub fn send_fill_offer_message(
        &mut self,
        fill_offer: Fill,
        peer_pub_key: &str,
    ) -> Result<(), Error> {
        let message = &NostrdizerMessage {
            event_type: NostrdizerMessageKind::FillOffer,
            event: NostrdizerMessages::Fill(fill_offer),
        };

        let encypted_content =
            utils::encrypt_message(&self.identity.secret_key, peer_pub_key, message)?;

        self.nostr_client.publish_ephemeral_event(
            &self.identity,
            125,
            &encypted_content,
            &[vec!["p".to_string(), peer_pub_key.to_string()]],
            0,
        )?;

        Ok(())
    }

    /// Get offers that match send sorted for lowest fee first
    pub fn get_matching_offers(
        &mut self,
        send_amount: Amount,
    ) -> Result<Vec<NostrdizerOffer>, Error> {
        let offers = self.get_offers()?;
        let matching_offers = offers
            .into_iter()
            .filter(|(_k, offer)| match offer {
                Offer::AbsOffer(offer) => {
                    offer.maxsize > send_amount
                        && offer.minsize < send_amount
                        && offer.cjfee < self.config.cj_fee.abs_fee
                }
                Offer::RelOffer(offer) => {
                    offer.maxsize > send_amount
                        && offer.minsize < send_amount
                        && offer.cjfee < self.config.cj_fee.rel_fee
                }
            })
            .map(|(k, offer)| match offer {
                Offer::AbsOffer(offer) => NostrdizerOffer {
                    maker: k,
                    oid: offer.oid,
                    txfee: offer.txfee,
                    cjfee: offer.cjfee,
                },
                Offer::RelOffer(offer) => {
                    let cjfee = (offer.cjfee * send_amount.to_float_in(Denomination::Satoshi))
                        .floor() as u64;
                    NostrdizerOffer {
                        maker: k,
                        oid: offer.oid,
                        txfee: offer.txfee,
                        cjfee: Amount::from_sat(cjfee),
                    }
                }
            })
            .collect();

        Ok(matching_offers)
    }

    /// Gets current offers
    pub fn get_offers(&mut self) -> Result<Vec<(String, Offer)>, Error> {
        utils::get_offers(&mut self.nostr_client)
    }

    /// Publish unsigned cj psbt to relay
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

        let encypted_content =
            utils::encrypt_message(&self.identity.secret_key, peer_pub_key, &message)?;

        self.nostr_client.publish_ephemeral_event(
            &self.identity,
            127,
            &encypted_content,
            &[vec!["p".to_string(), peer_pub_key.to_string()]],
            0,
        )?;

        Ok(())
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
