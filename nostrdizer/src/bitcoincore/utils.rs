use crate::errors::Error;
use crate::types::{Role, VerifyCJInfo};

use bitcoin::{Amount, Denomination, SignedAmount};
use bitcoincore_rpc::{Client as RPCClient, RpcApi};
use bitcoincore_rpc_json::{
    GetRawTransactionResultVin, GetRawTransactionResultVout, ListUnspentResultEntry,
    SignRawTransactionResult,
};

/// Get output value of decoded tx
#[cfg(feature = "bitcoincore")]
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

#[cfg(feature = "bitcoincore")]
pub fn sign_tx_hex(
    unsigned_tx: &str,
    rpc_client: &RPCClient,
) -> Result<SignRawTransactionResult, Error> {
    Ok(rpc_client.sign_raw_transaction_with_wallet(unsigned_tx, None, None)?)
}

#[cfg(feature = "bitcoincore")]
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

/// Gets balance eligible for coinjoin
// Coins with 2 or more confirmations
#[cfg(feature = "bitcoincore")]
pub fn get_eligible_balance(rpc_client: &RPCClient) -> Result<Amount, Error> {
    Ok(rpc_client.get_balance(Some(2), Some(false))?)
}

/// Gets unspent UTXOs
#[cfg(feature = "bitcoincore")]
pub fn get_unspent(rpc_client: &RPCClient) -> Result<Vec<ListUnspentResultEntry>, Error> {
    Ok(rpc_client.list_unspent(None, None, None, Some(false), None)?)
}

/// Get mining fee to get into the next block
#[cfg(feature = "bitcoincore")]
pub fn get_mining_fee(rpc_client: &RPCClient) -> Result<Amount, Error> {
    let fee = rpc_client.estimate_smart_fee(1, None)?;

    if let Some(fee) = fee.fee_rate {
        Ok(fee)
    } else {
        Err(Error::FeeEstimation)
    }
}

/// Get the input value of decoded tx
#[cfg(feature = "bitcoincore")]
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
