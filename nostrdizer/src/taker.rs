use crate::{
    errors::Error,
    types::{
        AuthCommitment, CJFee, Fill, IoAuth, MaxMineingFee, NostrdizerMessage,
        NostrdizerMessageKind, NostrdizerMessages, NostrdizerOffer, Offer, Role, Taker,
        Transaction, VerifyCJInfo,
    },
    utils::{self, decrypt_message},
};

use bitcoin::{Amount, Denomination};

use nostr_rust::{events::Event, req::ReqFilter, utils::get_timestamp};

use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;

impl Taker {
    /// Get balance eligible (2 confirmations) for CJ
    pub fn get_eligible_balance(&mut self) -> Result<Amount, Error> {
        utils::get_eligible_balance(&self.rpc_client)
    }

    // TODO: This doesnt actually do anything
    pub fn get_maker_pubkey(&mut self) -> Result<(), Error> {
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

        let subscription_id = self.nostr_client.subscribe(vec![filter])?;

        let started_waiting = get_timestamp();
        loop {
            let data = self.nostr_client.next_data()?;
            for (_, message) in data {
                if let Ok(event) = serde_json::from_str::<Value>(&message.to_string()) {
                    if event[0] == "EOSE" && event[1].as_str() == Some(&subscription_id) {
                        break;
                    }
                    if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                        if event.verify().is_ok()
                            && event.kind == 20126
                            && event.tags[0].contains(&self.identity.public_key_str)
                        {
                            if let NostrdizerMessages::PubKey(_pubkey) = decrypt_message(
                                &self.identity.secret_key,
                                &event.pub_key,
                                &event.content,
                            )?
                            .event
                            {
                                self.nostr_client.unsubscribe(&subscription_id)?;
                                return Ok(());
                            }
                        }
                    }
                }
            }
            if started_waiting.gt(&(started_waiting + 300)) {
                return Err(Error::TakerFailedToSendTransaction);
            }
        }
    }

    /// Gets signed peer tx
    pub fn get_signed_peer_transaction(&mut self, peer_count: usize) -> Result<String, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![20130]),
            e: None,
            p: Some(vec![self.identity.public_key_str.clone()]),
            since: None,
            until: None,
            limit: None,
        };

        let subcription_id = self.nostr_client.subscribe(vec![filter])?;

        let mut peer_signed_transaction = HashMap::new();
        loop {
            let data = self.nostr_client.next_data()?;
            for (_, message) in data {
                if let Ok(event) = serde_json::from_str::<Value>(&message.to_string()) {
                    if event[0] == "EOSE" && event[1].as_str() == Some(&subcription_id) {
                        break;
                    }

                    if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                        if event.verify().is_ok()
                            && event.kind == 20130
                            && event.tags[0].contains(&self.identity.public_key_str)
                        {
                            if let NostrdizerMessages::SignedCJ(signed_tx) = decrypt_message(
                                &self.identity.secret_key,
                                &event.pub_key,
                                &event.content,
                            )?
                            .event
                            {
                                peer_signed_transaction
                                    .insert(event.pub_key.to_string(), signed_tx);

                                if peer_signed_transaction.len() >= peer_count {
                                    let txs: Vec<String> = peer_signed_transaction
                                        .values()
                                        .map(|p| hex::encode(p.tx.clone()))
                                        .collect();

                                    let combined_transaction =
                                        self.combine_raw_transaction(&txs)?;

                                    return Ok(combined_transaction);
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
        peer_count: usize,
        matching_offers: Vec<NostrdizerOffer>,
    ) -> Result<Vec<(NostrdizerOffer, IoAuth)>, Error> {
        // subscribe to maker inputs
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

        let subcription_id = &self.nostr_client.subscribe(vec![filter])?;

        let mut peer_inputs = vec![];
        // Get time stamp that waiting started
        let started_waiting = get_timestamp();
        loop {
            let data = &self.nostr_client.next_data()?;
            for (_, message) in data {
                if let Ok(event) = serde_json::from_str::<Value>(&message.to_string()) {
                    if event[0] == "EOSE" && event[1].as_str() == Some(subcription_id) {
                        break;
                    }

                    if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                        if event.verify().is_ok()
                            && event.kind == 20128
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
                                    // Finds the peers matching offer
                                    // pushes (offer, input)
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
                // TODO: Change this to time out and then be > then min makers
                if peer_inputs.len() >= peer_count {
                    return Ok(peer_inputs);
                }
                if get_timestamp() - started_waiting > 60 {
                    if peer_inputs.len() > self.config.minium_makers {
                        return Ok(peer_inputs);
                    } else {
                        return Err(Error::MakersFailedToRespond);
                    }
                }
            }
        }
    }

    /// Send fill offer from taker to maker
    pub fn send_fill_offer_message(
        &mut self,
        send_amount: Amount,
        peer_count: usize,
        matching_offers: &mut Vec<NostrdizerOffer>,
    ) -> Result<Vec<NostrdizerOffer>, Error> {
        // Sorts vec by lowest CJ fee
        matching_offers.sort_by_key(|o| o.cjfee);
        // Removes dupicate maker offers
        let unique_makers: HashSet<String> =
            matching_offers.iter().map(|o| o.clone().maker).collect();
        matching_offers.retain(|o| unique_makers.contains(&o.maker));

        let mut last_peer = 0;
        let commitment = self.generate_podle()?;
        let commitment = commitment.commit; // sha256::Hash::hash(commitment.p2.to_string().as_bytes());

        let mut matched_peers = vec![];
        for peer in matching_offers.iter_mut() {
            //debug!("Peer: {:?} Offer: {:?}", peer.0, peer.1);
            let fill_offer = Fill {
                offer_id: peer.oid,
                amount: send_amount,
                tencpubkey: "".to_string(),
                commitment,
            };
            let message = NostrdizerMessage {
                event_type: NostrdizerMessageKind::FillOffer,
                event: NostrdizerMessages::Fill(fill_offer),
            };
            let encypted_content =
                utils::encrypt_message(&self.identity.secret_key, &peer.maker, &message)?;

            self.nostr_client.publish_ephemeral_event(
                &self.identity,
                125,
                &encypted_content,
                &[vec!["p".to_string(), peer.maker.to_string()]],
                0,
            )?;
            matched_peers.push(peer.clone());
            last_peer += 1;
            if last_peer >= peer_count {
                break;
            }
        }

        Ok(matched_peers)
    }

    pub fn send_auth_message(
        &mut self,
        auth_commitment: AuthCommitment,
        matched_offers: Vec<NostrdizerOffer>,
    ) -> Result<(), Error> {
        let message = NostrdizerMessage {
            event_type: NostrdizerMessageKind::Auth,
            event: NostrdizerMessages::Auth(auth_commitment),
        };

        for offer in matched_offers {
            let encypted_content =
                utils::encrypt_message(&self.identity.secret_key, &offer.maker, &message)?;

            self.nostr_client.publish_ephemeral_event(
                &self.identity,
                127,
                &encypted_content,
                &[vec!["p".to_string(), offer.maker]],
                0,
            )?;
        }
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
                    oid: offer.offer_id,
                    txfee: offer.txfee,
                    cjfee: offer.cjfee,
                },
                Offer::RelOffer(offer) => {
                    let cjfee = (offer.cjfee * send_amount.to_float_in(Denomination::Satoshi))
                        .floor() as u64;
                    NostrdizerOffer {
                        maker: k,
                        oid: offer.offer_id,
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

    /// Publish unsigned cj transaction to relay
    pub fn send_unsigned_transaction(
        &mut self,
        peer_pub_key: &str,
        tx_hex: &str,
    ) -> Result<(), Error> {
        let message = NostrdizerMessage {
            event_type: NostrdizerMessageKind::UnsignedCJ,
            event: NostrdizerMessages::UnsignedCJ(Transaction {
                tx: tx_hex.to_string(),
            }),
        };

        let encypted_content =
            utils::encrypt_message(&self.identity.secret_key, peer_pub_key, &message)?;

        self.nostr_client.publish_ephemeral_event(
            &self.identity,
            129,
            &encypted_content,
            &[vec!["p".to_string(), peer_pub_key.to_string()]],
            0,
        )?;

        Ok(())
    }

    /// Verify that taker does not pay more the set fee for CJ
    pub fn verify_transaction(
        &mut self,
        send_amount: Amount,
        unsigned_tx: &str,
    ) -> Result<VerifyCJInfo, Error> {
        let cj_fee = CJFee {
            abs_fee: self.config.cj_fee.abs_fee,
            rel_fee: self.config.cj_fee.rel_fee,
        };

        let mining_fee = MaxMineingFee {
            abs_fee: self.config.mining_fee.abs_fee,
            rel_fee: self.config.mining_fee.rel_fee,
        };

        utils::verify_transaction(
            unsigned_tx,
            send_amount,
            Role::Taker(cj_fee, mining_fee),
            &self.rpc_client,
        )
    }
}
