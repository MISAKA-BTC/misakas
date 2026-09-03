//! **`palw-certify` — the ADR-0075 certification objects, from this build's own drills.**
//!
//! Certification used to be a compile-time set: adding a model's court certificate meant a code
//! change, a fingerprint move and a re-genesis. ADR-0075 makes it two lifecycle objects any
//! transaction can carry, graded by the court in the transition:
//!
//! * `drill --family <base0|qwen36|a16> --lane <attempt|fp> --out <file>` runs the family's
//!   fixture drill and writes a `FamilyCertified` object (borsh `PalwConsensusObjectV2`). The
//!   chain grades the same vectors with the same court and records the family.
//! * `bind --model-id <catalog id> --lane <attempt|fp> --out <file>` writes a `ClassLaneCertified`
//!   object for a catalog class: its own profile, which the chain checks hashes to the class id
//!   and whose kernels must lie in a family already certified on chain for that lane.
//! * `inspect --object <file>` prints what an object file carries.
//!
//! Submit either with `misaka-cli palw submit-object --object <file> --yes` (funded from the
//! CLI key; the fee is the rent, nothing signs).
//!
//! The drills here are the FIXTURE drills the RC certificates are pinned from — a family
//! certificate is about kernels, not weights, so the two-layer A16 store and the 4/8 QWEN36
//! fixture certify the kernels the 1.5B and 35B classes reach (ADR-0069 Decision 2).

use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_state_v2::{
    PALW_OBJECT_CHUNK_MAX_BYTES, PalwCertificationEvidenceV1, PalwCertifiedLaneV1, PalwConsensusObjectV2, palw_object_chunks_v1,
};
use misaka_palw_base0::e2e_drill::{
    PalwRcFamilyV1, catalog_profile_by_model_id_v1, covering_rc_family_v1, rc_attempt_evidence_v1, rc_free_prompt_evidence_v1,
};

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("palw-certify: {msg}");
    std::process::exit(2)
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  palw-certify drill (--family <base0|qwen36|a16> | --model-id <catalog model id>) --lane <attempt|fp> --out <file>\n  palw-certify bind --model-id <catalog model id> --lane <attempt|fp> --out <file> [--seat-ms-per-position <ms>]\n  palw-certify inspect --object <file>\n\n  --seat-ms-per-position is the SLOWEST fleet host's measured seat recompute, in milliseconds per\n  position (ADR-0082 Decision 9). Given, it is taken against the class row's own figure and the\n  slower of the two bounds the width this build will certify."
    );
    std::process::exit(2)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// **ADR-0082 Decision 9: the seat's window is what bounds a row's width, and it is checked HERE.**
///
/// A row nobody can seat certifies nothing (ADR-0075), so the bound belongs where seats are
/// measured — not in `verify_class_admission`, which cannot read a fleet measurement and must not
/// pretend to. `n_max = window_receipt × rate_seat_prefill`, with:
///
/// * `window_receipt` the ruleset's own receipt window (`PalwLatticeWindowsV1`), read, never typed;
/// * `rate_seat_prefill` the SLOWER of two measurements — the class row's SA-4 figure
///   (`PALW_COURT_ROW_COSTS`, whose source is written down beside it) and whatever this fleet
///   measured, passed in with `--seat-ms-per-position`. Taking the slower is what "the slowest
///   fleet host" means, and it can only ever admit FEWER positions.
///
/// Nothing is chosen: the two quantities are the ruleset's and the measurement's, and the
/// derivation is printed so a reader can check the arithmetic rather than the intention.
fn seat_width_bound(profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3, measured_ms: Option<u64>) -> (u64, u64, u64) {
    use kaspa_consensus_core::palw_context_ladder::{PALW_COURT_COST_A16, PALW_COURT_COST_BASE0, PALW_COURT_COST_QWEN36};
    let class_id = profile.shape_profile_id();
    // The class's own row where the build ships one; otherwise the shape decides which family's
    // measurement covers it, the way `palw_shipped_court_rows_v1` pairs them.
    let row_ms = kaspa_consensus_core::palw_court_deadline::palw_shipped_court_rows_v1()
        .ok()
        .and_then(|rows| rows.into_iter().find(|r| r.class_id == class_id).map(|r| r.cost.replay_ms_per_position()))
        .unwrap_or_else(|| {
            if profile.full_attention_interval == 0 || profile.gdn_heads > 0 {
                PALW_COURT_COST_QWEN36.replay_ms_per_position()
            } else if profile.vocab_size > PALW_BASE0_VOCAB_CEILING {
                PALW_COURT_COST_A16.replay_ms_per_position()
            } else {
                PALW_COURT_COST_BASE0.replay_ms_per_position()
            }
        });
    let ms = row_ms.max(measured_ms.unwrap_or(0));
    let rate = misaka_palw_base0::fp_recompute::base0_fp_seat_milli_positions_per_daa_v1(ms);
    let window = kaspa_consensus_core::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1.window_receipt;
    (misaka_palw_base0::fp_recompute::base0_fp_seat_width_bound_v1(window, rate), ms, rate)
}

/// The floor's vocabulary — above it a class is not the integer floor, which is the only thing
/// this needs to tell apart when a build ships no row for the class.
const PALW_BASE0_VOCAB_CEILING: u32 = 1_024;

fn lane_of(s: &str) -> PalwCertifiedLaneV1 {
    match s.trim().to_ascii_lowercase().as_str() {
        "attempt" => PalwCertifiedLaneV1::Attempt,
        "fp" | "free-prompt" | "freeprompt" => PalwCertifiedLaneV1::FreePrompt,
        other => die(format!("unknown lane `{other}` (attempt | fp)")),
    }
}

fn write_object(path: &str, object: &PalwConsensusObjectV2) -> usize {
    let bytes = borsh::to_vec(object).unwrap_or_else(|e| die(format!("the object does not serialize: {e}")));
    std::fs::write(path, &bytes).unwrap_or_else(|e| die(format!("{path}: {e}")));
    bytes.len()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("drill") => {
            let lane = lane_of(flag(&args, "--lane").unwrap_or_else(|| usage()));
            let family = match (flag(&args, "--family"), flag(&args, "--model-id")) {
                (Some(f), _) => PalwRcFamilyV1::parse(f).unwrap_or_else(|| die(format!("unknown family `{f}`"))),
                (None, Some(model_id)) => {
                    // The mainnet route for a model this build never pinned: whichever RC
                    // family's drill covers the kernels the model's registered graph reaches.
                    let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                        .unwrap_or_else(|e| die(format!("the shipped court params: {e:?}")));
                    let profile = catalog_profile_by_model_id_v1(&court, model_id)
                        .unwrap_or_else(|| die(format!("this build's catalogs have no `{model_id}` row")));
                    let family = covering_rc_family_v1(&profile, lane).unwrap_or_else(|| {
                        die(format!(
                            "no family this build can drill covers every kernel `{model_id}` reaches on the {lane} lane — \
                             a new architecture needs a build whose court serves it (ADR-0069 Decision 2)"
                        ))
                    });
                    eprintln!("palw-certify: {model_id} reaches the kernels of {} — drilling that family", family.name());
                    family
                }
                (None, None) => usage(),
            };
            let out = flag(&args, "--out").unwrap_or_else(|| usage());
            let evidence = match lane {
                PalwCertifiedLaneV1::Attempt => PalwCertificationEvidenceV1::Attempt(
                    rc_attempt_evidence_v1(family).unwrap_or_else(|e| die(format!("{} attempt-lane drill: {e}", family.name()))),
                ),
                PalwCertifiedLaneV1::FreePrompt => PalwCertificationEvidenceV1::FreePrompt(
                    rc_free_prompt_evidence_v1(family)
                        .unwrap_or_else(|e| die(format!("{} free-prompt-lane drill: {e}", family.name()))),
                ),
            };
            // Graded here first, so a drill the chain would refuse never leaves this machine.
            let graded = evidence.grade().unwrap_or_else(|e| die(format!("this build's court refuses its own drill: {e}")));
            let vectors = evidence.vector_count();
            let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(evidence) };
            let bytes = write_object(out, &object);
            println!(
                "wrote {out}: FamilyCertified, {lane} lane, family {} ({}), digest {}, {vectors} fault vectors, {} kernels, {bytes} bytes",
                family.name(),
                graded.family_id,
                graded.digest(),
                graded.kernel_ids.len()
            );
            // ADR-0075 Decision 14: a drill above one carrier's bytes rides in chunks — written
            // beside the whole object, submitted in index order.
            match palw_object_chunks_v1(&object) {
                Ok(None) => println!("fits one carrier: submit {out} as it is"),
                Ok(Some(chunks)) => {
                    let mut names = Vec::with_capacity(chunks.len());
                    for chunk in &chunks {
                        let PalwConsensusObjectV2::ObjectChunk { index, count, group, .. } = chunk else { unreachable!() };
                        let name = format!("{out}.chunk{index}");
                        let n = write_object(&name, chunk);
                        println!("wrote {name}: ObjectChunk {index}/{count} of group {group}, {n} bytes");
                        names.push(name);
                    }
                    println!(
                        "too large for one carrier ({bytes} > {PALW_OBJECT_CHUNK_MAX_BYTES}): submit the chunks in order — \
                         misaka-cli palw submit-object {} --yes",
                        names.iter().map(|n| format!("--object {n}")).collect::<Vec<_>>().join(" ")
                    );
                }
                Err(e) => die(format!("the drill cannot be chunked: {e}")),
            }
        }
        Some("bind") => {
            let model_id = flag(&args, "--model-id").unwrap_or_else(|| usage());
            let lane = lane_of(flag(&args, "--lane").unwrap_or_else(|| usage()));
            let out = flag(&args, "--out").unwrap_or_else(|| usage());
            let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .unwrap_or_else(|e| die(format!("the shipped court params: {e:?}")));
            let profile = catalog_profile_by_model_id_v1(&court, model_id)
                .unwrap_or_else(|| die(format!("this build's catalogs have no `{model_id}` row")));
            let class_id = profile.shape_profile_id();
            let kernels = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&profile).len();
            // **ADR-0082 Decision 9, before anything is written.** A certificate is a statement
            // that seats can judge this row; a width no seat can recompute inside `window_receipt`
            // is a row whose seats file `Incapable`, whose claims reach no quorum, and whose
            // certificate would be a promise the fleet cannot keep.
            let measured = flag(&args, "--seat-ms-per-position").map(|v| {
                v.parse::<u64>().unwrap_or_else(|_| die(format!("--seat-ms-per-position takes a whole number of milliseconds, not `{v}`")))
            });
            let (n_max, ms, rate) = seat_width_bound(&profile, measured);
            let window = kaspa_consensus_core::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1.window_receipt;
            println!(
                "seat window (ADR-0082 Decision 9): {ms} ms per position → {} positions/DAA × window_receipt {window} DAA = n_max {n_max}; this class registers n_ctx {}{}",
                rate as f64 / 1_000.0,
                profile.n_ctx,
                match measured {
                    Some(m) => format!(" (measured {m} ms/position on this host)"),
                    None => String::new(),
                }
            );
            if profile.n_ctx as u64 > n_max {
                die(format!(
                    "this class registers n_ctx {} and the slowest measured seat recomputes {n_max} positions inside \
                     window_receipt ({window} DAA at {ms} ms per position) — certifying it would certify a row nobody can seat \
                     (ADR-0082 Decision 9, ADR-0075). Re-measure with --seat-ms-per-position from the slowest fleet host, or \
                     register a narrower row.",
                    profile.n_ctx
                ));
            }
            let bytes = write_object(out, &PalwConsensusObjectV2::ClassLaneCertified { class_id, lane, profile: Box::new(profile) });
            println!(
                "wrote {out}: ClassLaneCertified, {lane} lane, class {class_id} ({model_id}), {kernels} reachable kernels, {bytes} bytes"
            );
        }
        Some("inspect") => {
            let path = flag(&args, "--object").unwrap_or_else(|| usage());
            let bytes = std::fs::read(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
            let object: PalwConsensusObjectV2 =
                borsh::from_slice(&bytes).unwrap_or_else(|e| die(format!("{path} is not a borsh consensus object: {e}")));
            match &object {
                PalwConsensusObjectV2::FamilyCertified { evidence } => {
                    let verdict = match evidence.grade() {
                        Ok(family) => format!(
                            "grades: family {} digest {} ({} kernels)",
                            family.family_id,
                            family.digest(),
                            family.kernel_ids.len()
                        ),
                        Err(e) => format!("DOES NOT GRADE: {e}"),
                    };
                    println!(
                        "FamilyCertified: {} lane, {} fault vectors, {} bytes — {verdict}",
                        evidence.lane(),
                        evidence.vector_count(),
                        bytes.len()
                    );
                }
                PalwConsensusObjectV2::ClassLaneCertified { class_id, lane, profile } => {
                    let derived = profile.shape_profile_id();
                    println!(
                        "ClassLaneCertified: {lane} lane, class {class_id}, profile hashes to {derived} ({}), {} reachable kernels",
                        if derived == *class_id { "matches" } else { "MISMATCH" },
                        kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(profile).len()
                    );
                }
                PalwConsensusObjectV2::ObjectChunk { group, index, count, bytes: part } => {
                    println!("ObjectChunk: part {index} of {count} of group {group}, {} bytes", part.len());
                }
                PalwConsensusObjectV2::DerivedArtifactV1 { object, signature } => {
                    use kaspa_consensus_core::palw_derived_v1::{derived_id_v1, kind};
                    println!(
                        "DerivedArtifactV1: claim {}, kind {} ({}), grammar {}, transformer {}, dsl {}, artifact {} ({} bytes), derived id {}, signature {} bytes",
                        object.claim_id,
                        object.kind,
                        kind::name(object.kind).unwrap_or("unassigned"),
                        object.grammar_id,
                        object.transformer_id,
                        object.dsl_hash,
                        object.artifact_hash,
                        object.artifact_bytes,
                        derived_id_v1(object),
                        signature.len()
                    );
                }
                other => println!("{other:?}"),
            }
        }
        _ => usage(),
    }
}
