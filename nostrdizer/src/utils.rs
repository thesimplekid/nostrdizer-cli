use crate::errors::Error;
use crate::types::{
    CJFee, MaxMineingFee, NostrdizerMessage, NostrdizerMessageKind, NostrdizerMessages, Offer,
    SignedTransaction, VerifyCJInfo,
};
use bitcoin::{Amount, Denomination, SignedAmount};
use bitcoincore_rpc::{Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::{
    GetRawTransactionResultVin, GetRawTransactionResultVout, ListUnspentResultEntry,
    SignRawTransactionResult,
};

use nostr_rust::{
    nips::nip4::{decrypt, encrypt},
    nostr_client::Client as NostrClient,
    req::ReqFilter,
    Identity,
};

use log::debug;
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

pub fn sign_tx_hex(
    unsigned_tx: &str,
    rpc_client: &RPCClient,
) -> Result<SignRawTransactionResult, Error> {
    Ok(rpc_client.sign_raw_transaction_with_wallet(unsigned_tx, None, None)?)
}

/// Sends signed tx to peer
pub fn send_signed_tx(
    identity: &Identity,
    peer_pub_key: &str,
    tx: SignRawTransactionResult,
    nostr_client: &mut NostrClient,
) -> Result<(), Error> {
    let event = NostrdizerMessage {
        event_type: NostrdizerMessageKind::SignedCJ,
        event: NostrdizerMessages::SignedCJ(SignedTransaction {
            tx: tx.hex,
            nick_signature: "".to_string(),
        }),
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

/// Gets balance eligible for coinjoin
// Coins with 2 or more confirmations
pub fn get_eligible_balance(rpc_client: &RPCClient) -> Result<Amount, Error> {
    Ok(rpc_client.get_balance(Some(2), Some(false))?)
}

/// Gets unspent UTXOs
pub fn get_unspent(rpc_client: &RPCClient) -> Result<Vec<ListUnspentResultEntry>, Error> {
    Ok(rpc_client.list_unspent(None, None, None, Some(false), None)?)
}

/// Get mining fee to get into the next block
pub fn get_mining_fee(rpc_client: &RPCClient) -> Result<Amount, Error> {
    let fee = rpc_client.estimate_smart_fee(1, None)?;

    if let Some(fee) = fee.fee_rate {
        Ok(fee)
    } else {
        Err(Error::FeeEstimation)
    }
}

/// Get the input value of decoded tx
pub fn get_input_value(
    vin: Vec<GetRawTransactionResultVin>,
    rpc_client: &RPCClient,
) -> Result<(Amount, Amount), Error> {
    let mut my_input_value: bitcoin::Amount = Amount::ZERO;
    let mut input_value = Amount::ZERO;
    for vin in vin {
        let txid = vin.txid;
        let vout = vin.vout;

        match (txid, vout) {
            (Some(txid), Some(vout)) => {
                let tx_out = rpc_client.get_tx_out(&txid, vout, Some(false))?;
                if let Some(tx_out) = tx_out {
                    if let Some(address) = tx_out.script_pub_key.address {
                        let add_info = rpc_client.get_address_info(&address)?;
                        if add_info.is_mine == Some(true) {
                            my_input_value += tx_out.value;
                        }
                        input_value += tx_out.value;
                    }
                }
            }
            _ => panic!(),
        }
    }

    Ok((input_value, my_input_value))
}

/// Get output value of decoded tx
pub fn get_output_value(
    vout: Vec<GetRawTransactionResultVout>,
    rpc_client: &RPCClient,
) -> Result<(Amount, Amount), Error> {
    let mut my_output_value = Amount::ZERO;
    let mut output_value = Amount::ZERO;
    for vout in vout {
        if let Some(address) = vout.script_pub_key.address {
            let info = rpc_client.get_address_info(&address)?;

            if info.is_mine == Some(true) {
                my_output_value += vout.value;
            }
            output_value += vout.value;
        }
    }

    Ok((output_value, my_output_value))
}
pub enum Role {
    Maker(CJFee, Amount, Option<Amount>),
    Taker(CJFee, MaxMineingFee),
}

pub fn verify_transaction(
    unsigned_tx: &str,
    send_amount: Amount,
    role: Role,
    rpc_client: &RPCClient,
) -> Result<VerifyCJInfo, Error> {
    let decoded_transaction = rpc_client
        .decode_raw_transaction(unsigned_tx, None)
        .unwrap();
    let (input_value, my_input_value) = get_input_value(decoded_transaction.vin, rpc_client)?;
    let (output_value, my_output_value) = get_output_value(decoded_transaction.vout, rpc_client)?;

    let mining_fee = (input_value - output_value).to_signed()?;

    match role {
        Role::Maker(cj_fee, min_size, max_size) => {
            let maker_fee = my_output_value.to_signed()? - my_input_value.to_signed()?;
            let abs_fee_check = maker_fee.ge(&cj_fee.abs_fee.to_signed()?);
            let fee_as_percent = maker_fee.to_float_in(Denomination::Satoshi)
                / send_amount.to_float_in(Denomination::Satoshi);

            // Verify maker gets > set fee
            let rel_fee_check = fee_as_percent.ge(&cj_fee.rel_fee);

            // Max send amount check
            let max_amount_check = match max_size {
                Some(max_size) => send_amount <= max_size,
                None => true,
            };

            Ok(VerifyCJInfo {
                mining_fee,
                maker_fee,
                verifyed: abs_fee_check
                    && rel_fee_check
                    && max_amount_check
                    && send_amount.ge(&min_size),
            })
        }
        Role::Taker(cj_fee, max_mineing_fee) => {
            let maker_fee: SignedAmount =
                my_input_value.to_signed()? - my_output_value.to_signed()? - mining_fee;
            let abs_fee_check = maker_fee.lt(&cj_fee.abs_fee.to_signed()?);
            let fee_as_percent = maker_fee.to_float_in(Denomination::Satoshi)
                / send_amount.to_float_in(Denomination::Satoshi);

            let rel_fee_check = fee_as_percent.lt(&cj_fee.rel_fee);
            Ok(VerifyCJInfo {
                mining_fee,
                maker_fee,
                verifyed: abs_fee_check
                    && rel_fee_check
                    && mining_fee.lt(&max_mineing_fee.abs_fee.to_signed()?),
            })
        }
    }
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
