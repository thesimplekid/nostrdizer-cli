pub use bitcoin::Amount;
use bitcoin::{Address, SignedAmount, Txid};
use bitcoin_hashes::sha256::Hash;
use secp256k1::PublicKey;
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RelOffer {
    /// Order Id
    #[serde(rename = "oid")]
    pub offer_id: u32,
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AbsOffer {
    /// Order Id
    #[serde(rename = "oid")]
    pub offer_id: u32,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Offer {
    #[serde(rename = "sw0reloffer")]
    RelOffer(RelOffer),
    #[serde(rename = "sw0absoffer")]
    AbsOffer(AbsOffer),
}

/// Taker Fill
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename = "fill")]
pub struct Fill {
    #[serde(rename = "oid")]
    pub offer_id: u32,
    #[serde(with = "bitcoin::util::amount::serde::as_sat")]
    pub amount: Amount,
    pub tencpubkey: String,
    /// Used for Poodle Hash of P2
    pub commitment: Hash,
}

/// Maker pubkey
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename = "pubkey")]
pub struct Pubkey {
    pub mencpubkey: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename = "tx")]
pub struct Transaction {
    /// Transaction hex
    pub tx: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename = "ioauth")]
pub struct IoAuth {
    // TODO: Serialize as txid:vout
    #[serde(rename = "ulist")]
    pub utxos: Vec<(Txid, u32)>,
    pub maker_auth_pub: String,
    #[serde(rename = "coinjoinA")]
    pub coinjoin_address: Address,
    #[serde(rename = "changeA")]
    pub change_address: Address,
    /// bitcoin signature of mencpubkey
    pub bitcoin_sig: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename = "sig")]
pub struct SignedTransaction {
    #[serde(rename = "sig")]
    pub tx: Vec<u8>,
}

/// Possible messages that can be sent
#[derive(Serialize, Deserialize, Debug, Clone)]
// Look at these they may be able to tag better and remove the nostrdizer message type field
// https://serde.rs/enum-representations.html
pub enum NostrdizerMessages {
    Offer(Offer),
    Fill(Fill),
    PubKey(Pubkey),
    Auth(AuthCommitment),
    MakerInputs(IoAuth),
    UnsignedCJ(Transaction),
    SignedCJ(SignedTransaction),
}

/// Kinds of `NostrdizerMessages`
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum NostrdizerMessageKind {
    /// Maker offer
    Offer,
    /// Taker filling offer
    FillOffer,
    /// Maker pub key
    MakerPubkey,
    /// TakerAuth
    Auth,
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

// TODO: Need to serialize correctly
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AuthCommitment {
    #[serde(rename = "P")]
    pub p: PublicKey,
    #[serde(rename = "P2")]
    pub p2: PublicKey,
    pub commit: Hash,
    pub sig: Vec<u8>,
    pub e: Hash,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MakerConfig {
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub abs_fee: Amount,
    pub rel_fee: f64,
    #[serde(with = "bitcoin::util::amount::serde::as_btc")]
    pub minsize: Amount,
    #[serde(default, with = "bitcoin::util::amount::serde::as_btc::opt")]
    pub maxsize: Option<Amount>,
    pub will_broadcast: bool,
}

pub struct TakerConfig {
    pub cj_fee: CJFee,
    pub mining_fee: MaxMineingFee,
    pub minium_makers: usize,
}
