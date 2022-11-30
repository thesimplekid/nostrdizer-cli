use crate::{
    errors::Error,
    types::{
        FillOffer, MakerConfig, NostrdizerMessage, NostrdizerMessageKind, NostrdizerMessages,
        Offer, Psbt,
    },
    utils,
};
use nostr_rust::{
    events::{Event, EventPrepare},
    nips::nip4::decrypt,
    nostr_client::Client as NostrClient,
    req::ReqFilter,
    utils::get_timestamp,
    Identity,
};

use bitcoin::Amount;
use bitcoin::XOnlyPublicKey;
use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::{WalletCreateFundedPsbtResult, WalletProcessPsbtResult};

use log::{debug, info};

use std::str::FromStr;

use serde_json::Value;

use rand::{thread_rng, Rng};

pub struct Maker {
    pub identity: Identity,
    config: MakerConfig,
    nostr_client: NostrClient,
    rpc_client: RPCClient,
}

impl Maker {
    pub fn new(
        priv_key: &str,
        relay_urls: Vec<&str>,
        config: &mut MakerConfig,
        rpc_url: &str,
    ) -> Result<Self, Error> {
        let identity = Identity::from_str(priv_key).unwrap();

        let nostr_client = NostrClient::new(relay_urls).unwrap();

        let rpc_client = RPCClient::new(
            rpc_url,
            Auth::UserPass("bitcoin".to_string(), "password".to_string()),
        )
        .unwrap();

        if config.maxsize.is_none() {
            let bal = utils::get_eligible_balance(&rpc_client)?;
            config.maxsize = Some(bal);
        }

        let maker = Self {
            identity,
            config: config.clone(),
            nostr_client,
            rpc_client,
        };
        Ok(maker)
    }

    pub fn publish_offer(&mut self) -> Result<(), Error> {
        let mut rng = thread_rng();

        let maxsize = match self.config.maxsize {
            Some(maxsize) => maxsize,
            None => utils::get_eligible_balance(&self.rpc_client)?,
        };

        // TODO: This should be set better
        if maxsize < 5000 {
            return Err(Error::NoMatchingUtxo);
        }

        let offer = Offer {
            offer_id: rng.gen(),
            abs_fee: self.config.abs_fee,
            rel_fee: self.config.rel_fee,
            minsize: self.config.minsize,
            maxsize,
            will_broadcast: self.config.will_broadcast,
        };

        let content = serde_json::to_string(&NostrdizerMessage {
            event_type: NostrdizerMessageKind::Offer,
            event: NostrdizerMessages::Offer(offer),
        })?;

        let event = EventPrepare {
            pub_key: self.identity.public_key_str.clone(),
            created_at: get_timestamp(),
            kind: 99,
            tags: vec![],
            content,
        }
        .to_event(&self.identity, 0);
        debug!("Event: {:?}", event);

        self.nostr_client.publish_event(&event)?;

        Ok(())
    }

    /// Get active offer
    pub fn get_active_offer(&mut self) -> Result<Option<Offer>, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: Some(vec![self.identity.public_key_str.clone()]),
            kinds: Some(vec![93]),
            e: None,
            p: None,
            since: None,
            until: None,
            limit: None,
        };

        if let Ok(events) = self.nostr_client.get_events_of(vec![filter]) {
            if !events.is_empty() {
                let offer_event = events.last().unwrap();

                let j_event: NostrdizerMessage = serde_json::from_str(&offer_event.content)?;
                if let NostrdizerMessages::Offer(offer) = j_event.event {
                    return Ok(Some(offer));
                }
            }
        }
        Ok(None)
    }

    /// Maker waits for fill offer
    pub fn get_fill_offer(&mut self) -> Result<(String, FillOffer), Error> {
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
                        debug!("{:?}", decrypted_content);
                        let j_event: NostrdizerMessage =
                            serde_json::from_str(&decrypted_content).unwrap();
                        if let NostrdizerMessages::FillOffer(fill_offer) = j_event.event {
                            // Close subscription to relay
                            return Ok((event.pub_key, fill_offer));
                        }
                    }
                }
            }
        }
    }

    /// Send maker psbt
    pub fn send_maker_psbt(
        &mut self,
        peer_pub_key: &str,
        offer_id: u32,
        psbt: WalletCreateFundedPsbtResult,
    ) -> Result<(), Error> {
        let psbt = psbt.psbt;
        let message = NostrdizerMessage {
            event_type: NostrdizerMessageKind::MakerPsbt,
            event: NostrdizerMessages::MakerPsbt(Psbt { offer_id, psbt }),
        };
        self.nostr_client.send_private_message(
            &self.identity,
            peer_pub_key,
            &serde_json::to_string(&message)?,
            0,
        )?;

        Ok(())
    }

    /// Maker sign psbt
    pub fn get_unsigned_cj_psbt(&mut self) -> Result<String, Error> {
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
                        if let NostrdizerMessages::UnsignedCJ(unsigned_psbt) = j_event.event {
                            // Close subscription to relay
                            return Ok(unsigned_psbt.psbt);
                        }
                    }
                }
            }
        }
    }

    /// Maker verify and sign Psbt
    pub fn verify_and_sign_psbt(
        &mut self,
        fill_offer: &FillOffer,
        unsigned_psbt: &str,
    ) -> Result<WalletProcessPsbtResult, Error> {
        let decoded_tranaction = self.rpc_client.decode_psbt(unsigned_psbt).unwrap();

        let tx = decoded_tranaction.tx;

        let input_value = utils::get_my_input_value(tx.vin, &self.rpc_client)?;
        let output_value = utils::get_my_output_value(tx.vout, &self.rpc_client)?;
        info!("Maker is spending {} sats", input_value.to_sat());
        info!("Maker is receiving {} sats", output_value.to_sat());

        debug!("Output value {:?}", output_value);

        // NOTE: this assumes rel fee in format .015 for 1.5%
        let rel_fee = (fill_offer.amount as f32 * self.config.rel_fee).floor() as u64;

        let required_fee = Amount::from_sat(self.config.abs_fee + rel_fee);

        // Ensures maker is getting input + their set fee
        if output_value < (input_value + required_fee) {
            panic!();
        }

        if let Some(maxsize) = self.config.maxsize {
            if output_value > Amount::from_sat(maxsize) {
                panic!()
            }
        }

        if output_value < Amount::from_sat(self.config.minsize) {
            panic!()
        }

        utils::sign_psbt(unsigned_psbt, &self.rpc_client)
    }
}
