## nostrdizer

This document explains the order flow of nostrdizer.

--- 

## Overview

nostrdizer uses both replaceable and ephemeral events. Replaceable events are used by the maker to publish an `offer`.
ephemeral event are used for the coordination of a transaction.

## Message Kinds

| Message Kind        | `kind` |     type   | sender |
| ------------------- |--------|------------| ------ |
| Absolute Offer      | 10123  | Replaceable| Maker  |
| Relative Offer      | 10124  | Replaceable| Maker  |
| Fill                | 20125  | Ephemeral  | Taker  |
| IoAuth              | 20126  | Ephemeral  | Maker  |
| Transaction         | 20127  | Ephemeral  | Taker  |
| SignedTransaction   | 20128  | Ephemeral  | Maker  |


## Offer 
Offer events are used by the maker to publish the parameter of collaborative transactions they are willing to participate in.

### Relative Offer
Contents of an relative offer event:
- `oid` `u32`
- `minsize` `Amount` The minimum amount CJ a maker will partake in
- `maxsize` `Amount` The maximum amount CJ a maker will partake in 
- `txfee` `Amount` The amount the maker will contribute to mining fee 
- `cjfee` `f64` The percent as a decimal the maker expects 
- `nick_signature` `String` 

### Absolute Offer
Contents of an absolute offer event:
- `oid` `u32`
- `minsize` `Amount` The minimum amount CJ a maker will partake in
- `maxsize` `Amount` The maximum amount CJ a maker will partake in 
- `txfee` `Amount` The amount the maker will contribute to mining fee
- `cjfee` `Amount` The amount the maker expects 
- `nick_signature` `String` 
---

## Fill
Taker sends a `fill` to the maker to alert them they would like to use them in a transaction
Encrypted contents of a `fill` event:
- `offer_id` `u32` of the maker offer they are filling
- `amount` `Amount` the amount of BTC
- `tencpubkey` `String` taker pubkey used
- `commitment` `String` hash of P2
- `nick_signature` `String` 
---

## Io Auth 
The maker responds to a `filloffer` event with its inputs to be used by the taker to construct the transaction.
Encrypted content of the `IoAuth` event:
- `utxos` `Vec<(Txid, vout)>`
- `maker_auth_pub` `String`
- `coinjoin_address` `Address` Bitcoin address where send amount should be sent 
- `change_address` `Address` Bitcoin address for change 
- `bitcoin_sig` `String` bitcoin signature of mencpubkey
- `nick_signature` `String`
---

## Transaction
The taker constructs the transaction and sends to makers.
Encrypted contents of the `Transaction` event:
- `tx` `String` of raw transaction hex
- `nick_signature` `String`
---

## SignedTransaction
Maker verifies the CJ transactions and signs responding with the signed transaction
Encrypted contents of `SignedTransaction` event:
- `tx` `String` of raw transaction hex
- `nick_signature` `String`


