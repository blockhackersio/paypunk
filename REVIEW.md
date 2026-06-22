- Shift+Tab should work everywhere that Tab does
- Balance does not appear to work correctly:
    ```bash
    ❯ ./get_balance.sh 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
    10000000000000000000000 wei
    ```
    However when building and running ./target/debug/paypunk using 
    "test test test test test test test test test test test junk"

    The address appears correct however the balance remains 0

- We should use the wallet address and name at every page it makes sense.
