Currently we have some issues with the way that the protocol traits are setup.

- In our test pczt is created using the zcash_primitizves builder. This is not realistic we need to use the facilities provided by zcash_client_backend
- We must keep Protocol static. The reason for this is that we will want to run the Wallet sync process separately and simply hold a reference to whatever db wrapper we need in order to query for it's state.
- We need to do the setup for our pczt test using the Protocol traits
- We need the Protocol traits to be able to be polymorphic over other blockchains such as ethereum and bitcoin.

I think the first thing we need to do here is to research exactly what APIs zcash offers and how best we should consider testing creating a pczt transaction knowing the above constraints.

One idea I had was that we would consider a TransactionBuilder that held an Arc reference to a shared WalletDb that handled protocol syncing in a separte thread while the TransactionBuilder could build out the pczt based on the current data. We could then pass that TransactionBuilder to a method on Protocol that used it to setup the transaction. I don't love this and am not married to it so best to explore what APIs are available and put forward what the options are that still conform to what we could expect from a general chain API.
