use clap::{Parser, Subcommand};

use dotenvy::dotenv;
use std::{collections::HashMap, env, panic};

use log::{debug, LevelFilter};
use nostrdizer::{
    maker::{Config as MakerConfig, Maker},
    taker,
    types::{BitcoinCoreCreditals, FillOffer},
};

use nostr_rust::keys::get_random_secret_key;

use serde::{Deserialize, Serialize};

use rand::{thread_rng, Rng};
use std::io::Write;

use anyhow::{bail, Result};
use bitcoin::Amount;

/// CLI for nostrdizer
#[derive(Parser, Debug, Serialize, Deserialize)]
#[command(name = "nostrdizer")]
#[command(author = "thesimplekid tsk@thesimplekid.com")]
#[command(version = "0.1")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Nostr private key
    #[arg(short, long, value_parser)]
    priv_key: Option<String>,

    /// Bitcoin core rpc rpc_url
    #[arg(long, value_parser)]
    rpc_url: Option<String>,

    /// Bitcoin core username
    #[arg(long, value_parser)]
    rpc_username: Option<String>,

    /// Bitcoin core password
    #[arg(long, value_parser)]
    rpc_password: Option<String>,

    /// Nostr relays
    #[arg(long, value_parser)]
    nostr_relays: Option<Vec<String>>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    priv_key: Option<String>,
    rpc_url: Option<String>,
    rpc_username: Option<String>,
    rpc_password: Option<String>,
    nostr_relays: Option<Vec<String>>,
}

#[derive(Subcommand, Debug, Serialize, Deserialize)]
enum Commands {
    /// List unspent UTXOs
    ListUnspent,
    /// Show wallet balance
    GetEligibleBalance,
    /// List offers
    ListOffers,
    /// Send with coinjoin
    SendTransaction {
        #[arg(short, long)]
        send_amount: u64,
        #[arg(long)]
        number_of_makers: Option<usize>,
        // Add: max fee
    },
    // TODO: Run maker should check if offer is published and publish an offer if not
    /// Run as maker
    RunMaker {
        #[arg(long)]
        abs_fee: Option<u64>,
        #[arg(long)]
        rel_fee: Option<f64>,
        #[arg(long)]
        minsize: Option<u64>,
        #[arg(long)]
        maxsize: Option<u64>,
        #[arg(long)]
        will_broadcast: Option<bool>,
    },
}
fn main() -> Result<()> {
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}:{} {} [{}] - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter(Some("nostrdizer"), LevelFilter::Debug)
        .init();
    // Parse input
    let args: Cli = Cli::parse();
    dotenv().ok();

    let rpc_url = match args.rpc_url {
        Some(url) => url,
        // TODO: Add port
        None => {
            if let Ok(url) = env::var("RPC_URL") {
                url
            } else {
                panic!("No RPC url")
            }
        }
    };

    let rpc_username = match args.rpc_username {
        Some(username) => username,
        None => {
            if let Ok(username) = env::var("RPC_USERNAME") {
                username
            } else {
                panic!("No RPC username")
            }
        }
    };

    let rpc_password = match args.rpc_password {
        Some(password) => password,
        None => {
            if let Ok(password) = env::var("RPC_PASSWORD") {
                password
            } else {
                panic!("No RPC password")
            }
        }
    };

    let bitcoin_core_creds = BitcoinCoreCreditals {
        rpc_url,
        rpc_username,
        rpc_password,
    };

    let priv_key = match args.priv_key {
        Some(priv_key) => priv_key,
        None => {
            if let Ok(priv_key) = env::var("SECRET_KEY") {
                priv_key
            } else {
                let (sk, _) = get_random_secret_key();
                hex::encode(sk.as_ref())
            }
        }
    };

    let relay_urls = match args.nostr_relays {
        Some(nostr) => nostr,
        None => {
            if let Ok(nostr_relays) = env::var("NOSTR_RELAYS") {
                serde_json::from_str(&nostr_relays)?
            } else {
                vec!["ws://localhost:7000".to_string()]
            }
        }
    };

    // REVIEW: be nice to get rid of this
    let relay_urls: Vec<&str> = relay_urls.iter().map(|x| x as &str).collect();

    match &args.command {
        Commands::ListUnspent => {
            let mut taker = taker::Taker::new(&priv_key, relay_urls, bitcoin_core_creds)?;
            let unspent = taker.get_unspent();
            println!("{:#?}", unspent);
        }
        Commands::GetEligibleBalance => {
            let mut taker = taker::Taker::new(&priv_key, relay_urls, bitcoin_core_creds)?;
            let balance = taker.get_eligible_balance()?;
            println!("{:?}", balance);
        }
        Commands::ListOffers => {
            let mut taker = taker::Taker::new(&priv_key, relay_urls, bitcoin_core_creds)?;
            let offers = taker.get_offers()?;
            for (i, offer) in offers.iter().enumerate() {
                println!("Offer {}: {:?}", i, offer);
            }
        }
        Commands::SendTransaction {
            send_amount,
            number_of_makers,
        } => {
            let mut taker = taker::Taker::new(&priv_key, relay_urls, bitcoin_core_creds)?;

            let number_of_makers = match number_of_makers {
                Some(num) => *num,
                None => {
                    let mut rng = thread_rng();
                    rng.gen_range(3..9)
                }
            };

            let send_amount = Amount::from_sat(*send_amount);

            println!(
                "Looking for offers to send {} sats with {} peers.",
                send_amount.to_sat(),
                number_of_makers
            );
            // REVIEW: if there are no matching offers it just ends
            let matching_peers = taker.get_matching_offers(send_amount)?;
            debug!("Matching peers {:?}", matching_peers);
            println!("{} makers matched your order", matching_peers.len());

            let mut matched_peers = HashMap::new();
            if matching_peers.is_empty() {
                bail!("There are no makers that match this order")
            }

            println!("Choosing {} peers with the lowest fee", number_of_makers);
            for (i, peer) in matching_peers.iter().enumerate() {
                debug!("Peer: {:?} Offer: {:?}", peer.0, peer.1);
                taker.send_fill_offer_message(
                    FillOffer {
                        offer_id: peer.1.offer_id,
                        amount: send_amount,
                    },
                    &peer.0,
                )?;
                matched_peers.insert(peer.0.clone(), peer.1);

                if i > number_of_makers {
                    break;
                }
            }
            println!("Sent fill offers to peers");
            println!("Waiting for peer inputs...");

            // wait for responses from peers
            // Gets peers psbt inputs
            // loops until enough peers have responded
            let peer_inputs = taker.get_peer_inputs(number_of_makers)?;
            println!("Peers have sent inputs creating transaction...");

            let cj = taker.create_cj(send_amount, peer_inputs, matched_peers.clone())?;

            // Send unsigned psbt to peers
            for (pub_key, maker_input) in matched_peers {
                taker.send_unsigned_psbt(&pub_key, maker_input.offer_id, &cj)?;
            }

            println!("Waiting for peer signatures...");
            // Wait for signed psbts
            // Combine signed psbt
            let peer_signed_psbt = taker.get_signed_peer_psbts(number_of_makers)?;
            println!("Makers have signed transaction, signing ...");

            // Taker Sign psbt
            if let Ok(psbt_info) = taker.verify_psbt(send_amount, &peer_signed_psbt) {
                println!("Total fee to makers: {} sats.", psbt_info.maker_fee.to_sat());
                println!("Mining fee: {} sats", psbt_info.mining_fee.to_sat());
                if psbt_info.verifyed {
                    println!("Transaction passed verification, signing ...");
                    let signed_psbt = taker.sign_psbt(&peer_signed_psbt)?;
                    let finalized_psbt = taker.finalize_psbt(&signed_psbt.psbt)?;
                    println!("Finalized transaction, broadcasting ...");

                    // Broadcast signed psbt
                    let txid = taker.broadcast_transaction(finalized_psbt)?;
                    println!("TXID: {:?}", txid);
                } else {
                    bail!("Transaction could not be verified")
                }
            } else {
                bail!("Transaction could not be verified")
            }
        }
        Commands::RunMaker {
            abs_fee,
            rel_fee,
            minsize,
            maxsize,
            will_broadcast,
        } => {
            let abs_fee = match abs_fee {
                Some(abs_fee) => Amount::from_sat(*abs_fee),
                None => {
                    if let Ok(abs_fee) = env::var("MAKER_ABS_FEE") {
                        Amount::from_sat(abs_fee.parse::<u64>()?)
                    } else {
                        Amount::ZERO
                    }
                }
            };

            let rel_fee = match rel_fee {
                Some(rel_fee) => *rel_fee,
                None => {
                    if let Ok(rel_fee) = env::var("MAKER_REL_FEE") {
                        rel_fee.parse::<f64>()?
                    } else {
                        0.0
                    }
                }
            };

            let minsize = match minsize {
                Some(minsize) => Amount::from_sat(*minsize),
                None => {
                    if let Ok(minsize) = env::var("MAKER_MINSIZE") {
                        Amount::from_sat(minsize.parse()?)
                    } else {
                        Amount::from_sat(5000)
                    }
                }
            };

            let maxsize = match maxsize {
                Some(maxsize) => Some(Amount::from_sat(*maxsize)),
                None => {
                    if let Ok(maxsize) = env::var("MAKER_MAXSIZE") {
                        Some(Amount::from_sat(maxsize.parse()?))
                    } else {
                        None
                    }
                }
            };

            let will_broadcast = match will_broadcast {
                Some(will_broadcast) => *will_broadcast,
                None => {
                    if let Ok(will_broadcast) = env::var("WILL_BROADCAST") {
                        will_broadcast.parse()?
                    } else {
                        true
                    }
                }
            };

            let mut config = MakerConfig {
                rel_fee,
                abs_fee,
                minsize,
                maxsize,
                will_broadcast,
            };
            // TODO: check if offer is published if not prompt to publish

            let mut maker = Maker::new(&priv_key, relay_urls, &mut config, bitcoin_core_creds)?;

            let active_offer = maker.get_active_offer()?;

            // Should maybe just always replace with a new offer
            let offer = match active_offer {
                Some(offer) => offer,
                None => maker.publish_offer()?,
            };

            println!("Running maker with {:?}", offer);
            println!("Wailing for takers...");

            let (peer_pubkey, fill_offer) = maker.get_fill_offer()?;

            println!("Received fill Offer: {:?}", fill_offer);

            let maker_input = maker.get_inputs(&fill_offer)?;
            maker.send_maker_input(&peer_pubkey, maker_input)?;
            debug!("Sent");

            let unsigned_psbt = maker.get_unsigned_cj_psbt()?;

            if let Ok(psbt_info) = maker.verify_psbt(&fill_offer, &unsigned_psbt) {
                if psbt_info.verifyed {
                    let signed_psbt = maker.sign_psbt(&unsigned_psbt)?;
                    maker.send_signed_psbt(&peer_pubkey, fill_offer, &signed_psbt)?;
                } else {
                    bail!("Transaction could not be verified");
                }
            }
        }
    }
    Ok(())
}
