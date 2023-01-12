use crate::errors::Error;

use bitcoin::{psbt::PartiallySignedTransaction, Amount};
use bitcoincore_rpc::{Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::{
    GetRawTransactionResultVin, GetRawTransactionResultVout, ListUnspentResultEntry,
    SignRawTransactionResult, WalletProcessPsbtResult,
};

/// Get output value of decoded tx
pub fn get_output_value(
    vout: &[GetRawTransactionResultVout],
    rpc_client: &RPCClient,
) -> Result<(Amount, Amount), Error> {
    let mut my_output_value = Amount::ZERO;
    let mut output_value = Amount::ZERO;
    for vout in vout {
        if let Some(address) = &vout.script_pub_key.address {
            let info = rpc_client.get_address_info(address)?;

            if info.is_mine == Some(true) {
                my_output_value += vout.value;
            }
            output_value += vout.value;
        }
    }

    Ok((output_value, my_output_value))
}

pub fn sign_tx_hex(
    unsigned_tx: &str,
    rpc_client: &RPCClient,
) -> Result<SignRawTransactionResult, Error> {
    Ok(rpc_client.sign_raw_transaction_with_wallet(unsigned_tx, None, None)?)
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
    vin: &[GetRawTransactionResultVin],
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

/// Maker sign psbt
pub fn sign_psbt(
    unsigned_psbt: &PartiallySignedTransaction,
    rpc_client: &RPCClient,
) -> Result<WalletProcessPsbtResult, Error> {
    let signed_psbt =
        rpc_client.wallet_process_psbt(&unsigned_psbt.to_string(), Some(true), None, None)?;
    Ok(signed_psbt)
}
