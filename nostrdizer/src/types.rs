use bitcoin::{Address, Amount, SignedAmount, Txid};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NostrdizerOffer {
    pub maker: String,
    pub oid: u32,
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub txfee: Amount,
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub cjfee: Amount,
}

/// Maker Relative Offer
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct RelOffer {
    /// Order Id
    pub oid: u32,
    /// Min size of CJ
    /// REVIEW: Double check JM uses sats
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub minsize: Amount,
    /// Max size of CJ
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub maxsize: Amount,
    /// Amount Maker will contribute to mining fee
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub txfee: Amount,
    /// CJ Fee maker expects
    pub cjfee: f64,
}

/// Maker Absolute offer
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct AbsOffer {
    /// Order Id
    pub oid: u32,
    /// Min size of CJ
    /// REVIEW: Double check JM uses sats
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub minsize: Amount,
    /// Max size of CJ
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub maxsize: Amount,
    /// Amount Maker will contribute to mining fee
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub txfee: Amount,
    /// CJ Fee maker expects
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub cjfee: Amount,
}

/// Taker fill maker offer
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Offer {
    #[serde(rename = "sw0reloffer")]
    RelOffer(RelOffer),
    #[serde(rename = "sw0absoffer")]
    AbsOffer(AbsOffer),
}

/// Possible messages that can be sent
#[derive(Serialize, Deserialize, Debug, Clone)]
// Look at these they may be able to tag better and remove the nostrdizer message type field
// https://serde.rs/enum-representations.html
pub enum NostrdizerMessages {
    Offer(Offer),
    FillOffer(FillOffer),
    MakerInputs(MakerInput),
    UnsignedCJ(Psbt),
    SignedCJ(Psbt),
}

/// Kinds of `NostrdizerMessages`
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum NostrdizerMessageKind {
    /// Maker offer
    Offer,
    /// Taker filling offer
    FillOffer,
    /// Maker CJ inputs
    MakerInput,
    MakerPsbt,
    /// Unsigned CJ psbt
    UnsignedCJ,
    /// Signed CJ transactions
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
    // Add a signed message feild to prive ownership of inputs
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BitcoinCoreCreditals {
    pub rpc_url: String,
    pub rpc_username: String,
    pub rpc_password: String,
}

/// Final CJ transaction info
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VerifyCJInfo {
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub mining_fee: SignedAmount,
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub maker_fee: SignedAmount,
    pub verifyed: bool,
}

/// CJ Fee required for transaction
/// For a Taker, max fee will to pay
/// For Maker, min fee required
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CJFee {
    /// Absolute CJ fee
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub abs_fee: Amount,
    /// Relative CJ fee
    pub rel_fee: f64,
}

/// Maximum mining fee that can be paid
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MaxMineingFee {
    /// Max absolute value of mining fee
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub abs_fee: Amount,
    /// Max mining fee as percent of send amount
    pub rel_fee: f64,
}
