use crate::errors::Error;
use crate::types::{NostrdizerMessage, NostrdizerMessageKind, NostrdizerMessages, Psbt};
use bitcoin::Amount;
use bitcoincore_rpc::{Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::{
    CreateRawTransactionInput, DecodePsbtResultVin, DecodePsbtResultVout, ListUnspentResultEntry,
    WalletCreateFundedPsbtOptions, WalletCreateFundedPsbtResult, WalletProcessPsbtResult,
};

use nostr_rust::{nostr_client::Client as NostrClient, Identity};

use log::debug;

use std::collections::HashMap;

pub fn get_input_psbt(
    amount: u64,
    fee_rate: Option<Amount>,
    rpc_client: &RPCClient,
) -> Result<WalletCreateFundedPsbtResult, Error> {
    let unspent = rpc_client.list_unspent(None, None, None, None, None)?;
    for utxo in unspent {
        if utxo.amount.to_sat() > amount {
            // label, Address type
            let cj_out_address = rpc_client.get_new_address(Some("CJ out"), None)?;

            let input = CreateRawTransactionInput {
                txid: utxo.txid,
                vout: utxo.vout,
                sequence: None,
            };

            // Outputs
            let mut outputs = HashMap::new();
            outputs.insert(cj_out_address.to_string(), Amount::from_sat(amount));

            let psbt_options = WalletCreateFundedPsbtOptions {
                add_inputs: None,
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
                &[input],
                &outputs,
                None,
                Some(psbt_options),
                None,
            )?;

            return Ok(psbt);
        }
    }
    Err(Error::NoMatchingUtxo)
}

/// Maker sign psbt
pub fn sign_psbt(
    unsigned_psbt: &str,
    rpc_client: &RPCClient,
) -> Result<WalletProcessPsbtResult, Error> {
    let signed_psbt = rpc_client.wallet_process_psbt(unsigned_psbt, Some(true), None, None)?;
    debug!("signed {:?}", signed_psbt);
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

    nostr_client.send_private_message(
        identity,
        peer_pub_key,
        &serde_json::to_string(&event)?,
        0,
    )?;

    debug!("Sent psbt");

    Ok(())
}

/// Gets balance eligible for coinjoin
// Coins with 2 or more confirmations
pub fn get_eligible_balance(rpc_client: &RPCClient) -> Result<u64, Error> {
    Ok(rpc_client.get_balance(Some(2), Some(false))?.to_sat())
}

/// Gets unspent UTXOs
pub fn get_unspent(rpc_client: RPCClient) -> Result<Vec<ListUnspentResultEntry>, Error> {
    println!("Unspent");
    Ok(rpc_client.list_unspent(None, None, None, Some(false), None)?)
}

/// Get the input value of decoded psbt that is mine
pub fn get_my_input_value(
    vin: Vec<DecodePsbtResultVin>,
    rpc_client: &RPCClient,
) -> Result<Amount, Error> {
    let mut input_value: bitcoin::Amount = Amount::ZERO;
    for vin in vin {
        let txid = vin.txid;
        let vout = vin.vout;

        match (txid, vout) {
            (Some(txid), Some(vout)) => {
                println!("{:?} {:?}", txid, vout);
                let tx_out = rpc_client.get_tx_out(&txid, vout, Some(false))?;
                debug!("tx_out: {:?}", tx_out);
                if let Some(tx_out) = tx_out {
                    if let Some(address) = tx_out.script_pub_key.address {
                        debug!("Address: {:?}", address);
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
    vout: Vec<DecodePsbtResultVout>,
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
