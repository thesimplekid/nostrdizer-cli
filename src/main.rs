use clap::{Parser, Subcommand};

use dotenvy::dotenv;
use std::{env, panic};

use log::{debug, error, warn, LevelFilter};
use nostrdizer::{
    errors::Error as NostrdizerError,
    maker::{Config as MakerConfig, Maker},
    podle::verify_podle,
    taker,
    types::{Amount, BitcoinCoreCreditals},
};

use serde::{Deserialize, Serialize};

use rand::{thread_rng, Rng};
use std::io::Write;

use anyhow::{bail, Result};
//use bitcoin::Amount;

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
    /// Test Poodle
    TestPoodle,
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
    /*
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
    */

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
        Commands::TestPoodle => {
            let taker = taker::Taker::new(args.priv_key, relay_urls, bitcoin_core_creds)?;
            let commit = taker.generate_podle()?;

            if let Err(_err) = verify_podle(255, commit.clone(), commit.commit) {
                panic!()
            }

            // let num = podle::get_nums(0).unwrap().to_string();

            // println!("{:?}", num);
        }
        Commands::ListUnspent => {
            let mut taker = taker::Taker::new(args.priv_key, relay_urls, bitcoin_core_creds)?;
            let unspent = taker.get_unspent();
            println!("{:#?}", unspent);
        }
        Commands::GetEligibleBalance => {
            let mut taker = taker::Taker::new(args.priv_key, relay_urls, bitcoin_core_creds)?;
            let balance = taker.get_eligible_balance()?;
            println!("{:?}", balance);
        }
        Commands::ListOffers => {
            let mut taker = taker::Taker::new(args.priv_key, relay_urls, bitcoin_core_creds)?;
            let offers = taker.get_offers()?;
            for (i, offer) in offers.iter().enumerate() {
                println!("Offer {}: {:?}", i, offer);
            }
        }
        Commands::SendTransaction {
            send_amount,
            number_of_makers,
        } => {
            let mut taker = taker::Taker::new(args.priv_key, relay_urls, bitcoin_core_creds)?;

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

            // Check to make sure taker has sufficient balance
            if taker.get_eligible_balance()? < send_amount {
                bail!("Insufficient funds")
            }

            // REVIEW: if there are no matching offers it just ends
            let mut matching_peers = taker.get_matching_offers(send_amount)?;
            // debug!("Matching peers {:?}", matching_peers);
            // println!("{} makers matched your order", matching_peers.len());

            if matching_peers.is_empty() {
                bail!("There are no makers that match this order")
            }

            println!("Choosing {} peers with the lowest fee", number_of_makers);

            // Step 2: Send fill offer (!fill)
            let matched_offers = taker.send_fill_offer_message(
                send_amount,
                number_of_makers,
                &mut matching_peers,
            )?;

            println!("Sent fill offers to peers");

            // Step 3: Receive maker pub key (!pubkey)
            // TODO: Just gonna skip this for now
            taker.get_maker_pubkey()?;
            debug!("got pub key");

            println!("Waiting for peer inputs...");
            // Step 4: Send auth (!auth)
            let auth_commitment = taker.generate_podle()?;
            taker.send_auth_message(auth_commitment, matched_offers)?;
            debug!("Sent auth");

            // Step 5: Receive maker inputs (!ioauth)
            // wait for responses from peers
            // Gets peers tx inputs
            // loops until enough peers have responded
            let peer_inputs = taker.get_peer_inputs(number_of_makers, matching_peers)?;
            println!("Peers have sent inputs creating transaction...");

            // Step 6: Send CJ transaction (!tx)
            let cj = taker.create_cj(send_amount, &peer_inputs)?;
            // Send unsigned tx to peers
            for (offer, _maker_input) in peer_inputs {
                taker.send_unsigned_transaction(&offer.maker, &cj)?;
            }

            // Step 7: Sign TX (!sig)
            println!("Waiting for peer signatures...");
            // Wait for signed txs
            // Combine signed tx
            let peer_signed_txs = taker.get_signed_peer_transaction(number_of_makers)?;
            println!("Makers have signed transaction, signing ...");

            // Taker Sign tx
            if let Ok(tx_info) = taker.verify_transaction(send_amount, &peer_signed_txs) {
                println!("Total fee to makers: {} sats.", tx_info.maker_fee.to_sat());
                println!("Mining fee: {} sats", tx_info.mining_fee.to_sat());
                if tx_info.verifyed {
                    println!("Transaction passed verification, signing ...");
                    let signed_tx = taker.sign_transaction(&peer_signed_txs)?;
                    println!("Finalized transaction, broadcasting ...");

                    // Broadcast signed tx
                    let txid = taker.broadcast_transaction(signed_tx)?;
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
            let mut maker = Maker::new(
                args.priv_key,
                relay_urls.clone(),
                &mut config,
                bitcoin_core_creds,
            )?;
            loop {
                // Step 1: Publish order (!ordertype)
                maker.publish_offer()?;

                // println!("Running maker with {:?}", offer);
                println!("Waiting for takers...");

                // Step 2: Receives fill offer (!fill)
                let (peer_pubkey, fill_offer) = maker.get_fill_offer()?;

                println!("Received fill Offer: {:?}", fill_offer);

                maker.delete_active_offer()?;

                // Step 3: sends maker (!pubkey)
                maker.send_pubkey(&peer_pubkey)?;

                // Step 4: Receives !auth
                let auth_commitment = maker.get_commitment_auth()?;
                // TODO: Handle errors
                maker.verify_podle(auth_commitment)?;

                // Step 5: sends (!ioauth)
                let maker_input = maker.get_inputs(&fill_offer)?;
                maker.send_maker_input(&peer_pubkey, maker_input)?;

                // Step 6: Receives Transaction Hex (!tx)
                match maker.get_unsigned_cj_transaction() {
                    Ok(unsigned_tx) => {
                        if let Ok(tx_info) = maker.verify_transaction(&fill_offer, &unsigned_tx) {
                            if tx_info.verifyed {
                                // Step 7: Signs and sends transaction to taker if verified (!sig)
                                let signed_tx = maker.sign_tx_hex(&unsigned_tx)?;
                                maker.send_signed_tx(&peer_pubkey, &signed_tx)?;
                            } else {
                                warn!("Transaction could not be verified");
                            }
                        }
                    }
                    Err(NostrdizerError::TakerFailedToSendTransaction) => {
                        warn!("Taker did not send transaction");
                    }
                    Err(err) => error!("{:?}", err),
                }
            }
        }
    }
    Ok(())
}
