# nostrdizer


A [Nostr](https://github.com/nostr-protocol/nostr) client built to create Bitcoin Collaborative transactions, known as Coinjoins. 
Based on the [Joinmarket](https://github.com/JoinMarket-Org/joinmarket-clientserver) model where there is a maker and a taker. 
A maker is always online available to take part in the transaction, and a taker who chooses when and what size of transaction to create.

To incentivize running a maker the taker pays a small fee to the maker for their service (Not yet implemented). The Maker also has the benefit of gaining privacy from each transaction they participate in so they can choose to set a fee of zero to be more likely to be choses by the taker in a transaction.  

---
**This is Alpha level software with many things that need to be changed, added, improved and tested, please do not use on mainnet.**
---

## Getting started
Currently Bitcoin Core 23 is required as the core RPC api [changed](https://github.com/rust-bitcoin/rust-bitcoincore-rpc/issues/260) between 22 and 23, hopefully I can figure out a way to handle this and older versions will be supported. The end goal will be to remove the requirement to use core at all, and users can choose what backend they want to use for their blockdata and wallet, but support for this will take some time.  

### A Note on Forks
My fork of [nostr_rust](https://github.com/0xtlt/nostr_rust) is currently used as the function to get all private messages hasn't been merged upstream.
Similarly, my fork of [rust-bitcoin-rpc](https://github.com/rust-bitcoin/rust-bitcoincore-rpc) is required as the decode psbt function is not merged upstream. 
I do intend to clean these up and create PRs to merge upstream as I would rather not depend on forks. 
### Run Maker 
```
cargo r -- --rpc-url "<url oof bitcoin core RPC API>" run-maker
```
### Run Taker
```
cargo r -- --rpc-url "<url of bitcoin core RPC API>" send-transaction --send-amount <Send amount> --number-of-makers <number of makers>

```


### Known Issues
- [ ] Maker can't collect fee
- [ ] Offers aren't filtered by Maker fee
- [ ] Fee estimation doesn't work
- [ ] No coin control to prevent mixing change and CJ

### Todo
- [ ] Use Replaceable events for offers (maybe even for DMs)
- [ ] Fidelity Bond (it'll be a bit)
- [ ] Cleanup and add tests
- [ ] Add print outs 
- [ ] Add Docs

## License
Code is under the BSD 3-Clause License ([LICENSE](LICENSE) or [https://opensource.org/licenses/BSD-3-Clause](https://opensource.org/licenses/BSD-3-Clause))  

