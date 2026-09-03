//! **`palw-certify` — the ADR-0075 certification objects, from this build's own drills.**
//!
//! Certification used to be a compile-time set: adding a model's court certificate meant a code
//! change, a fingerprint move and a re-genesis. ADR-0075 makes it two lifecycle objects any
//! transaction can carry, graded by the court in the transition:
//!
//! * `drill --family <base0|qwen36|a16> --lane <attempt|fp> --out <file>` runs the family's
//!   fixture drill and writes a `FamilyCertified` object (borsh `PalwConsensusObjectV2`). The
//!   chain grades the same vectors with the same court and records the family.
//! * `bind (--artifact <file> [--n-ctx <n>] | --n-ctx <n> | --model-id <catalog id>) --lane
//!   <attempt|fp> --out <file>` writes a `ClassLaneCertified` object for ONE class: its own
//!   profile, which the chain checks hashes to the class id and whose kernels must lie in a family
//!   already certified on chain for that lane.
//! * `inspect --object <file>` prints what an object file carries.
//!
//! Submit either with `misaka-cli palw submit-object --object <file> --yes` (funded from the
//! CLI key; the fee is the rent, nothing signs).
//!
//! The drills here are the FIXTURE drills the RC certificates are pinned from — a family
//! certificate is about kernels, not weights, so the two-layer A16 store and the 4/8 QWEN36
//! fixture certify the kernels the 1.5B and 35B classes reach (ADR-0069 Decision 2).
//!
//! # Why `bind` can name a WIDTH, and why that is not a catalog row
//!
//! `--model-id` was the only way to name a class, and **a model id does not identify one**.
//! `n_ctx` is a field of `PalwShapeProfileV3` and `shape_profile_id` is the hash of that struct's
//! borsh, so the width is inside the identity; the A16 catalog is a fixed three-row table (16, 18,
//! 16), so `--model-id` could only ever produce those three classes. An acceptance drill on a live
//! devnet found the consequence at devnet scale, hours before a genesis fence would have armed:
//! the panel registered the dense tier at the artifact's own width and `bind --model-id
//! Qwen/Qwen2.5-1.5B/graph-v2` wrote a certificate for a different class. Under ADR-0075 a class
//! with no `ClassLaneCertified` ships with its free-prompt lane CLOSED, so the first free-prompt
//! request refuses — and refuses in a way that reads like a context-width wall rather than a
//! missing certificate.
//!
//! **The repair is to name the class, not to add a row**, and the reason is falsifiability. A
//! fourth catalog row at 512 would make the drill green and could not be falsified: the width
//! would be a constant again, so a WRONG width would bind to the wrong class in silence. Naming
//! the width — or better, naming the FILE that states it — makes a wrong width fail to bind, and
//! that difference is the whole design. The table has already failed in exactly this direction:
//! its own comment marks `n_ctx` 17 **BURNED** by a 2026-08-28 mispairing that put a class on
//! chain against the genesis constant, past a green suite.
//!
//! So `--artifact` is the primary form. It decodes the file the way the panel's dense lineage
//! does (`decode_artifact_file_v1`, which recomputes the declared digest over every byte),
//! identifies the family with the panel's own pairing check, takes the width from the header's
//! `max_position`, and projects the graph with the shipped ladder function — one derivation,
//! borrowed whole. `--n-ctx` alone is the lighter form for a machine where the 1.7 GiB file is
//! not; it reaches the same projection with the artifact's two contributions (family confirmation
//! and a bound on the width) removed, which is precisely why it is the lighter one.
//!
//! Deeper still: the defect here is one class root spelled twice — once derived from the
//! artifact's own inventory, once from a constant — with nothing forcing the two equal. That is
//! the A16 genesis root defect again. `--artifact` removes the second spelling for this path
//! rather than adding a third.

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
        "usage:\n  \
         palw-certify drill (--family <base0|qwen36|a16> | --model-id <catalog model id>) --lane <attempt|fp> --out <file>\n  \
         palw-certify bind --artifact <.palwart> [--n-ctx <n>] --lane <attempt|fp> --out <file>\n  \
         palw-certify bind --n-ctx <n>            --lane <attempt|fp> --out <file>\n  \
         palw-certify bind --model-id <catalog model id> --lane <attempt|fp> --out <file>\n  \
         palw-certify inspect --object <file>\n\
         \n\
         bind names ONE class, three ways:\n  \
         --artifact  the dense A16 row the FILE can serve: its header states the width (`max_position`,\n              \
         the rotary span) and its geometry is paired against the catalog's A16 rows. --n-ctx may\n              \
         narrow the row below that span; it may never widen it. This is the form to use.\n  \
         --n-ctx     the same dense A16 ladder row at a width taken on your word — nothing confirms the\n              \
         family and nothing bounds the width. For a machine where the artifact is elsewhere.\n  \
         --model-id  a row of this build's catalogs, at the width that row is defined at. A model id\n              \
         does NOT determine a width, so this cannot reach a class the catalogs do not table."
    );
    std::process::exit(2)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

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
            let lane = lane_of(flag(&args, "--lane").unwrap_or_else(|| usage()));
            let out = flag(&args, "--out").unwrap_or_else(|| usage());
            let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .unwrap_or_else(|e| die(format!("the shipped court params: {e:?}")));
            let n_ctx = flag(&args, "--n-ctx").map(|s| {
                s.trim().parse::<u32>().unwrap_or_else(|e| die(format!("--n-ctx {s}: {e} — a width is a decimal number of positions")))
            });
            // Exactly one source names the class. Two would be two widths for one graph, and the
            // whole point of this subcommand is that a width is part of the identity.
            let (profile, named_by, checked) = match (flag(&args, "--artifact"), flag(&args, "--model-id"), n_ctx) {
                (Some(_), Some(_), _) => die(
                    "--artifact and --model-id name the class two ways. Use --artifact (the file states its own width) \
                     or --model-id (a catalog row's width is part of the row)",
                ),
                (None, Some(model_id), Some(n)) => die(format!(
                    "--model-id {model_id} --n-ctx {n} names two widths for one class: a catalog row's n_ctx is part of the \
                     row, so a width beside it would be a third spelling of the graph. Use --artifact <file> (checked \
                     against the weights), or --n-ctx {n} alone for the dense A16 ladder row"
                )),
                (Some(path), None, asked) => {
                    // The panel's own load: the whole file is decoded and its declared digest
                    // recomputed over every byte, so a truncated or rewritten artifact is refused
                    // here rather than certified.
                    let bytes = std::fs::read(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
                    let read_bytes = bytes.len();
                    let artifact = misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes)
                        .unwrap_or_else(|e| die(format!("{path} is not a readable dense PALW artifact: {e}")));
                    drop(bytes);
                    let row = misaka_palw_base0::classes::a16_artifact_row_v1(&court, &artifact, asked).unwrap_or_else(|e| {
                        die(format!(
                            "{path}: {e}\n  \
                             no class id was computed and nothing was written — a certificate whose class the weights \
                             cannot execute is worse than none"
                        ))
                    });
                    let width_source = if row.narrowed {
                        format!("--n-ctx {}, NARROWED from the artifact's {}-position rotary span", row.n_ctx, row.artifact_span)
                    } else {
                        format!("the artifact's own header (`max_position` {}), the widest row these weights serve", row.n_ctx)
                    };
                    (
                        row.profile,
                        format!("{path}, n_ctx {}", row.n_ctx),
                        vec![
                            format!("container: {read_bytes} bytes decoded, declared digest recomputed over every byte"),
                            format!(
                                "family:    header pairs with this build's A16 dense rows [{}] (CanonicalClassV1::shape_matches, \
                                 the panel's own check)",
                                row.family_rows.join(", ")
                            ),
                            format!("width:     {width_source}"),
                            "graph:     palw_a16_context_row_profile_v1 — the shipped ladder projection, under the epsilon the \
                             artifact executes"
                                .to_string(),
                        ],
                    )
                }
                (None, None, Some(n)) => {
                    let profile = misaka_palw_base0::classes::a16_ladder_row_v1(n)
                        .unwrap_or_else(|e| die(format!("--n-ctx {n}: {e}\n  no class id was computed and nothing was written")));
                    (
                        profile,
                        format!("--n-ctx {n} (dense A16 ladder row)"),
                        vec![
                            format!("width:     {n}, taken on your word — NOTHING here checked it against an artifact"),
                            "family:    unchecked — pass --artifact <file> to have the weights confirm it".to_string(),
                            "graph:     palw_a16_context_row_profile_v1 — the shipped ladder projection, under the epsilon the \
                             artifact executes"
                                .to_string(),
                        ],
                    )
                }
                (None, Some(model_id), None) => {
                    let profile = catalog_profile_by_model_id_v1(&court, model_id)
                        .unwrap_or_else(|| die(format!("this build's catalogs have no `{model_id}` row")));
                    let n = profile.n_ctx;
                    (
                        profile,
                        model_id.to_string(),
                        vec![format!("width:     n_ctx {n}, the width the `{model_id}` catalog row is DEFINED at, not a width you named")],
                    )
                }
                (None, None, None) => usage(),
            };

            // Everything below is the chain's own acceptance of a `ClassLaneCertified`
            // (`palw_state_v2::apply_object`), run here so an object the chain would refuse never
            // leaves this machine — the same seal `drill` puts on its evidence.
            profile
                .validate_shape()
                .unwrap_or_else(|e| die(format!("{named_by}: the derived profile is not a valid graph: {e:?}")));
            let class_id = profile.shape_profile_id();
            let reachable = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&profile);
            let covering = covering_rc_family_v1(&profile, lane).unwrap_or_else(|| {
                die(format!(
                    "{named_by}\n  \
                     class {class_id}\n  \
                     {} reachable kernels, and NO family this build can drill covers them on the {lane} lane.\n  \
                     The chain would refuse this object with NoCertifiedFamilyCovers — a certificate is about kernels the \
                     court implements (ADR-0069 Decision 2), so this needs a build whose court serves the graph, not a \
                     different flag.",
                    reachable.len()
                ))
            });
            // The falsifiable half of the verdict: whether the id this derivation produced is one
            // of the fixed rows, or one only a named width can reach. A `--model-id` bind must
            // always land on a row; an `--artifact` bind at a genesis ladder width must NOT, and
            // saying so is what stops "it printed a class id" from reading as "it printed the
            // right class id".
            let tabled = misaka_palw_base0::e2e_drill::catalog_profiles_v1(&court)
                .into_iter()
                .find(|(_, p)| p.shape_profile_id() == class_id)
                .map(|(id, _)| id);
            let bytes =
                write_object(out, &PalwConsensusObjectV2::ClassLaneCertified { class_id, lane, profile: Box::new(profile.clone()) });
            println!("wrote {out}: ClassLaneCertified, {lane} lane, class {class_id}");
            println!("  named by:  {named_by}");
            for line in &checked {
                println!("  {line}");
            }
            println!(
                "  kernels:   {} reachable, covered by the {} RC family's {lane}-lane drill",
                reachable.len(),
                covering.name()
            );
            match &tabled {
                Some(id) => println!("  catalog:   this id IS the `{id}` row of this build's catalogs"),
                None => println!(
                    "  catalog:   NO row of this build's catalogs carries this id — it is reachable only by naming the width, \
                     which is the point"
                ),
            }
            // A row whose worst case does not fit the shipped 2^22 step ladder is expressible
            // only on a chain whose court was armed for it (ADR-0077 Decision 12 / the context
            // ladder). Printed rather than refused: which ladder a chain runs is that chain's
            // genesis decision, and this tool has no chain.
            let deep = kaspa_consensus_core::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES;
            let shipped = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;
            match kaspa_consensus_core::palw_step::worst_case_step_leaf_count_capped_v1(&profile, shipped) {
                Ok(n) => println!("  ladder:    worst case {n} step leaves — inside the shipped {shipped} cap"),
                Err(_) => match kaspa_consensus_core::palw_step::worst_case_step_leaf_count_capped_v1(&profile, deep) {
                    Ok(n) => println!(
                        "  ladder:    worst case {n} step leaves — OVER the shipped {shipped} cap, inside the context ladder's \
                         {deep}. The chain must have the deeper ladder armed or it cannot price this class."
                    ),
                    Err(e) => println!("  ladder:    worst case exceeds even the context ladder's {deep} cap: {e:?}"),
                },
            }
            println!(
                "  submit:    the chain must already carry {class_id} as an Active class — this object binds a LANE of a \
                 class, it cannot create one"
            );
            println!("  bytes:     {bytes}");
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
