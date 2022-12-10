## nostrdizer

This document explains the order flow of nostrdizer.

--- 

## Overview

nostrdizer uses both replaceable and ephemeral events. Replaceable events are used by the maker to publish an `offer`.
ephemeral event are used for the coordination of a ephemeral transaction.

## Message Kinds

| Message Kind | `kind` |     type   | sender |
| -------------|--------|------------| ------ |
| Offer        | 10124  | Replaceable| Maker  |
| FillOffer    | 20125  | Ephemeral  | Taker  |
| MakerInput   | 20126  | Ephemeral  | Maker  |
| UnsignedCJ   | 20127  | Ephemeral  | Taker  |
| SignedCJ     | 20128  | Ephemeral  | Maker  |


## Offer 
Offer events are used by the maker to publish the parameter of collaborative transactions they are willing to participate in.
Contents of an offer event:
`offer_id` `u32`
`abs_fee` `Amount` the absolute fee the maker expects to receive
`rel_fee` `f64` the percent of send amount maker expects as a fee
`minsize` `Amount` the minimum size of the send amount 
`maxsize` `Amount` the maximum size of the send amount
`will_brodcast` `bool` if the maker is willing to broadcast the final transaction

## Fill Offer
Taker sends a `filloffer` to the maker to alert them they would like to use them in a transaction
Encrypted contents of a `filloffer` event:
`offer_id` `u32` of the maker offer they are filling
`send_amount` `Amount` the amount of BTC

## Maker Input 
The maker responds to a `filloffer` event with its inputs to be used by the taker to construct the transaction.
Encrypted content of the `makerinput` event:
`offer_id` `u32`
`inputs` `Vec<(Txid, vout)>`
`cj_out_address` `Address` Bitcoin address where send amount should be sent 
`change_address` `Address` Bitcoin address for change 

## UnsignedCJ
The taker constructs the transaction and sends to makers.
Encrypted contents of the `unsignedcj` event:
`offer_id` `u32`
`psbt` `String`

## SignedCJ
Maker verifies the CJ transactions and signs responding with the signed transaction
Encrypted contents of `signedcj` event:
`offer_id` `u32`
`psbt` `String`


