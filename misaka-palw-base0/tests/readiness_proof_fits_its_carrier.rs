//! **A possession proof has to fit the carrier that takes it to the chain** (ADR-0133 §11.2).
//!
//! The V2 proof opens `PALW_READINESS_V2_CHUNKS_V1` = 16 leaves of the whole artifact, and the
//! consensus cap on what it may carry — `PALW_READINESS_V2_OPERAND_MAX_BYTES_V1` = 1 MiB — is a
//! bare power of two, derived from nothing. Everything else in this tree that rides a carrier
//! derives its ceiling from the carrier: `PALW_OBJECT_CHUNK_MAX_BYTES` is 100,000 because that is
//! the largest round number under the 120,000 bytes a relayable transaction holds.
//!
//! So this measures the real thing: the leaves this span's challenge names, opened from the
//! shipped artifact, assembled into the object a seat actually carries, and weighed against the
//! carrier. It skips loudly without `MISAKA_PALW_ARTIFACT`, because a measurement that quietly did
//! not run is worth less than no measurement.

use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, palw_artifact_multiproof_v1, verify_artifact_multiproof_v1};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_READINESS_V2_BUDGET_BYTES_V1, PALW_READINESS_V2_CHUNKS_V1, PALW_READINESS_V2_FRAME_BYTES_V1,
    PALW_READINESS_V2_LEAF_MAX_BYTES_V1, PALW_READINESS_V2_OPERAND_MAX_BYTES_V1, palw_readiness_v2_challenge_seed_v1,
    palw_readiness_v2_draw_v1, palw_readiness_v2_opening_is_the_challenge_v1,
};
use kaspa_consensus_core::palw_state_v2::{PALW_OBJECT_CHUNK_MAX_BYTES, PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use misaka_palw_base0::classes::*;

/// What one lifecycle carrier can hold: `PALW_OBJECT_CHUNK_MAX_BYTES` is the tree's own answer to
/// "how much may one transaction carry", so a proof above it needs the chunk group and a proof
/// above `PALW_OBJECT_CHUNK_MAX_COUNT` chunks cannot ride at all.
const ONE_CARRIER: usize = PALW_OBJECT_CHUNK_MAX_BYTES;

#[test]
fn the_shipped_artifacts_possession_proof_is_weighed_against_its_carrier() {
    let Ok(path) = std::env::var("MISAKA_PALW_ARTIFACT") else {
        println!("SKIPPED: set MISAKA_PALW_ARTIFACT to a .palwart. No proof was weighed.");
        return;
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let artifact = misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    let row = a16_artifact_row_v1(&court(), &artifact, None, None)
        .unwrap_or_else(|e| panic!("the artifact at {path} names no A16 row: {e}"));
    let digest = misaka_palw_base0::inventory::a16_inventory_digest_v1(&artifact, &row.profile)
        .unwrap_or_else(|e| panic!("the inventory does not digest: {e:?}"));
    let class_id = row.profile.shape_profile_id();
    let leaves: Vec<Hash64> = digest.rows().iter().map(|r| r.leaf_hash).collect();
    let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0));
    let bond_bytes = borsh::to_vec(&bond).expect("a bond key serializes");

    {
        let mut sizes: Vec<u32> = digest.rows().iter().map(|r| r.byte_len).collect();
        sizes.sort_unstable();
        let total: u64 = sizes.iter().map(|b| *b as u64).sum();
        println!(
            "leaf bytes: min {} p50 {} p90 {} p99 {} max {} mean {}",
            sizes[0],
            sizes[sizes.len() / 2],
            sizes[sizes.len() * 9 / 10],
            sizes[sizes.len() * 99 / 100],
            sizes[sizes.len() - 1],
            total / sizes.len() as u64
        );
    }
    let mut largest = 0usize;
    let mut smallest = usize::MAX;
    let mut over_one_carrier = 0usize;
    const SPANS: u64 = 8;
    for span in 1..=SPANS {
        let seed = palw_readiness_v2_challenge_seed_v1(&class_id, &bond_bytes, span);
        // The prover's own rule: the draw's order, stopped at the budget.
        let draw = palw_readiness_v2_draw_v1(&seed, digest.leaf_count());
        let mut opened: Vec<(u32, PalwArtifactOperandV1)> = Vec::with_capacity(draw.len());
        let mut budget = 0usize;
        for index in &draw {
            if budget >= PALW_READINESS_V2_BUDGET_BYTES_V1 {
                break;
            }
            let r = &digest.rows()[*index as usize];
            budget += r.byte_len as usize;
            let row_bytes = misaka_palw_base0::inventory::a16_inventory_row_bytes_v1(&artifact, &row.profile, &r.tensor_name, r.layer, r.row_start)
                .unwrap_or_else(|e| panic!("leaf {index}: {e:?}"))
                .unwrap_or_else(|| panic!("leaf {index} names a row the artifact does not emit"));
            opened.push((
                *index,
                PalwArtifactOperandV1 { tensor_name: r.tensor_name.clone(), layer: r.layer, row_start: r.row_start, bytes: row_bytes },
            ));
        }
        palw_readiness_v2_opening_is_the_challenge_v1(
            &draw,
            &opened.iter().map(|(index, o)| (*index, o.bytes.len())).collect::<Vec<_>>(),
        )
        .expect("the prover's prefix is the one the challenge names");
        opened.sort_by_key(|(index, _)| *index);
        let proof = palw_artifact_multiproof_v1(&leaves, &opened).expect("the opened leaves are the inventory's");
        verify_artifact_multiproof_v1(&proof, digest.root()).expect("the multiproof opens the registered root");
        let object = PalwConsensusObjectV2::SeatReadinessProvedV2 {
            bond,
            class_id,
            span,
            proof: Box::new(proof),
            // The ML-DSA-87 signature a seat actually attaches, by length.
            signature: vec![0u8; 4627],
        };
        let carried = borsh::to_vec(&object).expect("the object serializes").len();
        largest = largest.max(carried);
        smallest = smallest.min(carried);
        if carried > ONE_CARRIER {
            over_one_carrier += 1;
        }
        println!(
            "span {span}: {} of {} drawn leaves, {} operand bytes, carrier payload {carried} bytes ({:.2} carriers)",
            opened.len(),
            draw.len(),
            opened_bytes(&opened),
            carried as f64 / ONE_CARRIER as f64
        );
    }
    println!(
        "class {class_id} — {} leaves of {} bytes; {} of {SPANS} spans need more than one carrier; largest {largest}, smallest {smallest}",
        digest.leaf_count(),
        artifact_bytes(&digest),
        over_one_carrier
    );
    // **The measurement, stated as the property it is about.** A seat that cannot carry its proof
    // is a seat that never becomes ready, and a class whose seats are never ready never leaves
    // PREFETCHING — silently, because nothing on the chain says a proof was refused by a carrier.
    assert!(
        largest <= ONE_CARRIER,
        "this artifact's possession proof is {largest} bytes at its largest and one carrier holds {ONE_CARRIER}: \
         {over_one_carrier} of {SPANS} spans cannot ride, so its seats never prove possession"
    );
    // **The ceiling is the carrier's, by arithmetic** — the property the old `1 << 20` had no way
    // to state. Pinned here beside the measurement that needed it.
    assert_eq!(
        PALW_READINESS_V2_OPERAND_MAX_BYTES_V1 + PALW_READINESS_V2_FRAME_BYTES_V1,
        ONE_CARRIER,
        "the operand ceiling and the frame must be exactly what one carrier holds"
    );
    assert_eq!(PALW_READINESS_V2_OPERAND_MAX_BYTES_V1, PALW_READINESS_V2_BUDGET_BYTES_V1 + PALW_READINESS_V2_LEAF_MAX_BYTES_V1);
    assert_eq!(PALW_READINESS_V2_CHUNKS_V1, 16, "the draw's width moved; this measurement was taken at sixteen");
    // Every leaf of the shipped class is one a proof may open: a class whose rows are larger cannot
    // be proved at all, and that is a fact about the artifact, measured here rather than assumed.
    let biggest = digest.rows().iter().map(|r| r.byte_len as usize).max().unwrap_or(0);
    assert!(
        biggest <= PALW_READINESS_V2_LEAF_MAX_BYTES_V1,
        "the shipped class's largest row is {biggest} bytes and no proof may open a row above \
         {PALW_READINESS_V2_LEAF_MAX_BYTES_V1}"
    );
}

fn opened_bytes(opened: &[(u32, PalwArtifactOperandV1)]) -> usize {
    opened.iter().map(|(_, o)| o.bytes.len()).sum()
}

fn artifact_bytes(digest: &kaspa_consensus_core::palw_artifact::PalwArtifactInventoryDigestV1) -> usize {
    digest.rows().iter().map(|r| r.byte_len as usize).sum()
}

fn court() -> PalwCourtParamsV2 {
    PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("the shipped court params project")
}
