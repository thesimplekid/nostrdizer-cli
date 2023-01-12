use serde::{Deserialize, Serialize};

#[cfg(feature = "bitcoincore")]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BitcoinCoreCreditals {
    pub rpc_url: String,
    pub rpc_username: String,
    pub rpc_password: String,
}
