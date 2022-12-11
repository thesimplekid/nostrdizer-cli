# nostrdizer

[![License](https://img.shields.io/badge/License-BSD_3--Clause-blue.svg)](LICENSE)

A [Nostr](https://github.com/nostr-protocol/nostr) client built to create Bitcoin Collaborative transactions, known as Coinjoins. 
Based on the [Joinmarket](https://github.com/JoinMarket-Org/joinmarket-clientserver) model where there is a maker and a taker. 
A maker is always online available to take part in the transaction, and a taker who chooses when and what size of transaction to create.

To incentivize running a maker the taker pays a small fee to the maker for their service. The Maker also has the benefit of gaining privacy from each transaction they participate in so they can choose to set a fee of zero to be more likely to be selected by the taker in a transaction.  

---
**This is Alpha level software with many things that need to be changed, added, improved and tested, please do not use on mainnet.**
---

An Overview of the order [flow](./nostrdizer/docs/FLOW.md).

## Getting started
Currently Bitcoin Core 23 is required as the core RPC api [changed](https://github.com/rust-bitcoin/rust-bitcoincore-rpc/issues/260) between 22 and 23, hopefully I can figure out a way to handle this and older versions will be supported. The end goal will be to remove the requirement to use core at all, and users can choose what backend they want to use for their blockdata and wallet, but support for this will take some time.  


### Run Maker 
```
cargo r -- --rpc-url "<url oof bitcoin core RPC API>" run-maker
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
- [x] Use Replaceable events for offers
- [x] Delete events when cj completed
    - [x] Use ephemeral events for messages
    - [x] Delete maker offer
- [x] Maker should republish offer after completed Coinjoins
    - [ ] New key with proof of work
- [ ] When maker sends inputs should sign message to prove ownership
- [ ] Taker should handle makers not responding 
    - [ ] At input collection
    - [ ] At signing
- [x] Maker republish offer if taker doesn't not respond 
- [ ] Select maker to broadcast transaction (maybe maker doesnt even need to be one of the ones in CJ)
- [ ] Fidelity Bond (it'll be a bit)
- [ ] Cleanup and add tests
- [ ] Add print outs 
- [ ] Add Docs

### A Note on Forks
My fork of [rust-bitcoin-rpc](https://github.com/rust-bitcoin/rust-bitcoincore-rpc) is required as the decode psbt function is not merged upstream. 
I do intend to clean this up and create a PR to merge upstream as I would rather not depend on forks. 

## License
Code is under the BSD 3-Clause License ([LICENSE](LICENSE) or [https://opensource.org/licenses/BSD-3-Clause](https://opensource.org/licenses/BSD-3-Clause))  

