use incrementalmerkletree::{Hashable, Level};
use orchard::circuit::ProvingKey;
use orchard::keys::{FullViewingKey, Scope, SpendAuthorizingKey, SpendingKey};
use orchard::note::{ExtractedNoteCommitment, RandomSeed, Rho};
use orchard::tree::MerkleHashOrchard;
use orchard::value::NoteValue;
use pczt::roles::{
    creator::Creator, io_finalizer::IoFinalizer, prover::Prover, signer::Signer,
    spend_finalizer::SpendFinalizer, tx_extractor::TransactionExtractor,
};
use rand_core::OsRng;
use secp256k1::{Secp256k1, SecretKey};
use zcash_primitives::transaction::builder::{BuildConfig, Builder};
use zcash_primitives::transaction::fees::zip317;
use zcash_protocol::consensus::BlockHeight;
use zcash_protocol::local_consensus::LocalNetwork;
use zcash_protocol::memo::MemoBytes;
use zcash_protocol::value::Zatoshis;
use zcash_transparent::address::TransparentAddress;
use zcash_transparent::bundle::{OutPoint, TxOut};
use zcash_transparent::util::hash160;

/// Build a shielded Orchard PCZT and run it through the full pipeline:
/// create → finalize IO → prove → sign → finalize spends → extract.
/// Verifies the extracted transaction is structurally valid.
#[test]
fn test_orchard_shielded_pczt_full_pipeline() {
    let params = LocalNetwork {
        overwinter: Some(BlockHeight::from_u32(1)),
        sapling: Some(BlockHeight::from_u32(1)),
        blossom: Some(BlockHeight::from_u32(1)),
        heartwood: Some(BlockHeight::from_u32(1)),
        canopy: Some(BlockHeight::from_u32(1)),
        nu5: Some(BlockHeight::from_u32(1)),
        nu6: Some(BlockHeight::from_u32(1)),
        nu6_1: Some(BlockHeight::from_u32(1)),
    };
    let target_height = BlockHeight::from_u32(10);

    // ── 1. Generate keys and create a note ──────────────────────────────
    let sk = SpendingKey::from_zip32_seed(&[0xab; 32], 133, zip32::AccountId::try_from(0).unwrap())
        .expect("SpendingKey from seed");
    let fvk = FullViewingKey::from(&sk);
    let ask = SpendAuthorizingKey::from(&sk);
    let recipient = fvk.address_at(0u32, Scope::External);

    let value = NoteValue::from_raw(60_000);
    let rho = Rho::from_bytes(&[7; 32]).into_option().unwrap();
    let rseed = RandomSeed::from_bytes([8u8; 32], &rho).into_option().unwrap();
    let note = orchard::Note::from_parts(recipient, value, rho, rseed).unwrap();

    // ── 2. Compute merkle path for a single note at position 0 in an
    //       otherwise empty tree. The auth path is all empty roots.
    let cmx: ExtractedNoteCommitment = note.commitment().into();
    let auth_path: [MerkleHashOrchard; 32] = core::array::from_fn(|i| {
        MerkleHashOrchard::empty_root(Level::from(i as u8))
    });
    let merkle_path = orchard::tree::MerklePath::from_parts(0, auth_path);
    let anchor = merkle_path.root(cmx);

    // ── 3. Build transaction via zcash_primitives Builder ────────────────
    let mut builder = Builder::new(
        &params,
        target_height,
        BuildConfig::Standard {
            sapling_anchor: None,
            orchard_anchor: Some(anchor),
        },
    );

    builder
        .add_orchard_spend::<zip317::FeeError>(fvk.clone(), note, merkle_path)
        .expect("add_orchard_spend");

    // Output (change): send remaining value back to ourselves
    let change_addr = fvk.address_at(1u32, Scope::External);
    builder
        .add_orchard_output::<zip317::FeeError>(
            Some(fvk.to_ovk(Scope::Internal)),
            change_addr,
            Zatoshis::from_u64(50_000).unwrap(),
            MemoBytes::empty(),
        )
        .expect("add_orchard_output");

    // ── 4. Build for PCZT → Creator → IoFinalizer ───────────────────────
    let pczt_result = builder
        .build_for_pczt(OsRng, &zip317::FeeRule::standard())
        .expect("build_for_pczt");

    let created = Creator::build_from_parts(pczt_result.pczt_parts)
        .expect("Creator::build_from_parts");

    let io_finalized = IoFinalizer::new(created)
        .finalize_io()
        .expect("IoFinalizer::finalize_io");

    // ── 5. Prove (Prover role) ──────────────────────────────────────────
    let pk = ProvingKey::build();
    let proven = Prover::new(io_finalized)
        .create_orchard_proof(&pk)
        .expect("create_orchard_proof")
        .finish();

    // ── 6. Sign (Signer role) ───────────────────────────────────────────
    // The orchard builder may add dummy actions for padding; try signing
    // each action and skip those with mismatched keys.
    let mut signer = Signer::new(proven).expect("Signer::new");
    for i in 0..10 {
        match signer.sign_orchard(i, &ask) {
            Ok(()) => break,
            Err(pczt::roles::signer::Error::InvalidIndex) => break,
            Err(_) => continue,
        }
    }
    let signed = signer.finish();

    // ── 7. Finalize spends + extract transaction ────────────────────────
    let finalized = SpendFinalizer::new(signed)
        .finalize_spends()
        .expect("SpendFinalizer::finalize_spends");

    let orchard_vk = orchard::circuit::VerifyingKey::build();
    let tx = TransactionExtractor::new(finalized)
        .with_orchard(&orchard_vk)
        .extract()
        .expect("TransactionExtractor::extract");

    // ── 8. Verify ───────────────────────────────────────────────────────
    let orchard_bundle = tx.orchard_bundle().expect("orchard bundle");
    // Builder pads to at least 2 actions; 1 real + 1 dummy
    assert_eq!(orchard_bundle.actions().len(), 2);
}

/// Simplest possible PCZT construction test:
/// build a transparent-input-only transaction through the full PCZT pipeline
/// and verify round-trip serialization.
#[test]
fn test_construct_raw_pczt_inline() {
    // All upgrades active at height 1 (regtest-like)
    let params = LocalNetwork {
        overwinter: Some(BlockHeight::from_u32(1)),
        sapling: Some(BlockHeight::from_u32(1)),
        blossom: Some(BlockHeight::from_u32(1)),
        heartwood: Some(BlockHeight::from_u32(1)),
        canopy: Some(BlockHeight::from_u32(1)),
        nu5: Some(BlockHeight::from_u32(1)),
        nu6: Some(BlockHeight::from_u32(1)),
        nu6_1: Some(BlockHeight::from_u32(1)),
    };
    let target_height = BlockHeight::from_u32(10);

    let mut builder = Builder::new(
        &params,
        target_height,
        BuildConfig::Standard {
            sapling_anchor: None,
            orchard_anchor: None,
        },
    );

    // Generate a real secp256k1 keypair so that the transparent input
    // validation in TransparentInputInfo::from_parts passes.
    let secp = Secp256k1::new();
    let sk = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let pk = secp256k1::PublicKey::from_secret_key(&secp, &sk);
    let hash = hash160::hash(&pk.serialize());
    let addr = TransparentAddress::PublicKeyHash(hash);
    let coin = TxOut::new(Zatoshis::from_u64(100_000).unwrap(), addr.script().into());
    let outpoint = OutPoint::new([0u8; 32], 0);

    builder
        .add_transparent_p2pkh_input(pk, outpoint, coin)
        .expect("add_transparent_p2pkh_input");

    // Transparent output (recipient)
    let to = TransparentAddress::PublicKeyHash([0u8; 20]);
    builder
        .add_transparent_output(&to, Zatoshis::from_u64(50_000).unwrap())
        .expect("add_transparent_output");

    // Change output — builder requires balance to be zero after fees
    let change_addr = TransparentAddress::PublicKeyHash([1u8; 20]);
    builder
        .add_transparent_output(&change_addr, Zatoshis::from_u64(40_000).unwrap())
        .expect("add_change_output");

    // Build → Creator → IoFinalizer
    let pczt_result = builder
        .build_for_pczt(OsRng, &zip317::FeeRule::standard())
        .expect("build_for_pczt");

    let created = Creator::build_from_parts(pczt_result.pczt_parts)
        .expect("Creator::build_from_parts");

    let io_finalized = IoFinalizer::new(created)
        .finalize_io()
        .expect("IoFinalizer::finalize_io");

    // Round-trip serialization — verify the PCZT structure is correct
    let bytes = io_finalized.serialize();
    let parsed = pczt::Pczt::parse(&bytes).expect("Pczt::parse");
    assert_eq!(parsed.transparent().inputs().len(), 1);
    assert_eq!(parsed.transparent().outputs().len(), 2);

    // Verify the parsed PCZT has the expected global fields
    assert_eq!(*parsed.global().tx_version(), 5);
    assert!(parsed.sapling().spends().is_empty());
    assert!(parsed.orchard().actions().is_empty());
}
