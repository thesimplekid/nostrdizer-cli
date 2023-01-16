use bdk::blockchain::ConfigurableBlockchain;
use bdk::blockchain::{
    rpc::{Auth, RpcBlockchain, RpcConfig},
    AnyBlockchain,
};
use bdk::database::{AnyDatabase, MemoryDatabase};
use bdk::wallet::AddressIndex;
use bdk::{LocalUtxo, SyncOptions, Wallet};
use bitcoin::psbt::Input;
use bitcoin::{Amount, TxOut};

use bdk::bitcoin::secp256k1::Secp256k1;
use bdk::bitcoin::util::bip32::{DerivationPath, KeySource};
use bdk::bitcoin::Network;
use bdk::keys::bip39::{Language, Mnemonic, WordCount};
use bdk::keys::DescriptorKey::Secret;
use bdk::keys::{DerivableKey, DescriptorKey, ExtendedKey, GeneratableKey, GeneratedKey};
use bdk::miniscript::miniscript::Segwitv0;
use std::str::FromStr;

use crate::errors::Error;
use crate::types::RpcInfo;

pub fn new_rpc_blockchain(blockchain_config: RpcInfo) -> Result<AnyBlockchain, Error> {
    // let client = Client::new("localhost:50000").unwrap();

    let config = RpcConfig {
        url: blockchain_config.url,
        auth: Auth::UserPass {
            username: blockchain_config.username,
            password: blockchain_config.password,
        },
        network: blockchain_config.network,
        wallet_name: blockchain_config.wallet_name,
        sync_params: None,
    };
    let blockchain = RpcBlockchain::from_config(&config)?;
    // let blockchain = ElectrumBlockchain::from(client);

    Ok(AnyBlockchain::Rpc(Box::new(blockchain)))
}

pub fn new_wallet(
    blockchain: &AnyBlockchain,
    descriptor: (String, String),
) -> Result<Wallet<AnyDatabase>, Error> {
    let wallet = Wallet::new(
        &descriptor.0,
        Some(&descriptor.1),
        bitcoin::Network::Regtest,
        AnyDatabase::Memory(MemoryDatabase::new()),
    )?;

    wallet.sync(blockchain, SyncOptions::default())?;

    println!("Descriptor balance: {} SAT", wallet.get_balance()?);
    log::debug!("Fund address: {:?}", wallet.get_address(AddressIndex::New));

    Ok(wallet)
}

pub fn get_unspent(wallet: &Wallet<AnyDatabase>) -> Result<Vec<LocalUtxo>, Error> {
    // TODO: Figure out syncing
    //wallet.sync(blockchain, sync_opts)

    Ok(wallet.list_unspent()?)
}

pub fn get_input_value(
    inputs: &[Input],
    wallet: &Wallet<AnyDatabase>,
) -> Result<(Amount, Amount), Error> {
    let mut my_input_value = 0;
    let mut input_value = 0;
    for input in inputs {
        if let Some(script) = &input.witness_utxo {
            if wallet.is_mine(&script.script_pubkey)? {
                my_input_value += input.witness_utxo.as_ref().unwrap().value;
            }
            input_value += &input.witness_utxo.as_ref().unwrap().value;
        }
    }

    Ok((
        Amount::from_sat(input_value),
        Amount::from_sat(my_input_value),
    ))
}

pub fn get_output_value(
    outputs: &[TxOut],
    wallet: &Wallet<AnyDatabase>,
) -> Result<(Amount, Amount), Error> {
    let mut my_output_value = Amount::ZERO;
    let mut output_value = Amount::ZERO;

    for output in outputs {
        if wallet.is_mine(&output.script_pubkey)? {
            my_output_value += Amount::from_sat(output.value);
        }
        output_value += Amount::from_sat(output.value);
    }

    Ok((output_value, my_output_value))
}
// https://github.com/bitcoindevkit/bitcoindevkit.org
// generate fresh descriptor strings and return them via (receive, change) tuple
pub fn get_descriptors() -> (String, String) {
    // Create a new secp context
    let secp = Secp256k1::new();

    // You can also set a password to unlock the mnemonic
    let password = Some("random password".to_string());

    // Generate a fresh mnemonic, and from there a privatekey
    let mnemonic: GeneratedKey<_, Segwitv0> =
        Mnemonic::generate((WordCount::Words12, Language::English)).unwrap();
    let mnemonic = mnemonic.into_key();
    let xkey: ExtendedKey = (mnemonic, password).into_extended_key().unwrap();
    let xprv = xkey.into_xprv(Network::Regtest).unwrap();

    // Create derived privkey from the above master privkey
    // We use the following derivation paths for receive and change keys
    // receive: "m/84h/1h/0h/0"
    // change: "m/84h/1h/0h/1"
    let mut keys = Vec::new();

    for path in ["m/84h/1h/0h/0", "m/84h/1h/0h/1"] {
        let deriv_path: DerivationPath = DerivationPath::from_str(path).unwrap();
        let derived_xprv = &xprv.derive_priv(&secp, &deriv_path).unwrap();
        let origin: KeySource = (xprv.fingerprint(&secp), deriv_path);
        let derived_xprv_desc_key: DescriptorKey<Segwitv0> = derived_xprv
            .into_descriptor_key(Some(origin), DerivationPath::default())
            .unwrap();

        // Wrap the derived key with the wpkh() string to produce a descriptor string
        if let Secret(key, _, _) = derived_xprv_desc_key {
            let mut desc = "wpkh(".to_string();
            desc.push_str(&key.to_string());
            desc.push(')');
            keys.push(desc);
        }
    }

    // Return the keys as a tuple
    (keys[0].clone(), keys[1].clone())
}
