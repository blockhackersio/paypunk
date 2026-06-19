
# Create account password requirement

## Current state
- `create_account` requires a password but this is not a great user experience as the user must enter their password every time they create a new account. For some chains such as Ethereum this could be avoided because it is possible to create wallets based on the HD wallet public key. But for others such as zcash this is more complex.

## Suggestion
The user flow should be assuming preexisting wallet:
    1. screen 1: greeting screen -> enter password
    3. screen 2: accounts -> create account 
    4. transact etc.

I suggest we look at doing the following:

- make submitting the greeting form to enter password 
    - unlock the paypunkd database (and start the service - take care around password safety)
    - query the database for any public keymaterial for the first 30 keys on the standard derivation for the protocol
    - go to the service and bulk extract public key data so that we can create multiple accounts and then save that data to the database.



# Daemon spawning

- How do we know that keypunkd/paypunkd is just available? I would have thought we would want to run this under the cli eg. `paypunk keypunkd` -> launch keypunkd | `paypunk paypunkd` -> launch paypunkd
- The shutdown atomic bool is cloned to the ctrl+c capture but it is not sent to the paypunk_tui? Is this a problem?


# PROBLEM: There is a hardcoded password in the config

This should be removed configuration is no place for passwords.

The db password should function something like this:
1. tui launches and shows greeting form with password field
2. daemons boot up
3. user submits password
4. api accepts password derives argon2 salted padded key for unlocking the db (as well as extracting public key material if required - see above)
5. api sends padded password encrypted to paypunkd's public key alongside plain password encrypted to keypunkds public key (as is the case already).
6. paypunkd decrypts the padded salted password and uses that to unlock the db and start up
7. paypunkd queries the db if it has any account view keys - if not it will forward the encrypted key to keypunkd to bulk request keys as above
8. keypunkd returns the bulk keys to paypunkd and paypunkd stores them in the db



# PROBLEM: There are still hardcoded socket location strings. 

We should apply the configuration every where it is used.
