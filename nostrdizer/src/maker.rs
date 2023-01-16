use crate::{
    errors::Error,
    podle,
    types::{
        AbsOffer, Amount, AuthCommitment, Fill, IoAuth, NostrdizerMessage, NostrdizerMessageKind,
        NostrdizerMessages, Offer, Pubkey, RelOffer, ABS_OFFER, AUTH, FILL, IOAUTH, REL_OFFER,
        TRANSACTION,
    },
    utils::{self, decrypt_message},
};

use bdk::bitcoin::psbt::PartiallySignedTransaction;
use nostr_rust::{
    events::{Event, EventPrepare},
    req::ReqFilter,
    utils::get_timestamp,
};

use serde_json::Value;

#[cfg(all(feature = "bitcoincore", not(feature = "bdk")))]
use crate::bitcoincore::maker::Maker;

#[cfg(all(feature = "bdk", not(feature = "bitcoincore")))]
use crate::bdk::maker::Maker;

use rand::{thread_rng, Rng};

impl Maker {
    pub fn publish_offer(&mut self) -> Result<(), Error> {
        let mut rng = thread_rng();

        let maxsize = match self.config.maxsize {
            Some(maxsize) => maxsize,
            None => self.get_eligible_balance()?,
        };

        // TODO: This should be set better
        if maxsize < Amount::from_sat(5000) {
            return Err(Error::NoMatchingUtxo);
        }
        // Publish Relative Offer
        let offer = RelOffer {
            offer_id: rng.gen(),
            cjfee: self.config.rel_fee,
            minsize: self.config.minsize,
            maxsize,
            txfee: Amount::ZERO,
        };

        let content = serde_json::to_string(&NostrdizerMessage {
            event_type: NostrdizerMessageKind::Offer,
            event: NostrdizerMessages::Offer(Offer::RelOffer(offer)),
        })?;

        self.nostr_client
            .publish_replaceable_event(&self.identity, 124, &content, &[], 0)?;

        // Publish Absolute Offer
        let offer = AbsOffer {
            offer_id: rng.gen(),
            cjfee: self.config.abs_fee,
            minsize: self.config.minsize,
            maxsize,
            txfee: Amount::ZERO,
            // TODO:
        };
        let content = serde_json::to_string(&NostrdizerMessage {
            event_type: NostrdizerMessageKind::Offer,
            event: NostrdizerMessages::Offer(Offer::AbsOffer(offer)),
        })?;

        self.nostr_client
            .publish_replaceable_event(&self.identity, 123, &content, &[], 0)?;

        Ok(())
    }

    /// Get active offer
    pub fn get_active_offer(&mut self) -> Result<Option<Offer>, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: Some(vec![self.identity.public_key_str.clone()]),
            kinds: Some(vec![REL_OFFER]),
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

    /// Maker delete active offer
    pub fn delete_active_offer(&mut self) -> Result<(), Error> {
        let filter = ReqFilter {
            ids: None,
            authors: Some(vec![self.identity.public_key_str.clone()]),
            kinds: Some(vec![REL_OFFER, ABS_OFFER]),
            e: None,
            p: None,
            since: None,
            until: None,
            limit: None,
        };

        if let Ok(events) = self.nostr_client.get_events_of(vec![filter]) {
            for event in events {
                let event_id = &event.id;
                self.nostr_client
                    .delete_event(&self.identity, event_id, 0)?;
            }
        }
        Ok(())
    }

    /// Maker waits for fill offer
    pub fn get_fill_offer(&mut self) -> Result<(String, Fill), Error> {
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![FILL]),
            e: None,
            p: Some(vec![self.identity.public_key_str.clone()]),
            since: None,
            until: None,
            limit: None,
        };

        let subcription_id = self.nostr_client.subscribe(vec![filter])?;
        let mut time = get_timestamp();
        loop {
            let data = self.nostr_client.next_data()?;
            for (_, message) in data {
                if let Ok(event) = serde_json::from_str::<Value>(&message.to_string()) {
                    if event[0] == "EOSE" && event[1].as_str() == Some(&subcription_id) {
                        break;
                    }

                    if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                        if event.kind == FILL
                            && event.tags[0].contains(&self.identity.public_key_str)
                        {
                            if let NostrdizerMessages::Fill(fill_offer) = decrypt_message(
                                &self.identity.secret_key,
                                &event.pub_key,
                                &event.content,
                            )?
                            .event
                            {
                                // TODO: Verify commitment in fill offer
                                self.fill_commitment = Some(fill_offer.commitment);
                                return Ok((event.pub_key, fill_offer));
                            }
                        }
                    }
                }
            }
            if get_timestamp().gt(&(time + 600)) {
                self.publish_offer()?;
                time = get_timestamp();
            }
        }
    }

    pub fn get_commitment_auth(&mut self) -> Result<AuthCommitment, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![AUTH]),
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
                            && event.kind == AUTH
                            && event.tags[0].contains(&self.identity.public_key_str)
                        {
                            if let NostrdizerMessages::Auth(auth_commitment) = decrypt_message(
                                &self.identity.secret_key,
                                &event.pub_key,
                                &event.content,
                            )?
                            .event
                            {
                                self.nostr_client.unsubscribe(&subscription_id)?;
                                return Ok(auth_commitment);
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

    /// Maker verify podle
    pub fn verify_podle(&self, auth_commitment: AuthCommitment) -> Result<(), Error> {
        podle::verify_podle(0, auth_commitment, self.fill_commitment.unwrap())
    }

    /// Send maker input
    pub fn send_maker_input(
        &mut self,
        peer_pub_key: &str,
        maker_input: IoAuth,
    ) -> Result<(), Error> {
        let message = NostrdizerMessage {
            event_type: NostrdizerMessageKind::MakerPsbt,
            event: NostrdizerMessages::MakerInputs(maker_input),
        };

        let encypted_content =
            utils::encrypt_message(&self.identity.secret_key, peer_pub_key, &message)?;

        let event = EventPrepare {
            pub_key: self.identity.public_key_str.clone(),
            created_at: get_timestamp(),
            kind: IOAUTH,
            tags: vec![vec!["p".to_string(), peer_pub_key.to_string()]],
            content: encypted_content,
        }
        .to_event(&self.identity, 0);

        self.nostr_client.publish_event(&event)?;

        /*
        self.nostr_client.publish_ephemeral_event(
            &self.identity,
            IOAUTH,
            &encypted_content,
            &[vec!["p".to_string(), peer_pub_key.to_string()]],
            0,
        )?;
        */

        Ok(())
    }

    /// Send pubkey message
    /// This is a dumby message for now
    pub fn send_pubkey(&mut self, peer_pub_key: &str) -> Result<(), Error> {
        let message = NostrdizerMessage {
            event_type: NostrdizerMessageKind::MakerPubkey,
            event: NostrdizerMessages::PubKey(Pubkey {
                mencpubkey: "".to_string(),
            }),
        };

        let encrypted_content =
            utils::encrypt_message(&self.identity.secret_key, peer_pub_key, &message)?;

        self.nostr_client.publish_ephemeral_event(
            &self.identity,
            126,
            &encrypted_content,
            &[vec!["p".to_string(), peer_pub_key.to_string()]],
            0,
        )?;

        Ok(())
    }

    /// Maker waits for unsigned CJ transaction
    pub fn get_unsigned_cj_transaction(&mut self) -> Result<PartiallySignedTransaction, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![TRANSACTION]),
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
                            && event.kind == TRANSACTION
                            && event.tags[0].contains(&self.identity.public_key_str)
                        {
                            if let NostrdizerMessages::UnsignedCJ(unsigned_tx_hex) =
                                decrypt_message(
                                    &self.identity.secret_key,
                                    &event.pub_key,
                                    &event.content,
                                )?
                                .event
                            {
                                self.nostr_client.unsubscribe(&subscription_id)?;
                                return Ok(unsigned_tx_hex.psbt);
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
}
