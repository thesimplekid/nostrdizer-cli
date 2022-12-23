use crate::{
    errors::Error,
    types::{
        AbsOffer, BitcoinCoreCreditals, CJFee, Fill, IoAuth, NostrdizerMessage,
        NostrdizerMessageKind, NostrdizerMessages, Offer, RelOffer, VerifyCJInfo,
    },
    utils::{self, decrypt_message},
};
use nostr_rust::{
    events::Event, nostr_client::Client as NostrClient, req::ReqFilter, utils::get_timestamp,
    Identity,
};

use bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::SignRawTransactionResult;

use log::debug;

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use rand::{thread_rng, Rng};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub abs_fee: Amount,
    pub rel_fee: f64,
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub minsize: Amount,
    #[serde(default, with = "bitcoin::util::amount::serde::as_btc::opt")]
    pub maxsize: Option<Amount>,
    pub will_broadcast: bool,
}

pub struct Maker {
    pub identity: Identity,
    config: Config,
    nostr_client: NostrClient,
    rpc_client: RPCClient,
}

impl Maker {
    pub fn new(
        priv_key: &str,
        relay_urls: Vec<&str>,
        config: &mut Config,
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
            // TODO:
            nick_signature: "".to_string(),
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
            nick_signature: "".to_string(),
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
            kinds: Some(vec![10124]),
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
            kinds: Some(vec![10124]),
            e: None,
            p: None,
            since: None,
            until: None,
            limit: None,
        };

        if let Ok(events) = self.nostr_client.get_events_of(vec![filter]) {
            if !events.is_empty() {
                let offer_event = events.last().unwrap();
                let event_id = &offer_event.id;
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
            kinds: Some(vec![20125]),
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
                        if event.kind == 20125
                            && event.tags[0].contains(&self.identity.public_key_str)
                        {
                            if let NostrdizerMessages::Fill(fill_offer) = decrypt_message(
                                &self.identity.secret_key,
                                &event.pub_key,
                                &event.content,
                            )?
                            .event
                            {
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

    /// Gets maker input for CJ
    pub fn get_inputs(&mut self, fill_offer: &Fill) -> Result<IoAuth, Error> {
        let unspent = self.rpc_client.list_unspent(None, None, None, None, None)?;
        let mut inputs = vec![];
        let mut value: Amount = Amount::ZERO;
        for utxo in unspent {
            let input = (utxo.txid, utxo.vout);

            inputs.push(input);
            value += utxo.amount;

            if value >= fill_offer.amount {
                break;
            }
        }

        let coinjoin_address = self.rpc_client.get_new_address(Some("CJ out"), None)?;
        debug!("Maker cj out: {}", coinjoin_address);

        let change_address = self.rpc_client.get_raw_change_address(None).unwrap();
        debug!("Maker change out: {}", change_address);

        let maker_input = IoAuth {
            utxos: inputs,
            coinjoin_address,
            change_address,
            maker_auth_pub: "".to_string(),
            bitcoin_sig: "".to_string(),
            nick_signature: "".to_string(),
        };

        Ok(maker_input)
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

        self.nostr_client.publish_ephemeral_event(
            &self.identity,
            126,
            &encypted_content,
            &[vec!["p".to_string(), peer_pub_key.to_string()]],
            0,
        )?;

        Ok(())
    }

    /// Maker waits for unsigned CJ transaction
    pub fn get_unsigned_cj_transaction(&mut self) -> Result<String, Error> {
        let filter = ReqFilter {
            ids: None,
            authors: None,
            kinds: Some(vec![20127]),
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
                        if event.kind == 20127
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
                                return Ok(unsigned_tx_hex.tx);
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

    /// Maker verify and sign tx
    pub fn verify_transaction(
        &mut self,
        fill_offer: &Fill,
        unsigned_tx: &str,
    ) -> Result<VerifyCJInfo, Error> {
        let cj_fee = CJFee {
            abs_fee: self.config.abs_fee,
            rel_fee: self.config.rel_fee,
        };

        utils::verify_transaction(
            unsigned_tx,
            fill_offer.amount,
            utils::Role::Maker(cj_fee, self.config.minsize, self.config.maxsize),
            &self.rpc_client,
        )
    }

    /// Sign tx hex
    pub fn sign_tx_hex(
        &mut self,
        unsigned_tx_hex: &str,
    ) -> Result<SignRawTransactionResult, Error> {
        utils::sign_tx_hex(unsigned_tx_hex, &self.rpc_client)
    }

    /// Send signed tx back to taker
    pub fn send_signed_tx(
        &mut self,
        peer_pub_key: &str,
        signed_tx: &SignRawTransactionResult,
    ) -> Result<(), Error> {
        utils::send_signed_tx(
            &self.identity,
            peer_pub_key,
            signed_tx.clone(),
            &mut self.nostr_client,
        )?;
        Ok(())
    }
}
