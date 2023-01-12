use crate::errors::Error;
use crate::types::{
    NostrdizerMessage, NostrdizerMessageKind, NostrdizerMessages, Offer, SignedTransaction,
};

use nostr_rust::{
    nips::nip4::{decrypt, encrypt},
    nostr_client::Client as NostrClient,
    req::ReqFilter,
    Identity,
};

use secp256k1::{SecretKey, XOnlyPublicKey};

use std::str::FromStr;

pub fn get_offers(nostr_client: &mut NostrClient) -> Result<Vec<(String, Offer)>, Error> {
    let filter = ReqFilter {
        ids: None,
        authors: None,
        kinds: Some(vec![10123, 10124]),
        e: None,
        p: None,
        since: None,
        until: None,
        limit: None,
    };

    let mut offers = Vec::new();

    let events = nostr_client.get_events_of(vec![filter])?;
    for event in events {
        let j_event: NostrdizerMessage = serde_json::from_str(&event.content)?;
        if let NostrdizerMessages::Offer(offer) = j_event.event {
            offers.push((event.pub_key, offer));
        }
    }

    Ok(offers.clone())
}

/// Sends signed tx to peer
pub fn send_signed_tx(
    identity: &Identity,
    peer_pub_key: &str,
    tx: &[u8],
    nostr_client: &mut NostrClient,
) -> Result<(), Error> {
    let event = NostrdizerMessage {
        event_type: NostrdizerMessageKind::SignedCJ,
        event: NostrdizerMessages::SignedCJ(SignedTransaction { tx: tx.to_vec() }),
    };

    let encrypt_message = encrypt_message(&identity.secret_key, peer_pub_key, &event)?;
    nostr_client.publish_ephemeral_event(
        identity,
        130,
        &encrypt_message,
        &[vec!["p".to_string(), peer_pub_key.to_string()]],
        0,
    )?;

    Ok(())
}

pub fn encrypt_message(
    sk: &SecretKey,
    pk: &str,
    message: &NostrdizerMessage,
) -> Result<String, Error> {
    let x_pub_key = XOnlyPublicKey::from_str(pk)?;
    Ok(encrypt(sk, &x_pub_key, &serde_json::to_string(&message)?)?)
}

pub fn decrypt_message(
    sk: &SecretKey,
    pk: &str,
    message: &str,
) -> Result<NostrdizerMessage, Error> {
    let x = XOnlyPublicKey::from_str(pk)?;
    Ok(serde_json::from_str(&decrypt(sk, &x, message)?)?)
}
