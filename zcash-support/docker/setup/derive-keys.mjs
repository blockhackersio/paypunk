/**
 * derive-keys.mjs
 *
 * Derives a Zcash regtest transparent address (tm…) and WIF private key
 * from the well-known Hardhat "test junk" BIP-39 mnemonic, using the
 * BIP-44 derivation path m/44'/133'/0'/0/<index>.
 *
 * Coin type 133 = Zcash (registered in SLIP-44).
 *
 * Usage:
 *   node derive-keys.mjs              # derive account 0, index 0
 *   node derive-keys.mjs 3            # derive account 0, index 3
 *   node derive-keys.mjs --all        # derive indices 0-9
 *   node derive-keys.mjs --json       # machine-readable JSON output
 */

import { HDKey } from "@scure/bip32";
import { mnemonicToSeedSync } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english";
import { base58check } from "@scure/base";
import { sha256 } from "@noble/hashes/sha256";
import { ripemd160 } from "@noble/hashes/ripemd160";

// ── The Hardhat canonical test mnemonic ──────────────────────────────
const MNEMONIC =
  "test test test test test test test test test test test junk";

// ── Zcash address encoding constants ─────────────────────────────────
//
// Zcash transparent addresses use a TWO-byte version prefix (unlike
// Bitcoin's single byte).  We need to handle the base58check encoding
// manually because the @scure/base encoder expects a single-byte prefix
// in its helper, so we just do raw base58check with the two-byte prefix
// prepended to the 20-byte pubkey hash.
//
// Mainnet P2PKH: 0x1CB8  →  addresses start with "t1"
// Testnet P2PKH: 0x1D25  →  addresses start with "tm"
// Regtest reuses testnet prefixes.

const TESTNET_P2PKH_PREFIX = new Uint8Array([0x1d, 0x25]);
const TESTNET_WIF_PREFIX = 0xef; // same as Bitcoin testnet

const b58c = base58check(sha256);

// ── Helpers ──────────────────────────────────────────────────────────

function hash160(data) {
  return ripemd160(sha256(data));
}

function toTaddr(pubkey) {
  const h = hash160(pubkey);
  // Prepend the two-byte version prefix, then base58check-encode the lot.
  const payload = new Uint8Array(2 + 20);
  payload.set(TESTNET_P2PKH_PREFIX, 0);
  payload.set(h, 2);
  return b58c.encode(payload);
}

function toWIF(privkey) {
  // WIF = base58check( version(1) || key(32) || compress_flag(1) )
  const payload = new Uint8Array(1 + 32 + 1);
  payload[0] = TESTNET_WIF_PREFIX;
  payload.set(privkey, 1);
  payload[34] = 0x01; // compressed
  return b58c.encode(payload);
}

function deriveIndex(root, accountIndex, addressIndex) {
  // BIP-44: m / purpose' / coin_type' / account' / change / address_index
  const path = `m/44'/133'/${accountIndex}'/0/${addressIndex}`;
  const child = root.derive(path);
  if (!child.privateKey || !child.publicKey) {
    throw new Error(`Derivation failed for ${path}`);
  }
  return {
    path,
    taddr: toTaddr(child.publicKey),
    wif: toWIF(child.privateKey),
    pubkey: Buffer.from(child.publicKey).toString("hex"),
  };
}

// ── Main ─────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
const jsonMode = args.includes("--json");
const allMode = args.includes("--all");

const seed = mnemonicToSeedSync(MNEMONIC);
const root = HDKey.fromMasterSeed(seed);

const indices = allMode
  ? [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
  : [parseInt(args.find((a) => !a.startsWith("--")) ?? "0", 10)];

const results = indices.map((i) => deriveIndex(root, 0, i));

if (jsonMode) {
  console.log(JSON.stringify(results, null, 2));
} else {
  console.log(`\nMnemonic : ${MNEMONIC}`);
  console.log(`Coin     : Zcash (SLIP-44 type 133)`);
  console.log(`Network  : testnet / regtest\n`);
  console.log("─".repeat(72));

  for (const r of results) {
    console.log(`  Path    : ${r.path}`);
    console.log(`  Address : ${r.taddr}`);
    console.log(`  WIF     : ${r.wif}`);
    console.log(`  PubKey  : ${r.pubkey}`);
    console.log("─".repeat(72));
  }
}
