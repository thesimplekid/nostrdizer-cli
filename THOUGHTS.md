1) I've back and forth on whether to try and integrate with Joinmarket directly, this would have the advantage of immediate access to a larger liquidity pool. But it would be limiting in that it would be a step away from nostr, forcing compliance with JM's stack this would reduce some of the advantages of nostr. So at this point I'm not going to try and integrate directly with JM and see where nostr takes me. 

2) I would like to integrate BDK into this as it would be a great steps towards making this available on mobile, so I've move bitcoincore related calls to its own module behind a feature and will do the same with BDK, this allows the app builder to choose what backend to use. Ideally I would keep function parody between core and BDK.  I have a feeling this will get messy so will likely have to reevaluate the best way to achieve this.

3) This will suffer from leaking ips to nostr realys, I think for simplicity it makes sense to not worry about that at this level and let apps that integrate the nostrdizer library to handle that. 

4) Nostr also has an issue of all meta data is public, I think this isnt a huge problem, as long as new nostr keys are used for each transaction and nostr keys are not used for other things. But its something to keep in mind, and there maybe new nostr NIPS that could help here.  

5) One possible improvement to ensure that the message sender is the owner of the utxo is to use the bitcoin key pairs to encrypt the message.  The flow for this could be Maker publishes offer with new random key pair. Taker sends fill offer with pubkey generated from utxo they intend to use, Maker uses their original random key to decrypt but then responds from key generated from bitcoin key using that key for remainder of messages. 
