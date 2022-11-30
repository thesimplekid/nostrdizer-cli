# nostrdizer


A [Nostr](https://github.com/nostr-protocol/nostr) Client build to create Bitcoin Collaborative transactions, known as Coinjoins. 
Based on the [Joinmarket](https://github.com/JoinMarket-Org/joinmarket-clientserver) model where there is a maker and a taker. 
A maker is always online available to take part in the transaction, and a taker who chooses when and what size of transaction to create.
To incentivize running a maker the taker pays a small fee to the maker for their service. (Not yet implemented)

---
## This is Alpha level software with many things that need to be changed, added, improved and tested, please do not use on mainnet.
---


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



