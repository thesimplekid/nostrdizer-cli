use crate::{
    errors::Error,
    types::{
        BitcoinCoreCreditals, FillOffer, NostrdizerMessage, NostrdizerMessageKind,
        NostrdizerMessages, Offer, Psbt,
    },
    utils,
};

use bitcoin::{Amount, XOnlyPublicKey};

use bitcoincore_rpc_json::{
    FinalizePsbtResult, ListUnspentResultEntry, WalletCreateFundedPsbtResult,
    WalletProcessPsbtResult,
};
use nostr_rust::{
    events::Event, nips::nip4::decrypt, nostr_client::Client as NostrClient, req::ReqFilter,
    Identity,
};

use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use log::info;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;

struct Config {
    max_rel_fee: f32,
    max_abs_fee: u64,
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
        let identity = Identity::from_str(priv_key).unwrap();
        let nostr_client = NostrClient::new(relay_urls).unwrap();
        let rpc_client = RPCClient::new(
            &bitcoin_core_creds.rpc_url,
            Auth::UserPass(
                bitcoin_core_creds.rpc_username,
                bitcoin_core_creds.rpc_password,
            ),
        )
        .unwrap();
        let config = Config {
            // TODO: Get this from config
            max_rel_fee: 0.15,
            max_abs_fee: 5000,
        };
        let taker = Self {
            identity,
            config,
            nostr_client,
            rpc_client,
        };
        Ok(taker)
    }

    pub fn get_eligible_balance(&mut self) -> Result<Amount, Error> {
        utils::get_eligible_balance(&self.rpc_client)
    }

    pub fn get_unspent(&mut self) -> Result<Vec<ListUnspentResultEntry>, Error> {
        utils::get_unspent(&self.rpc_client)
    }

    pub fn get_input_psbt(
        &mut self,
        send_amount: u64,
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
    // Taker
    pub fn get_peer_inputs(&mut self, peer_count: usize) -> Result<HashMap<String, Psbt>, Error> {
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

        let mut peer_inputs = HashMap::new();
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
                        if let NostrdizerMessages::MakerPsbt(maker_input) = j_event.event {
                            // Close subscription to relay
                            peer_inputs.insert(event.pub_key.clone(), maker_input);
                        }
                    }
                }
            }

            if peer_inputs.len() >= peer_count {
                // Close subscription to relay
                return Ok(peer_inputs.clone());
            }
        }
    }

    /// Get input psbt

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
    pub fn get_matching_offers(&mut self, send_amount: u64) -> Result<Vec<(String, Offer)>, Error> {
        // TODO: match should also be based on fee rate
        let offers = self.get_offers()?;
        let mut matching_offers: Vec<(String, Offer)> = offers
            .into_iter()
            .filter(|(_k, offer)| {
                offer.maxsize > send_amount
                    && offer.minsize < send_amount
                    && offer.rel_fee < self.config.max_rel_fee
                    && offer.abs_fee < self.config.max_abs_fee
            })
            .collect();

        // Sorts so lowest fee maker is first
        // Not sure how efficient this is
        matching_offers.sort_by(|a, b| {
            (a.1.rel_fee * send_amount as f32 + a.1.abs_fee as f32)
                .partial_cmp(&(b.1.rel_fee * send_amount as f32 + b.1.abs_fee as f32))
                .unwrap()
        });

        Ok(matching_offers)
    }

    /// Gets current offers
    pub fn get_offers(&mut self) -> Result<Vec<(String, Offer)>, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![99]),
            e: None,
            p: None,
            since: None,
            until: None,
            limit: None,
        };

        let mut offers = Vec::new();

        let events = self.nostr_client.get_events_of(vec![filter])?;
        for event in events {
            let j_event: NostrdizerMessage = serde_json::from_str(&event.content)?;
            if let NostrdizerMessages::Offer(offer) = j_event.event {
                offers.push((event.pub_key, offer));
            }
        }

        Ok(offers.clone())
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

    /// Verify and sign psbt
    pub fn verify_and_sign_psbt(
        &mut self,
        send_amount: u64,
        unsigned_psbt: &str,
    ) -> Result<WalletProcessPsbtResult, Error> {
        log::debug!("Verify {:?}", unsigned_psbt);
        let decoded_transaction = self.rpc_client.decode_psbt(unsigned_psbt).unwrap();
        log::debug!("Decoded");
        let tx = decoded_transaction.tx;
        let input_value = utils::get_my_input_value(tx.vin, &self.rpc_client)?;
        let output_value = utils::get_my_output_value(tx.vout, &self.rpc_client)?;
        info!("Taker is spending {} sats", input_value.to_sat());
        info!("Taker is receiving {} sats", output_value.to_sat());

        let fee = input_value - output_value;

        if fee > Amount::from_sat(self.config.max_abs_fee) {
            panic!()
        }

        let fee_as_percent = fee.to_sat() as f32 / send_amount as f32;

        info!("Relative fee: {}", fee_as_percent);
        // REVIEW: account for mining fee
        if fee_as_percent > self.config.max_rel_fee {
            panic!()
        }

        utils::sign_psbt(unsigned_psbt, &self.rpc_client)
    }

    pub fn broadcast_transaction(
        &mut self,
        final_psbt: FinalizePsbtResult,
    ) -> Result<bitcoin::Txid, Error> {
        log::debug!("{:?}", final_psbt);
        if let Some(final_hex) = final_psbt.hex {
            Ok(self.rpc_client.send_raw_transaction(&final_hex)?)
        } else {
            Err(Error::FailedToBrodcast)
        }
    }
}
