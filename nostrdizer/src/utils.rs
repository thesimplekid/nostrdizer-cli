use crate::errors::Error;
use crate::types::{
    CJFee, MaxMineingFee, NostrdizerMessage, NostrdizerMessageKind, NostrdizerMessages, Offer,
    Psbt, VerifyCJInfo,
};
use bitcoin::{Amount, Denomination, SignedAmount};
use bitcoincore_rpc::{Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::{
    CreateRawTransactionInput, GetRawTransactionResultVin, GetRawTransactionResultVout,
    ListUnspentResultEntry, WalletCreateFundedPsbtOptions, WalletCreateFundedPsbtResult,
    WalletProcessPsbtResult,
};

use nostr_rust::{
    nips::nip4::{decrypt, encrypt},
    nostr_client::Client as NostrClient,
    req::ReqFilter,
    Identity,
};

use log::debug;
use secp256k1::SecretKey;

use std::collections::HashMap;
use std::str::FromStr;

pub fn get_offers(nostr_client: &mut NostrClient) -> Result<Vec<(String, Offer)>, Error> {
    let filter = ReqFilter {
        ids: None,
        authors: None,
        kinds: Some(vec![10124]),
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

pub fn get_input_psbt(
    amount: Amount,
    fee_rate: Option<Amount>,
    rpc_client: &RPCClient,
) -> Result<WalletCreateFundedPsbtResult, Error> {
    let unspent = rpc_client.list_unspent(None, None, None, None, None)?;
    debug!("List unspent: {:?}", unspent);
    let mut inputs: Vec<CreateRawTransactionInput> = vec![];
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

    if value >= amount {
        // label, Address type
        let cj_out_address = rpc_client.get_new_address(Some("CJ out"), None)?;

        // Outputs
        let mut outputs = HashMap::new();
        outputs.insert(cj_out_address.to_string(), amount);

        let psbt_options = WalletCreateFundedPsbtOptions {
            add_inputs: Some(true),
            change_address: None,
            change_position: None,
            change_type: None,
            include_watching: None,
            lock_unspent: None,
            fee_rate,
            subtract_fee_from_outputs: vec![],
            replaceable: Some(false),
            conf_target: None,
            estimate_mode: None,
        };
        let psbt = rpc_client.wallet_create_funded_psbt(
            &inputs,
            &outputs,
            None,
            Some(psbt_options),
            None,
        )?;

        return Ok(psbt);
    }
    Err(Error::NoMatchingUtxo)
}

/// Maker sign psbt
pub fn sign_psbt(
    unsigned_psbt: &str,
    rpc_client: &RPCClient,
) -> Result<WalletProcessPsbtResult, Error> {
    let signed_psbt = rpc_client.wallet_process_psbt(unsigned_psbt, Some(true), None, None)?;
    Ok(signed_psbt)
}

/// Sends signed psbt to peer
pub fn send_signed_psbt(
    identity: &Identity,
    peer_pub_key: &str,
    offer_id: u32,
    psbt: WalletProcessPsbtResult,
    nostr_client: &mut NostrClient,
) -> Result<(), Error> {
    let event = NostrdizerMessage {
        event_type: NostrdizerMessageKind::SignedCJ,
        event: NostrdizerMessages::SignedCJ(Psbt {
            offer_id,
            psbt: psbt.psbt,
        }),
    };

    let encrypt_message = encrypt_message(&identity.secret_key, peer_pub_key, &event)?;

    nostr_client.publish_ephemeral_event(
        identity,
        128,
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

/// Get the input value of decoded psbt that is mine
pub fn get_my_input_value(
    vin: Vec<GetRawTransactionResultVin>,
    rpc_client: &RPCClient,
) -> Result<Amount, Error> {
    let mut input_value: bitcoin::Amount = Amount::ZERO;
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
                            input_value += tx_out.value;
                        }
                    }
                }
            }
            _ => panic!(),
        }
    }

    Ok(input_value)
}

/// Get output value of decoded psbt that is is_mine
pub fn get_my_output_value(
    vout: Vec<GetRawTransactionResultVout>,
    rpc_client: &RPCClient,
) -> Result<Amount, Error> {
    let mut output_value = Amount::ZERO;
    for vout in vout {
        if let Some(address) = vout.script_pub_key.address {
            let info = rpc_client.get_address_info(&address)?;

            if info.is_mine == Some(true) {
                output_value += vout.value;
            }
        }
    }

    Ok(output_value)
}
pub enum Role {
    Maker(CJFee, Amount, Option<Amount>),
    Taker(CJFee, MaxMineingFee),
}

pub fn verify_psbt(
    unsigned_psbt: &str,
    send_amount: Amount,
    role: Role,
    rpc_client: &RPCClient,
) -> Result<VerifyCJInfo, Error> {
    let decoded_transaction = rpc_client.decode_psbt(unsigned_psbt).unwrap();
    let tx = decoded_transaction.tx;
    let input_value = get_my_input_value(tx.vin, rpc_client)?;
    let output_value = get_my_output_value(tx.vout, rpc_client)?;

    let mining_fee = decoded_transaction
        .fee
        .unwrap_or(Amount::ZERO)
        .to_signed()?;

    match role {
        Role::Maker(cj_fee, min_size, max_size) => {
            let maker_fee = output_value.to_signed()? - input_value.to_signed()?;
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
                input_value.to_signed()? - output_value.to_signed()? - mining_fee;
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
    let x_pub_key = secp256k1::XOnlyPublicKey::from_str(pk)?;
    Ok(encrypt(sk, &x_pub_key, &serde_json::to_string(&message)?)?)
}
