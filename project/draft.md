- alter the tauri app at ./signer so that it does the following:
    1. show a button that launches QR code scanning
    2. when the user taps the button the QR scanner starts the camera and you scan a QR code
    3. take the bytes from the QR code and send them to the `PongHandler::handle` function - if it returns an error show the error on the screen using the appropriate interface
    4. if the return from the `PongHandler::handle` function is the Ok variant then pass the bytes directly to a QR generator and show that QR code on the screen with a closable popup
    5. Closing the popup takes you back to step 1
- `PongHandler` can be found in the `./pong` crate that is already available as a dependency by the `./signer/src-tauri` app 
- Keep the current screens (kitchen sink UI) available but NOT hooked up. Do NOT delete them but do not show them either.
- Try to use konsta components for the interface.
- Only explore `./signer` and `./pong` and workspace files the rest of the codebase is not important for this task.

