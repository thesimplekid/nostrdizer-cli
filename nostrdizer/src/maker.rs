use crate::{
    errors::Error,
    types::{
        BitcoinCoreCreditals, CJFee, FillOffer, MakerInput, NostrdizerMessage,
        NostrdizerMessageKind, NostrdizerMessages, Offer, VerifyCJInfo,
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

use bitcoin::XOnlyPublicKey;
use bitcoin::{Amount, Denomination};
use bitcoincore_rpc::{Auth, Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::{WalletCreateFundedPsbtResult, WalletProcessPsbtResult};

use log::{debug, info};

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

    pub fn publish_offer(&mut self) -> Result<Offer, Error> {
        let mut rng = thread_rng();

        let maxsize = match self.config.maxsize {
            Some(maxsize) => maxsize,
            None => utils::get_eligible_balance(&self.rpc_client)?,
        };

        // TODO: This should be set better
        if maxsize < Amount::from_sat(5000) {
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

        self.nostr_client
            .publish_replaceable_event(&self.identity, 124, &content, &[], 0)?;

        Ok(offer)
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
    pub fn get_fill_offer(&mut self) -> Result<(String, FillOffer), Error> {
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
                let event: Value = serde_json::from_str(&message.to_string())?;

                if event[0] == "EOSE" && event[1].as_str() == Some(&subcription_id) {
                    break;
                }

                if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                    if event.kind == 20125 && event.tags[0].contains(&self.identity.public_key_str)
                    {
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
            if get_timestamp().gt(&(time + 600)) {
                self.publish_offer()?;
                time = get_timestamp();
            }
        }
    }

    pub fn get_input_psbt(
        &mut self,
        send_amount: Amount,
        fee_rate: Option<Amount>,
    ) -> Result<WalletCreateFundedPsbtResult, Error> {
        utils::get_input_psbt(send_amount, fee_rate, &self.rpc_client)
    }

    /// Gets maker input for CJ
    pub fn get_inputs(&mut self, fill_offer: &FillOffer) -> Result<MakerInput, Error> {
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

        let cj_out_address = self.rpc_client.get_new_address(Some("CJ out"), None)?;
        debug!("Maker cj out: {}", cj_out_address);

        let change_address = self.rpc_client.get_raw_change_address(None).unwrap();
        debug!("Maker change out: {}", change_address);

        let maker_input = MakerInput {
            offer_id: fill_offer.offer_id,
            inputs,
            cj_out_address,
            change_address,
        };

        Ok(maker_input)
    }

    /// Send maker input
    pub fn send_maker_input(
        &mut self,
        peer_pub_key: &str,
        maker_input: MakerInput,
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

    /// Maker sign psbt
    pub fn get_unsigned_cj_psbt(&mut self) -> Result<String, Error> {
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

        let subcription_id = self.nostr_client.subscribe(vec![filter])?;

        let started_waiting = get_timestamp();
        loop {
            let data = self.nostr_client.next_data()?;
            for (_, message) in data {
                let event: Value = serde_json::from_str(&message.to_string())?;

                if event[0] == "EOSE" && event[1].as_str() == Some(&subcription_id) {
                    break;
                }
                if let Ok(event) = serde_json::from_value::<Event>(event[2].clone()) {
                    if event.kind == 20127 && event.tags[0].contains(&self.identity.public_key_str)
                    {
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
            if started_waiting.gt(&(started_waiting + 300)) {
                return Err(Error::TakerFailedToSendTransaction);
            }
        }
    }

    /// Maker verify and sign Psbt
    pub fn verify_psbt(
        &mut self,
        fill_offer: &FillOffer,
        unsigned_psbt: &str,
    ) -> Result<VerifyCJInfo, Error> {
        let cj_fee = CJFee {
            abs_fee: self.config.abs_fee,
            rel_fee: self.config.rel_fee,
        };

        utils::verify_psbt(
            unsigned_psbt,
            fill_offer.amount,
            utils::Role::Maker(cj_fee, self.config.minsize, self.config.maxsize),
            &self.rpc_client,
        )
    }

    /// Sign psbt
    pub fn sign_psbt(&mut self, unsigned_psbt: &str) -> Result<WalletProcessPsbtResult, Error> {
        utils::sign_psbt(unsigned_psbt, &self.rpc_client)
    }

    pub fn send_signed_psbt(
        &mut self,
        peer_pub_key: &str,
        fill_offer: FillOffer,
        signed_psbt: &WalletProcessPsbtResult,
    ) -> Result<(), Error> {
        Ok(utils::send_signed_psbt(
            &self.identity,
            peer_pub_key,
            fill_offer.offer_id,
            signed_psbt.clone(),
            &mut self.nostr_client,
        )
        .unwrap())
    }
}
