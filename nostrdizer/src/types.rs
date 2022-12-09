use bitcoin::{Address, Amount, SignedAmount, Txid};
use serde::{Deserialize, Serialize};

/// Info for maker offer
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Offer {
    /// Offer Id
    pub offer_id: u32,
    /// Absolute fee to maker
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub abs_fee: Amount,
    /// Percent of send amount fee to maker
    pub rel_fee: f64,
    /// Min size of coinjoin for maker
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub minsize: Amount,
    /// Max size of coinjoin for maker
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub maxsize: Amount,
    /// If maker is willing to broadcast final transaction
    pub will_broadcast: bool,
}

/// Taker sends to fill maker offer
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FillOffer {
    pub offer_id: u32,
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub amount: Amount,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Psbt {
    pub offer_id: u32,
    pub psbt: String,
}

/// Possible messages that can be sent
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum NostrdizerMessages {
    Offer(Offer),
    FillOffer(FillOffer),
    MakerInputs(MakerInput),
    UnsignedCJ(Psbt),
    SignedCJ(Psbt),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum NostrdizerMessageKind {
    Offer,
    FillOffer,
    MakerInput,
    MakerPsbt,
    UnsignedCJ,
    SignedCJ,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NostrdizerMessage {
    pub event_type: NostrdizerMessageKind,
    pub event: NostrdizerMessages,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MakerInput {
    pub offer_id: u32,
    pub inputs: Vec<(Txid, u32)>,
    pub cj_out_address: Address,
    pub change_address: Address,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BitcoinCoreCreditals {
    pub rpc_url: String,
    pub rpc_username: String,
    pub rpc_password: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VerifyCJInfo {
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub mining_fee: SignedAmount,
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub maker_fee: SignedAmount,
    pub verifyed: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CJFee {
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub abs_fee: Amount,
    pub rel_fee: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MaxMineingFee {
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub abs_fee: Amount,
    pub rel_fee: f64,
}
