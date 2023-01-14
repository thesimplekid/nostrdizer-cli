# nostrdizer

[![License](https://img.shields.io/badge/License-BSD_3--Clause-blue.svg)](LICENSE)

A [Nostr](https://github.com/nostr-protocol/nostr) client built to create Bitcoin Collaborative transactions, known as Coinjoins. 
Based on the [Joinmarket](https://github.com/JoinMarket-Org/joinmarket-clientserver) model where there is a maker and a taker. 
A maker is always online available to take part in the transaction, and a taker who chooses when and what size of transaction to create.

To incentivize running a maker the taker pays a small fee to the maker for their service. The Maker also has the benefit of gaining privacy from each transaction they participate in so they can choose to set a fee of zero to be more likely to be selected by the taker in a transaction. 

## State
I'm currently in the process of changing from using bitcoincore-rpc to BDK, so there is quite a bit of dead or half finished code around, that I'm going to leave until I finish getting BDK to work. Using the bitcoincore-rpc a complete transaction can occur, however it doesn't not actually verify the transaction is correct (ie you don't spend to much on maker fees, the maker gets back what they put in). Since im planning to use BDK for this I'm not going to fix it and just bypass it as a proof of concept.  

---
**This is Alpha level software with many things that need to be changed, added, improved and tested, please do not use on mainnet.**
---

An Overview of the order [flow](./nostrdizer/docs/FLOW.md).

## Getting started

### Run Maker 
```
cargo r -- --rpc-url "<url of bitcoin core RPC API>" run-maker
```
### Run Taker
```
cargo r -- --rpc-url "<url of bitcoin core RPC API>" send-transaction --send-amount <Send amount> --number-of-makers <number of makers>

```

### Known Issues
- [ ] Mining fee estimation doesn't work
- [ ] Does not check for dust
- [ ] No coin control to prevent mixing change and CJ 
    - [ ] Maker does not verify that CJ outputs are sent to correct send vs change address
### Todo
- [ ] Update nostr_rust version
    Nostr_rust greater then 14 uses a newer version of secp256k1 that causes compatibility issues with version used in rust_bitcoin, should be fixed in next release of rust_bitcoin.
    - [x] Events should be verified
- [ ] Cleanup, add tests, and COMMENTS
- [ ] Move as much code as possible to common not behind features
 - [ ] BDK has rpc capablities might be better to use that
- [x] Use Replaceable events for offers
- [x] Delete events when cj completed
    - [x] Use ephemeral events for messages
    - [x] Delete maker offer
- [x] Maker should republish offer after completed Coinjoins
    - [ ] New key with proof of work?
- [x] Maker republish offer if taker doesn't not respond 
- [ ] Taker griefing [#1](https://github.com/thesimplekid/nostrdizer-cli/issues/1)
    - [x] Taker generates and sends Podle commitment
    - [x] Maker validates poodle commitment
    - [ ] Maker stores lists of used commits and checks it was not used before (this is what makes it useful)
        - [ ] Should these be gossiped?
- ~~[ ] Work out how to make interoperable with JM as a taker.~~
    - [ ] Serialization of messages 
    - [ ] Many more ...
- [ ] Use [nip-40](https://github.com/nostr-protocol/nips/blob/master/40.md) expiring events for offers
- [ ] Fidelity Bond (it'll be a bit)
- [ ] Add print outs 
- [ ] Add Docs

Working but should fix
- [ ] I'm incositanct about where i loop to send messages, some send messages accept a vec of the peers and send the messages. some only accept on pub key and loop is in main.  This all should accept vec

### A Note on Forks
My fork of [rust-bitcoin-rpc](https://github.com/rust-bitcoin/rust-bitcoincore-rpc) is required as a few functions are not merged upstream. 
I do intend to clean this up and create a PR to merge upstream as I would rather not depend on forks 
- [x] [decode transaction](https://github.com/rust-bitcoin/rust-bitcoincore-rpc/pull/271). 
- [ ] [Get Change Address](https://github.com/rust-bitcoin/rust-bitcoincore-rpc/pull/261)

### Bitcoin Core
Bitcoin core is requited, v23 and v22 have been tested. Other versions may work but have not been tested. 

## License
Code is under the BSD 3-Clause License ([LICENSE](LICENSE) or [https://opensource.org/licenses/BSD-3-Clause](https://opensource.org/licenses/BSD-3-Clause))  

