use bdk::bitcoin::util::amount::ParseAmountError;
use nostr_rust::nips::{nip16::NIP16Error, nip9::NIP9Error};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[cfg(feature = "bitcoincore")]
    #[error("Bitcoin Rpc error: {}", _0)]
    BitcoinRpcError(bitcoincore_rpc::Error),

    #[error("No matching utxos")]
    NoMatchingUtxo,

    #[error("Serde error")]
    SerdeError(serde_json::Error),

    #[error("Nostr rust nip4 error")]
    NostrRustError(nostr_rust::nips::nip4::Error),

    #[error("Nostr rust client error")]
    NostrRustClientError(nostr_rust::nostr_client::ClientError),

    #[error("Nostr rust nip 16 error")]
    NIP16(NIP16Error),

    #[error("Nostr rust nip 9 error")]
    NIP9(NIP9Error),

    #[error("Bitcoin Sep256k1 error")]
    BitcoinSecpError(bdk::bitcoin::secp256k1::Error),

    #[error("Sep256k1 error")]
    Secp256k1Error(secp256k1::Error),

    #[error("Could not broadcast transaction")]
    FailedToBroadcast,

    #[error("CJ value over max")]
    CJValueOveMax,

    #[error("Output value less then expected value")]
    OutputValueLessExpected,

    #[error("CJ value below minimum")]
    CJValueBelowMin,

    #[error("Could not estimate fee")]
    FeeEstimation,

    #[error("Total fee to makers is too high")]
    MakerFeeTooHigh,

    #[error("Could not convert from string")]
    FromStringError(std::string::String),

    #[error("Could not parse amount")]
    CouldNotParseError(ParseAmountError),

    #[error("Insufficient funds")]
    InsufficientFunds,

    #[error("Taker did not respond with transaction")]
    TakerFailedToSendTransaction,

    #[error("Not enough makers")]
    NotEnoughMakers,

    #[error("Could not verify podle")]
    PodleVerifyFailed,

    #[error("Podle commit does not match provided")]
    PodleCommitment,

    #[error("Could not get num")]
    GetNum,

    #[error("Not enough makers responded")]
    MakersFailedToRespond,

    #[cfg(feature = "bdk")]
    #[error("BDK error: {}", _0)]
    BDKError(bdk::Error),

    #[error("DecodeError")]
    DecodeError(String),

    #[error("Bad input script")]
    BadInput,

    #[error("Fees too high")]
    FeesTooHigh,

    #[error("Invalid credentials")]
    InvalidCredentials,
}

#[cfg(feature = "bitcoincore")]
impl From<bitcoincore_rpc::Error> for Error {
    fn from(err: bitcoincore_rpc::Error) -> Self {
        Self::BitcoinRpcError(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::SerdeError(err)
    }
}

impl From<nostr_rust::nips::nip4::Error> for Error {
    fn from(err: nostr_rust::nips::nip4::Error) -> Self {
        Self::NostrRustError(err)
    }
}

impl From<nostr_rust::nostr_client::ClientError> for Error {
    fn from(err: nostr_rust::nostr_client::ClientError) -> Self {
        Self::NostrRustClientError(err)
    }
}

impl From<bdk::bitcoin::secp256k1::Error> for Error {
    fn from(err: bdk::bitcoin::secp256k1::Error) -> Self {
        Self::BitcoinSecpError(err)
    }
}
/*
impl From<secp256k1::Error> for Error {
    fn from(err: secp256k1::Error) -> Self {
        Self::Secp256k1Error(err)
    }
}
*/

impl From<std::string::String> for Error {
    fn from(err: std::string::String) -> Self {
        Self::FromStringError(err)
    }
}

impl From<ParseAmountError> for Error {
    fn from(err: ParseAmountError) -> Self {
        Self::CouldNotParseError(err)
    }
}

impl From<NIP16Error> for Error {
    fn from(err: NIP16Error) -> Self {
        Self::NIP16(err)
    }
}

impl From<NIP9Error> for Error {
    fn from(err: NIP9Error) -> Self {
        Self::NIP9(err)
    }
}

#[cfg(feature = "bdk")]
impl From<bdk::Error> for Error {
    fn from(err: bdk::Error) -> Self {
        Self::BDKError(err)
    }
}
