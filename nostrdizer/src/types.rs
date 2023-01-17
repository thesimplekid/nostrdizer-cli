pub use bdk::bitcoin::{Amount, Network};

use bdk::bitcoin::{
    psbt::{Input, PartiallySignedTransaction},
    Address, OutPoint, SignedAmount,
};
use bitcoin_hashes::sha256::Hash;
use secp256k1::PublicKey;
use serde::{Deserialize, Serialize};

// Nostr Message Kinds
pub const ABS_OFFER: u16 = 10123;
pub const REL_OFFER: u16 = 10124;
pub const FILL: u16 = 125;
pub const PUBKEY: u16 = 126;
pub const AUTH: u16 = 127;
pub const IOAUTH: u16 = 128;
pub const TRANSACTION: u16 = 129;
pub const SIGNED_TRANSACTION: u16 = 130;

// Dust limit
pub const DUST: u64 = 546;

// Max fee percent
pub const MAX_FEE: f32 = 0.15;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NostrdizerOffer {
    pub maker: String,
    pub oid: u32,
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
    pub txfee: Amount,
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
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
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
    pub minsize: Amount,
    /// Max size of CJ
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
    pub maxsize: Amount,
    /// Amount Maker will contribute to mining fee
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
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
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
    pub minsize: Amount,
    /// Max size of CJ
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
    pub maxsize: Amount,
    /// Amount Maker will contribute to mining fee
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
    pub txfee: Amount,
    /// CJ Fee maker expects
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
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
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_sat")]
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
    pub psbt: PartiallySignedTransaction,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename = "ioauth")]
pub struct IoAuth {
    // TODO: input should not be an option
    // Its an issue between compatibility of BDK and core
    #[serde(rename = "ulist")]
    pub utxos: Vec<(OutPoint, Option<Input>)>,
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
    pub psbt: PartiallySignedTransaction,
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
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_btc")]
    pub mining_fee: SignedAmount,
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_btc")]
    pub maker_fee: SignedAmount,
    pub verifyed: bool,
}

/// CJ Fee required for transaction
/// For a Taker, max fee will to pay
/// For Maker, min fee required
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CJFee {
    /// Absolute CJ fee
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_btc")]
    pub abs_fee: Amount,
    /// Relative CJ fee
    pub rel_fee: f64,
}

/// Maximum mining fee that can be paid
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MaxMineingFee {
    /// Max absolute value of mining fee
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_btc")]
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
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_btc")]
    pub abs_fee: Amount,
    pub rel_fee: f64,
    #[serde(with = "bdk::bitcoin::util::amount::serde::as_btc")]
    pub minsize: Amount,
    #[serde(default, with = "bdk::bitcoin::util::amount::serde::as_btc::opt")]
    pub maxsize: Option<Amount>,
    pub will_broadcast: bool,
}

pub struct TakerConfig {
    pub cj_fee: CJFee,
    pub mining_fee: MaxMineingFee,
    pub minium_makers: usize,
}

pub struct RpcInfo {
    pub url: String,
    pub username: String,
    pub password: String,
    pub network: bdk::bitcoin::Network,
    pub wallet_name: String,
}
#[cfg(feature = "bitcoincore")]
pub struct BitcoinCoreCredentials {
    pub rpc_url: String,
    pub wallet_name: String,
    pub rpc_username: String,
    pub rpc_password: String,
}

pub enum BlockchainConfig {
    #[cfg(feature = "bitcoincore")]
    CoreRPC(BitcoinCoreCredentials),
    RPC(RpcInfo),
    // electrum
}
