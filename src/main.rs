use clap::{Parser, Subcommand};

use dotenvy::dotenv;
use std::{env, panic};

use log::{debug, info, LevelFilter};
use nostrdizer::{
    maker::Maker,
    taker,
    types::{BitcoinCoreCreditals, FillOffer, MakerConfig},
};

use nostr_rust::keys::get_random_secret_key;

use serde::{Deserialize, Serialize};

use rand::{thread_rng, Rng};
use std::io::Write;

use bitcoin::Amount;

/// CLI for joinstr
#[derive(Parser, Debug, Serialize, Deserialize)]
#[command(name = "nostr-tool")]
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
        rel_fee: Option<f32>,
        #[arg(long)]
        minsize: Option<u64>,
        #[arg(long)]
        maxsize: Option<u64>,
        #[arg(long)]
        will_broadcast: Option<bool>,
    },
}
fn main() -> anyhow::Result<()> {
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
    debug!("Main");
    // Parse input
    let args: Cli = Cli::parse();
    dotenv().ok();

    // let wallet_name = args.wallet_name;

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
            // TODO: Should filter peers by rate
            let number_of_makers = match number_of_makers {
                Some(num) => *num,
                None => {
                    let mut rng = thread_rng();
                    rng.gen_range(3..9)
                }
            };
            // REVIEW: if there are no matching offers it just ends
            let matching_peers = taker.get_matching_offers(*send_amount)?;
            debug!("Matching peers {:?}", matching_peers);
            if matching_peers.is_empty() {
                println!("There are no makers that match this send")
            } else {
                for (i, peer) in matching_peers.iter().enumerate() {
                    debug!("Peer: {:?}", peer);
                    taker.send_fill_offer_message(
                        FillOffer {
                            offer_id: peer.1.offer_id,
                            amount: *send_amount,
                        },
                        peer.0,
                    )?;

                    if i > number_of_makers {
                        break;
                    }
                }
                debug!("Send fills");

                // wait for responses from peers
                // Gets peers psbt inputs
                // loops until enough peers have responded
                let peer_psbts_hash = taker.get_peer_inputs(number_of_makers)?;
                debug!("Peer input psbts: {:?}", peer_psbts_hash);
                // Create taker psbt
                // Setting fee rate as none uses core's fee estimation
                // TODO: get the size of the transaction including all peer inputs
                let fee_rate = Some(Amount::from_sat(5000));
                let taker_psbt = taker.get_input_psbt(*send_amount, fee_rate)?;

                // converts maker inputs to vec of sting maker psbts
                let mut psbts: Vec<String> = peer_psbts_hash
                    .values()
                    .map(|maker_input| maker_input.psbt.to_string())
                    .collect();
                psbts.push(taker_psbt.psbt);

                // Join maker psbts with taker psbt
                let joined_psbt = taker.join_psbt(psbts)?;
                debug!("combined_psbt: {:?}", joined_psbt);
                // Send unsigned psbt to peers
                for (pub_key, maker_input) in peer_psbts_hash {
                    taker.send_unsigned_psbt(&pub_key, maker_input.offer_id, &joined_psbt)?;
                }
                // Wait for signed psbts
                // Combine signed psbt
                let peer_signed_psbt = taker.get_signed_peer_psbts(number_of_makers)?;
                info!("Makers have signed transaction, signing ...");
                // Taker Sign psbt
                let signed_psbt = taker.verify_and_sign_psbt(*send_amount, &peer_signed_psbt)?;
                debug!("Taker signed: {:?}", signed_psbt);
                // Broadcast signed psbt

                let finalized_psbt = taker.finalize_psbt(&signed_psbt.psbt)?;
                // debug!("Finalized psbt: {:?}", finalized_psbt);

                let txid = taker.broadcast_transaction(finalized_psbt)?;
                debug!("TXID: {:?}", txid);
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
                Some(abs_fee) => *abs_fee,
                None => {
                    if let Ok(abs_fee) = env::var("MAKER_ABS_FEE") {
                        abs_fee.parse::<u64>()?
                    } else {
                        0
                    }
                }
            };

            let rel_fee = match rel_fee {
                Some(rel_fee) => *rel_fee,
                None => {
                    if let Ok(rel_fee) = env::var("MAKER_REL_FEE") {
                        rel_fee.parse::<f32>()?
                    } else {
                        0.0
                    }
                }
            };

            let minsize = match minsize {
                Some(minsize) => *minsize,
                None => {
                    if let Ok(minsize) = env::var("MAKER_MINSIZE") {
                        minsize.parse()?
                    } else {
                        5000
                    }
                }
            };

            let maxsize = match maxsize {
                Some(maxsize) => Some(*maxsize),
                None => {
                    if let Ok(maxsize) = env::var("MAKER_MAXSIZE") {
                        Some(maxsize.parse()?)
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
            println!("Listening...");

            let mut maker = Maker::new(&priv_key, relay_urls, &mut config, bitcoin_core_creds)?;

            let active_offer = maker.get_active_offer()?;

            if let Some(active_offer) = active_offer {
                // TODO Replace current offer with new offer
                info!("Running maker with offer: {:?}", active_offer);
            } else {
                maker.publish_offer()?;
            }

            let (peer_pubkey, fill_offer) = maker.get_fill_offer()?;

            debug!("Fill Offer: {:?}", fill_offer);

            let maker_psbt = maker.get_input_psbt(fill_offer.amount, Some(Amount::from_sat(0)))?;
            debug!("Maker psbt {:?}", maker_psbt);

            let psbt_string = serde_json::to_string(&maker_psbt)?;
            debug!("Psbt string: {:?}", psbt_string);
            maker.send_maker_psbt(&peer_pubkey, fill_offer.offer_id, maker_psbt)?;
            debug!("Sent");

            let unsigned_psbt = maker.get_unsigned_cj_psbt()?;
            debug!("Unsigned psbt: {:?}", unsigned_psbt);

            let signed_psbt = maker.verify_and_sign_psbt(&fill_offer, &unsigned_psbt)?;

            maker.send_signed_psbt(&peer_pubkey, fill_offer, &signed_psbt)?;
        }
    }
    Ok(())
}
