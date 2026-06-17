# TUI Dependency Conflict: time-core

## Problem

`paypunk-tui` (ratatui 0.30) and `paypunk-chains-zcash` (via `zcash_client_backend`) cannot coexist in the same Cargo workspace because of a transitive dependency conflict over `time-core`:

```
ratatui 0.30
  → ratatui-widgets 0.3.1
    → time ^0.3.47         (resolves to time ≥0.3.47)
      → time-core =0.1.8   (exact pin)

zcash_client_backend 0.22.x
  → time ^0.3.22
    → time-core =0.1.2     (exact pin)
```

Both `ratatui-widgets` and `zcash_client_backend` pin `time-core` to exact but incompatible versions (`=0.1.8` vs `=0.1.2`). Cargo's resolver cannot select both simultaneously in a single workspace.

## Approaches Considered

### 1. Downgrade ratatui to 0.28/0.29 (REJECTED)

User explicitly forbade changing ratatui from 0.30.

### 2. Upgrade zcash_client_backend (REJECTED)

User explicitly forbade changing zcash_client_backend version.

### 3. `[patch]` time-core to 0.1.2 (WON'T WORK)

```toml
[patch.crates-io]
time-core = "=0.1.2"
```

Cargo's `[patch]` only applies when the patched version satisfies the original version requirement. Since `time` requires `time-core = "=0.1.8"`, a patch providing 0.1.2 does NOT satisfy `=0.1.8` and will be ignored.

### 4. `[patch]` time to an older version (WON'T WORK)

```toml
[patch.crates-io]
time = "=0.3.36"
```

`ratatui-widgets` requires `time = "^0.3.47"` (≥0.3.47). Version 0.3.36 does not satisfy this, so the patch is ignored.

### 5. `[patch]` time to a version ≥0.3.47 that uses time-core 0.1.2 (NEEDS VERIFICATION)

If a version of `time` exists that is both ≥0.3.47 AND uses `time-core =0.1.2`, patching to that version would work. This needs checking — the `time-core` bump from 0.1.2 to 0.1.8 likely happened in a specific `time` release, and we need to find the boundary.

### 6. Fork ratatui-widgets (TOO INVASIVE)

Fork `ratatui-widgets`, change its `time` dependency to `^0.3.22`, and use `[patch]` to point to the fork. This is maintenance-heavy.

### 7. Standalone crate (CURRENT SOLUTION)

`tui/` has its own `[workspace]` table, making it an independent crate with its own Cargo.lock. Both the workspace and TUI compile cleanly.

## Recommendation

Investigate approach 5 first: find the latest `time` version that still uses `time-core =0.1.2`. If it's ≥0.3.47, patch `time` in the workspace. Otherwise, keep the standalone approach.
