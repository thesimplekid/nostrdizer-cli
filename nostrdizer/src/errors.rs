use bitcoin::util::amount::ParseAmountError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Bitcoin Rpc error")]
    BitcoinRpcError(bitcoincore_rpc::Error),

    #[error("No matching utxos")]
    NoMatchingUtxo,

    #[error("Serde error")]
    SerdeError(serde_json::Error),

    #[error("Nostr rust nip4 error")]
    NostrRustError(nostr_rust::nips::nip4::Error),

    #[error("Nostr rust client error")]
    NostrRustClientError(nostr_rust::nostr_client::ClientError),

    #[error("Bitcoin Sep256k1 error")]
    BitcoinSecpError(bitcoin::secp256k1::Error),

    #[error("Could not broadcast transaction")]
    FailedToBrodcast,

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
}

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

impl From<bitcoin::secp256k1::Error> for Error {
    fn from(err: bitcoin::secp256k1::Error) -> Self {
        Self::BitcoinSecpError(err)
    }
}

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
