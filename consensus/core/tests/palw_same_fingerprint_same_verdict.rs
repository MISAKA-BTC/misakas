//! **ADR-0150 §3.5: same fingerprint ⇒ same verdict.**
//!
//! The manifest makes a rule change visible in the fingerprint *when a developer says so*. This is
//! the backstop for when they do not: a stored corpus of consensus objects — the well-formed, the
//! malformed and the adversarial — is replayed through the stateless validity gate every node
//! applies to a carrier's payload, and the verdicts are digested and pinned.
//!
//! A build that accepts or refuses anything in this corpus differently from the pinned build fails
//! here, and the failure says what to do: if the change is intended, raise the revision of the
//! ruleset that changed in `PALW_CONSENSUS_RULE_MANIFEST_V1` (so the fingerprint moves and nobody
//! mistakes the two builds for one network) and re-pin this digest in the same commit.
//!
//! **It is a sample, not a proof, and its scope is named.** This corpus runs the STATELESS gate —
//! the one every node applies to a carrier's payload before any state is consulted — so a case
//! whose verdict belongs to the fold (a chunk's count and index, a claim's phase, a bond's
//! standing) rides here and is judged elsewhere. What it covers is the class of change that has
//! actually happened in this tree: a bound rewritten, a branch reordered, a constant redefined,
//! where the diff looks local and the verdict is not. Widening it to the fold wants a state
//! fixture and is worth doing; it is not what this file claims today.
//!
//! The corpus is stored (`corpus/palw_object_corpus_v1.txt`, one `label<TAB>hex` per line) rather
//! than generated in this file, so that it survives the generator: `write_the_corpus` rebuilds it
//! deliberately, and a corpus that changes is as much a decision as a rule that changes.

use std::collections::BTreeMap;
use std::path::PathBuf;

use kaspa_consensus_core::palw_artifact::{PalwArtifactMultiproofV1, PalwArtifactOpeningV1, PalwArtifactOperandV1};
use kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2;
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_READINESS_V2_BUDGET_BYTES_V1, PALW_READINESS_V2_LEAF_MAX_BYTES_V1, PALW_READINESS_V2_OPERAND_MAX_BYTES_V1,
};
use kaspa_consensus_core::palw_state_v2::{PALW_OBJECT_CHUNK_MAX_BYTES, PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

/// **The verdicts this build returns for the stored corpus.** Moves only when a verdict moves.
const CORPUS_VERDICT_DIGEST: &str = "f9f74ea144635c4cd4b61c190223954d6646e19a5dd47479492b38793150aa64";

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/palw_object_corpus_v1.txt")
}

fn h64(seed: u8) -> Hash64 {
    Hash64::from_bytes([seed; 64])
}

fn bond(seed: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(h64(seed), 0))
}

fn operand(bytes: usize, seed: u8) -> PalwArtifactOperandV1 {
    PalwArtifactOperandV1 { tensor_name: "w".to_string(), layer: None, row_start: seed as u32 * 8, bytes: vec![seed; bytes] }
}

fn multiproof(opened: Vec<(u32, PalwArtifactOperandV1)>, siblings: usize) -> PalwArtifactMultiproofV1 {
    PalwArtifactMultiproofV1 { leaf_count: 1024, opened, siblings: (0..siblings).map(|i| h64(i as u8)).collect() }
}

/// The objects, by label. Built here so the corpus can be re-generated and read; stored on disk so
/// the test is about the bytes rather than about this function.
fn cases() -> BTreeMap<String, PalwConsensusObjectV2> {
    let mut out: BTreeMap<String, PalwConsensusObjectV2> = BTreeMap::new();
    let readiness_v2 = |label: &str, opened: Vec<(u32, PalwArtifactOperandV1)>, siblings: usize, signature: Vec<u8>| {
        (
            label.to_string(),
            PalwConsensusObjectV2::SeatReadinessProvedV2 {
                bond: bond(1),
                class_id: h64(2),
                span: 7,
                proof: Box::new(multiproof(opened, siblings)),
                signature,
            },
        )
    };
    // **The rule ADR-0150 was written for.** One leaf that reaches the budget is the shape a real
    // proof has; the others are the refusals the ride list owes.
    let one_budget_leaf = vec![(3u32, operand(PALW_READINESS_V2_BUDGET_BYTES_V1, 3))];
    out.extend([
        readiness_v2("readiness_v2/one_leaf_at_the_budget", one_budget_leaf.clone(), 24, vec![9u8; 64]),
        readiness_v2("readiness_v2/unsigned", one_budget_leaf.clone(), 24, Vec::new()),
        readiness_v2("readiness_v2/opens_nothing", Vec::new(), 4, vec![9u8; 64]),
        readiness_v2(
            "readiness_v2/leaf_above_the_ceiling",
            vec![(3, operand(PALW_READINESS_V2_LEAF_MAX_BYTES_V1 + 1, 3))],
            24,
            vec![9u8; 64],
        ),
        readiness_v2(
            "readiness_v2/operands_above_one_carrier",
            (0..4u32).map(|i| (i, operand(PALW_READINESS_V2_OPERAND_MAX_BYTES_V1 / 3, i as u8))).collect(),
            24,
            vec![9u8; 64],
        ),
        readiness_v2(
            "readiness_v2/more_leaves_than_a_challenge_names",
            (0..24u32).map(|i| (i, operand(16, i as u8))).collect(),
            24,
            vec![9u8; 64],
        ),
        readiness_v2("readiness_v2/more_siblings_than_a_tree_needs", one_budget_leaf, 64 * 16 + 1, vec![9u8; 64]),
    ]);
    // The V1 proof it superseded, at and above its own opening cap.
    let opening =
        |bytes: usize| PalwArtifactOpeningV1 { leaf_index: 3, leaf_count: 1024, operand: operand(bytes, 3), path: vec![h64(5)] };
    out.insert(
        "readiness_v1/within_the_opening_cap".to_string(),
        PalwConsensusObjectV2::SeatReadinessProved {
            bond: bond(1),
            class_id: h64(2),
            span: 7,
            opening: opening(4096),
            signature: vec![9u8; 64],
        },
    );
    out.insert(
        "readiness_v1/unsigned".to_string(),
        PalwConsensusObjectV2::SeatReadinessProved {
            bond: bond(1),
            class_id: h64(2),
            span: 7,
            opening: opening(4096),
            signature: Vec::new(),
        },
    );
    // The chunk carriage: the shape a close and a certification ride on.
    for (label, index, count, bytes) in [
        ("chunk/well_formed", 0u8, 2u8, 32usize),
        ("chunk/count_zero", 0, 0, 32),
        ("chunk/index_past_count", 2, 2, 32),
        ("chunk/empty", 0, 2, 0),
        ("chunk/above_one_carrier", 0, 2, PALW_OBJECT_CHUNK_MAX_BYTES + 1),
    ] {
        out.insert(label.to_string(), PalwConsensusObjectV2::ObjectChunk { group: h64(7), index, count, bytes: vec![1u8; bytes] });
    }
    out
}

fn verdict_line(label: &str, object: &PalwConsensusObjectV2) -> String {
    match palw_lifecycle_object_may_ride_v2(object) {
        Ok(()) => format!("{label}\tRIDES"),
        Err(why) => format!("{label}\tREFUSED\t{why}"),
    }
}

fn digest_of(lines: &[String]) -> String {
    let mut h = blake2b_simd::Params::new().hash_length(32).to_state();
    for line in lines {
        h.update(line.as_bytes());
        h.update(b"\n");
    }
    h.finalize().to_hex().to_string()
}

/// **Regenerate the stored corpus.** Ignored by default: it writes a fixture, and a fixture that
/// rewrites itself on every run is not a fixture. Run it deliberately when a case is added.
#[test]
#[ignore = "writes the stored corpus"]
fn write_the_corpus() {
    let mut lines = Vec::new();
    for (label, object) in cases() {
        lines.push(format!("{label}\t{}", faster_hex::hex_string(&borsh::to_vec(&object).expect("an object serializes"))));
    }
    let path = corpus_path();
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the corpus directory");
    std::fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write the corpus");
    println!("wrote {} cases to {}", lines.len(), path.display());
}

/// **The verdicts, from the bytes on disk.** The objects are decoded rather than rebuilt, so this
/// is about what a peer could send and not about what this file can construct today.
#[test]
fn the_stored_corpus_gets_the_verdicts_this_build_pinned() {
    let raw = std::fs::read_to_string(corpus_path()).expect("the stored corpus (run `write_the_corpus` to create it)");
    let mut lines = Vec::new();
    for entry in raw.lines().filter(|l| !l.trim().is_empty()) {
        let (label, hex) = entry.split_once('\t').expect("label<TAB>hex");
        let mut bytes = vec![0u8; hex.len() / 2];
        faster_hex::hex_decode(hex.as_bytes(), &mut bytes).expect("the corpus is hex");
        let object: PalwConsensusObjectV2 = borsh::from_slice(&bytes).expect("the corpus decodes");
        lines.push(verdict_line(label, &object));
    }
    assert!(lines.len() >= 10, "a corpus of {} cases is not a corpus", lines.len());
    let digest = digest_of(&lines);
    println!("corpus verdict digest {digest} ({} cases)", lines.len());
    assert_eq!(
        digest,
        CORPUS_VERDICT_DIGEST,
        "a stored object's verdict moved — this build admits or refuses something the pinned build did not:\n{}\n\n\
         If that is intended: raise the revision of the ruleset that changed in PALW_CONSENSUS_RULE_MANIFEST_V1, so the \
         fingerprint moves and no operator mistakes this build for the one before it, and re-pin this digest in the same commit.",
        lines.join("\n")
    );
}

/// **The corpus and the manifest are one commit.** A verdict digest pinned without a manifest entry
/// above R1 would mean the tree believes no rule has ever changed; this is the sentence that keeps
/// the two files honest about each other.
#[test]
fn the_pinned_verdicts_belong_to_a_named_manifest() {
    let changed: Vec<&str> = kaspa_consensus_core::palw_rule_manifest_v1::PALW_CONSENSUS_RULE_MANIFEST_V1
        .iter()
        .filter(|r| r.revision > 1)
        .map(|r| r.name)
        .collect();
    assert_eq!(changed, vec!["palw_readiness"], "the rulesets this tree says have changed since they were written");
}
