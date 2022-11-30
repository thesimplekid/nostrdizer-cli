use serde::{Deserialize, Serialize};

/// Info for maker offer
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Offer {
    /// Offer Id
    pub offer_id: u32,
    /// Absolute fee to maker in sats
    pub abs_fee: u64,
    /// Percent of send amount fee to maker
    pub rel_fee: f32,
    /// Min size of coinjoin for maker
    pub minsize: u64,
    /// Max size of coinjoin for maker
    pub maxsize: u64,
    /// If maker is willing to broadcast final transaction
    pub will_broadcast: bool,
}

/// Taker sends to fill maker offer
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FillOffer {
    pub offer_id: u32,
    pub amount: u64,
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
    MakerPsbt(Psbt),
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
pub struct MakerConfig {
    pub abs_fee: u64,
    pub rel_fee: f32,
    pub minsize: u64,
    pub maxsize: Option<u64>,
    pub will_broadcast: bool,
}
