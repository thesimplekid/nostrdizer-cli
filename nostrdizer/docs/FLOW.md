## nostrdizer

This document explains the order flow of nostrdizer.

--- 

## Overview

nostrdizer uses both replaceable and ephemeral events. Replaceable events are used by the maker to publish an `offer`.
ephemeral event are used for the coordination of a transaction.

## Message Kinds

| Message Kind  | `kind` |     type   | sender |
| ------------- |--------|------------| ------ |
| Absolute Offer| 10123  | Replaceable| Maker  |
| Relative Offer| 10124  | Replaceable| Maker  |
| Fill          | 20125  | Ephemeral  | Taker  |
| MakerInput    | 20126  | Ephemeral  | Maker  |
| UnsignedCJ    | 20127  | Ephemeral  | Taker  |
| SignedCJ      | 20128  | Ephemeral  | Maker  |


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


## Fill
Taker sends a `fill` to the maker to alert them they would like to use them in a transaction
Encrypted contents of a `filloffer` event:
- `offer_id` `u32` of the maker offer they are filling
- `amount` `Amount` the amount of BTC
- `tencpubkey` `String` taker pubkey used
- `commitment` `String` hash of P2
- `nick_signature` `String` 

## Maker Input 
The maker responds to a `filloffer` event with its inputs to be used by the taker to construct the transaction.
Encrypted content of the `makerinput` event:
- `offer_id` `u32`
- `inputs` `Vec<(Txid, vout)>`
- `cj_out_address` `Address` Bitcoin address where send amount should be sent 
- `change_address` `Address` Bitcoin address for change 

## UnsignedCJ
The taker constructs the transaction and sends to makers.
Encrypted contents of the `unsignedcj` event:
- `offer_id` `u32`
- `psbt` `String`

## SignedCJ
Maker verifies the CJ transactions and signs responding with the signed transaction
Encrypted contents of `signedcj` event:
- `offer_id` `u32`
- `psbt` `String`


