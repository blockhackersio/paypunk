# Agent Task: Tauri v2 + Konsta UI "Kitchen Sink" Android App (NixOS / devenv)

## Objective

Scaffold, build, and run a **Tauri v2** mobile application that serves as a **Konsta UI component "kitchen sink"** — a single app demonstrating all the main Konsta UI components — and prove the full loop end to end:

1. Data flows **from Rust into the UI** via Tauri commands (and at least one Rust→UI event).
2. The app **compiles to an Android APK** and **installs on a connected physical phone via `adb`**.
3. The UI can be **iterated locally and viewed in a normal web browser** (the developer browses to the dev-server URL).
4. There is a documented path to build a **signed release APK** and publish it to **GitHub Releases** for sideloading.

Treat this as a throwaway "kick the tyres" project, but make it clean enough to copy from later.

## Hard constraints — read before doing anything

- **OS is NixOS.** There is **no global package manager** and **no Android Studio**. Every dependency (Rust toolchain + Android Rust targets, Node + a JS package manager, JDK, Android SDK/NDK/platform-tools/`adb`, and the Linux system libs Tauri needs such as WebKitGTK) **must be provided through `devenv` (version 2.1.2)** via a checked-in `devenv.nix` (+ `devenv.yaml`). The contributor must be able to get a working environment with a single `devenv shell` (or `direnv` + `use devenv`).
- **The dev environment is a headless VM the developer reaches over a terminal**, and GUI windows are forwarded to the host via **waypipe**. GPU acceleration is unavailable; do not rely on it. This is *fine* for this stack because the UI renders as HTML/CSS in a webview/browser, not via accelerated graphics — keep it that way.
- **Device access is over the network, not USB.** Assume the phone is reachable via `adb connect <phone-ip>:<port>` (wireless debugging), not USB passthrough. The dev server must be reachable from the phone (see networking notes).
- **Do not rely on your training data for version-specific commands, Nix expressions, or library setup steps** — these move fast. Verify against the authoritative sources listed at the end, and confirm tool versions at runtime (`devenv version`, `cargo tauri info`, `node --version`, `adb --version`).

## Chosen stack

- **Tauri v2** (Rust backend, system WebView frontend container).
- **Frontend:** React + TypeScript + **Vite**.
- **Styling:** Tailwind CSS (current major) + **Konsta UI** (`konsta`), using Konsta's dual iOS / Material theme.
- **Package manager:** `pnpm` (acceptable to substitute npm if simpler under Nix — state the choice in the README).

If any of these have changed materially by the time you run this (e.g. Konsta's Tailwind setup), follow the official install docs for the *current* versions rather than what is written here.

---

## Deliverables (acceptance criteria)

A single git repository containing:

1. `devenv.nix` / `devenv.yaml` that provisions the **entire** toolchain (no host installs, no Android Studio). `devenv shell` must yield an environment where `cargo tauri info` reports Android prerequisites satisfied.
2. A Tauri v2 project (`src-tauri/` + React/Vite frontend) where the frontend is a **Konsta kitchen sink** covering the components listed below.
3. **Rust → UI data**: at least three `#[tauri::command]` functions and one emitted event, all surfaced visibly in the UI.
4. **UI → Rust → persisted data round-trip**: a UI control triggers a Rust command that mutates state persisted to disk by Rust, the UI reflects the change, and **the change survives an app restart**.
5. A **browser-friendly** frontend: when run in a plain browser (no Tauri runtime present), the app must still render and degrade gracefully (mocked command data) — see "Local iteration".
6. A `README.md` documenting: enter the environment, run in browser, run on device, build a signed release, and publish to GitHub Releases.
7. A working `tauri android dev` run on the physical phone, plus a built **debug APK installed via `adb`**.
8. A **GitHub Actions workflow** that, on a version tag, builds a **signed release APK** and attaches it to a GitHub Release.
9. A short `KNOWN_ISSUES.md` capturing any NixOS/Android friction you hit and how you resolved it.

---

## Functional requirements

### Konsta "kitchen sink"

Build a tabbed app (use Konsta `Tabbar`/`Page` navigation) whose screens collectively exercise the **main** Konsta components. Include, at minimum:

- Layout/chrome: `App` (with theme), `Page`, `Navbar`, `Toolbar`/`Tabbar`, `Block`, `BlockTitle`, `BlockFooter`.
- Actions & inputs: `Button` (all variants/sizes), `Link`, `List` + `ListItem`, `ListInput`, `Checkbox`, `Radio`, `Toggle`, `Range`, `Segmented`, `Stepper`, `Searchbar`.
- Content & feedback: `Card`, `Chip`, `Badge`, `Preloader`, `Progressbar`, `Notification`, `Fab`.
- Overlays: `Popup`, `Sheet`, `Dialog`, `Popover`, `Actions`, `Menu`/`MenuList` (where available).
- A visible **iOS ⇄ Material theme switch** wired to Konsta's `theme` so reviewers can see both design languages.

It is fine to organise these across 3–5 tabs (e.g. "Inputs", "Lists", "Overlays", "Feedback", "About"). Every component must be visible and interactable, not just imported.

### Rust → UI data (the point of the exercise)

Demonstrate the IPC boundary clearly:

- A command returning structured data (e.g. `get_app_info()` → app name, version, target triple, build profile) rendered in an "About"/"System" list.
- A command taking an argument and returning a computed result (e.g. `greet(name) -> String`) wired to a `ListInput` + `Button`, result shown in a `Notification` or `Block`.
- A command returning a `Vec<Item>` (serde-serialised) that **populates a Konsta `List`** so the list is demonstrably Rust-sourced, not hardcoded.
- One **event emitted from Rust** (e.g. a ticking counter or timer via `app.emit`) that the UI subscribes to with `@tauri-apps/api/event` and displays live (e.g. in a `Badge` or `Progressbar`).

Use `#[tauri::command]`, register via `invoke_handler(tauri::generate_handler![...])`, and call from the frontend with `invoke` from `@tauri-apps/api/core`. Keep the mobile entry point intact (`#[cfg_attr(mobile, tauri::mobile_entry_point)] pub fn run()` in `src-tauri/src/lib.rs`).

### Persisted state round-trip (required)

Demonstrate the full **UI → Rust → persistent storage → UI** loop, not just reads:

- Provide a **UI interaction in the kitchen sink** (e.g. a `Toggle`, `Stepper`, or a `ListInput` + save `Button`) that **invokes a `#[tauri::command]` which mutates app data persisted to disk by Rust** — for example a settings record like `{ theme_preference, launch_count, favourite_color, note }`.
- The Rust side owns persistence. Recommended: the official **Tauri Store plugin** (`tauri-plugin-store` / `@tauri-apps/plugin-store`), or a JSON/SQLite file written by Rust into the app data directory (`app.path().app_data_dir()`). Whatever you choose, the **write must happen in Rust**, triggered by the command — not in the frontend. If you use the store plugin, add its capability/permission under `src-tauri/capabilities/` and verify the current mobile setup against the plugin docs.
- After the command mutates and persists the value, the UI must **reflect the new value** (re-read via a `get_*` command or the command's return value).
- **The real test of "persisted": the value must survive an app restart.** On launch, read it back from disk and display it (e.g. increment and show a `launch_count`, and restore the saved setting into the relevant control). Document the on-disk path used on Android in the README.

This is the headline interaction — make it prominent (e.g. its own "Settings" tab) and easy for a reviewer to exercise and verify across a kill/relaunch.

---

## Recommended implementation order

1. **Environment first.** Author `devenv.nix` providing: a Rust toolchain with the Android targets `aarch64-linux-android`, `armv7-linux-androideabi`, `i686-linux-android`, `x86_64-linux-android`; Node + pnpm; **JDK 17**; the Android SDK + NDK + build-tools + platform-tools (`adb`) via Nix (`androidenv.composeAndroidPackages` or `android-nixpkgs`); and Tauri's Linux system deps (WebKitGTK 4.1 and friends) so desktop/browser dev works. Export `JAVA_HOME`, `ANDROID_HOME`, and **`NDK_HOME`** (the most common failure is `NDK_HOME` unset — Tauri's `android init` hard-fails without it). Verify with `cargo tauri info` inside `devenv shell`. **Consult the NixOS wiki Tauri page and the devenv docs for the current, correct expressions** — do not guess; Android-SDK-on-Nix has sharp edges (read-only SDK in the Nix store, writable `ANDROID_USER_HOME`/`GRADLE_USER_HOME`, license acceptance).
2. **Scaffold Tauri v2** with `create-tauri-app` choosing React + TypeScript + Vite, mobile enabled. Confirm `cargo tauri dev` opens the desktop webview (forwarded over waypipe).
3. **Add Tailwind + Konsta** following Konsta's current React install guide; wire the iOS/Material theme provider and the theme toggle.
4. **Build the kitchen sink UI** screen by screen.
5. **Add the Rust commands + event** and wire them into the UI, including the **browser fallback** (below).
6. **Android:** `cargo tauri android init` (generates `src-tauri/gen/android`), then `cargo tauri android dev` against the networked phone; then build and `adb install` a debug APK.
7. **Release signing + GitHub Actions** last.

---

## Local iteration & testing in a browser

Two loops — make both work and document them:

- **Webview loop (full IPC):** `cargo tauri dev`. Launches the app in the system WebView window, forwarded to the host via waypipe. Tauri commands/events work here.
- **Pure-browser loop (fast UI iteration, the preferred loop):** run the Vite dev server alone (e.g. `pnpm dev`) and **open it in a normal browser pointed at the dev URL** (the developer browses over waypipe to the VM's dev-server URL). This is the fast, GPU-free loop for styling and component work.

**Critical:** in the pure-browser loop the Tauri runtime is **not** present, so `invoke()` will throw and events never fire. Implement a small abstraction layer for all backend calls that detects the Tauri runtime (e.g. check for `window.__TAURI_INTERNALS__`, or wrap `invoke` in try/catch) and returns **mock data** when absent, so the kitchen sink renders fully in a plain browser. Make the mock obvious (e.g. an "About" field showing `source: mock` vs `source: rust`). The persisted round-trip must also degrade gracefully in-browser — back it with an in-memory (or `localStorage`) mock so the control still works, while the **real disk persistence only happens through Rust** in the webview/device builds.

Bind the Vite dev server to `0.0.0.0` and document the exact URL to browse to, so the browser (and later the phone) can reach it across the VM network boundary.

---

## Running on the physical phone (adb over network)

- Enable wireless debugging on the phone; from inside the VM `adb connect <phone-ip>:<port>` and confirm with `adb devices`.
- `cargo tauri android dev` runs with live reload using debug signing. For a **physical device the dev server must be reachable over the network**: Tauri replaces `localhost` in the dev URL with the host's IP and sets `TAURI_DEV_HOST`. In this VM topology you will likely need to **set `TAURI_DEV_HOST` explicitly** to the VM's network-reachable IP and ensure the phone can reach that IP/port (firewall, same subnet). Document the exact value used.
- To install a built APK directly: `cargo tauri android build --apk` then `adb install -r <path-to-apk>`. Note the default build produces an **unsigned** APK (see release section) — a debug build is fine for first install.
- Watch the 16KB-page-size requirement: building with **NDK r28+** produces compliant binaries; flag in `KNOWN_ISSUES.md` if an older NDK is pulled by Nix.

---

## Release build for sideloading via GitHub Releases

1. **Create a keystore once** (CLI only, no Android Studio): `keytool -genkeypair -v -keystore release-key.jks -keyalg RSA -keysize 2048 -validity 10000 -alias release`. Never commit the keystore or passwords.
2. **Wire signing into the generated Gradle project**: in `src-tauri/gen/android/app/build.gradle.kts` add a `release` `signingConfigs` block that reads `storeFile`/`storePassword`/`keyAlias`/`keyPassword` from Gradle properties (e.g. a `keystore.properties` file or `-P` flags / env), and reference it from the `release` build type. Because `gen/android` is generated, either commit it to the repo or script re-application after `android init`; document which approach you chose.
3. **Build the signed APK**: `cargo tauri android build --apk` (use `--aab` only if Play Store is ever wanted; for sideloading, APK). Confirm `versionCode` (Tauri auto-derives it from the app version as `major*1000000 + minor*1000 + patch`; document how to bump).
4. **GitHub Actions release workflow** (`.github/workflows/release-android.yml`): trigger on tag push (`v*`). On an Ubuntu runner: check out, install Rust + Android targets, set up JDK 17, install the Android SDK/NDK (NDK r28+), install Node + pnpm, `pnpm install`, `cargo tauri android init`, inject the keystore from repository **secrets** (base64-decode the `.jks` into the runner; pass passwords via secrets), `cargo tauri android build --apk`, then attach the signed APK to a GitHub Release. Print the SHA-256 of the APK in the job log and include it in the release notes so sideloaders can verify the download.

State clearly in the README that sideloaded APKs require "install unknown apps" permission on the device, and that users should verify the published SHA-256.

---

## Known gotchas to handle (and note in KNOWN_ISSUES.md)

- `NDK_HOME` (and `ANDROID_NDK_HOME`) must be exported or `tauri android init` fails even with the NDK installed.
- Android SDK from the Nix store is read-only; Gradle/Tauri may need writable `ANDROID_USER_HOME`/`GRADLE_USER_HOME` and accepted SDK licenses.
- Physical-device dev needs a network-reachable dev host (`TAURI_DEV_HOST`); default `localhost` will not work from the phone.
- Cleartext-traffic / `usesCleartextTraffic` may matter for the dev-server connection during `android dev`.
- Pure-browser iteration has no Tauri runtime — the mock fallback is mandatory or the UI breaks.
- NDK r28+ for 16KB page alignment.

## Authoritative references to consult (do not work from memory)

- Tauri v2 prerequisites: https://v2.tauri.app/start/prerequisites/
- Tauri v2 — building/distributing for Android (signing, APK/AAB, versionCode): the Distribute → Android section of https://v2.tauri.app
- Tauri NixOS setup: the Tauri page on the NixOS Wiki (linked from the Tauri prerequisites page)
- Konsta UI install + components (pick the React guide): https://konstaui.com
- devenv docs (languages, android, scripts): https://devenv.sh — confirm features available in **2.1.2**
- daisyUI/Tailwind are not required here; Konsta provides the components.

## Definition of done

`devenv shell` → `pnpm dev` renders the full kitchen sink in a browser with mocked data; `cargo tauri dev` renders it in the webview with **live Rust data**; a UI control triggers a Rust command that **persists data to disk and the value survives killing and relaunching the app**; `cargo tauri android dev` runs it on the networked phone with live Rust data and the same persistence across restart; a debug APK installs via `adb`; a tagged push produces a **signed release APK** attached to a GitHub Release with a published SHA-256. README + KNOWN_ISSUES document every step and every workaround.
