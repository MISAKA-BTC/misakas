//! **`derived-transformer`** (ADR-0108 Decision 5's "nothing rides"; ADR-0078 Decisions 3 and 5):
//! a transformer is named by its manifest, its id is `H(manifest bytes)`, and a consumer recomputes
//! — this build's code over the declared DSL vectors, compared hash for hash. A transformer this
//! build does not ship is a node extension: the chain admits its id, and a machine without the
//! code says *unverifiable here*.

use kaspa_hashes::Hash64;
use misaka_palw_derive::ids::{artifact_hash_v1, dsl_hash_v1, grammar_id_v1, transformer_id};
use misaka_palw_derive::registry::{
    PRIOR_SOURCE_TREES_SHA256_HEX, TransformerIdTree, grammar_by_name, transformer_by_id_with_tree, transformer_by_name,
};
use misaka_palw_derive::{Discipline, TransformerManifest, check_declared_bounds};

use crate::manifest::{PalwExtensionError, parse_hash64};
use crate::report::{PalwExtensionClassificationV1, PalwExtensionDepthV1};
use crate::verify::{KindOutcomeV1, VerifyCx};

/// `TransformerManifest` holds `&'static str` fields — it is the preimage of an id this build
/// spells as constants. A manifest handed in as a document is leaked into that shape: a few short
/// strings per verification, in a process that verifies a handful of manifests. Restating the
/// preimage over `String`s would be a second spelling of `transformer_manifest_bytes`.
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

fn tree_label(tree: &str) -> &'static str {
    if tree == misaka_palw_derive::SOURCE_TREE_SHA256_HEX {
        "current"
    } else if PRIOR_SOURCE_TREES_SHA256_HEX.contains(&tree) {
        "prior"
    } else {
        "unknown"
    }
}

fn expressible() -> PalwExtensionClassificationV1 {
    PalwExtensionClassificationV1::Expressible {
        admission_object: "none".to_string(),
        would_be_refused: None,
        serving: None,
        weightless: false,
        already_registered: false,
    }
}

pub(crate) fn verify(cx: &mut VerifyCx<'_>) -> Result<KindOutcomeV1, PalwExtensionError> {
    use PalwExtensionDepthV1::{Full, Structural, Vectors};
    let manifest = cx.manifest();
    let Some(spec) = manifest.transformer.clone() else {
        return Ok(KindOutcomeV1::at(cx.refuse("transformer", "a derived-transformer manifest names its transformer"), Structural));
    };
    let declared = parse_hash64("declares.object_id", &manifest.declares.object_id)?;

    // ---- Structural: the id, from the preimage the manifest carries or names -----------------
    let (id, name, tree) = if spec.form()? {
        let discipline = match spec.discipline.as_deref() {
            Some("integer") => Discipline::Integer,
            Some("exact-rational") => Discipline::ExactRational,
            other => {
                return Ok(KindOutcomeV1::at(
                    cx.refuse("transformer.discipline", format!("`{}` is not `integer` or `exact-rational`", other.unwrap_or(""))),
                    Structural,
                ));
            }
        };
        let tm = TransformerManifest {
            name: leak(&spec.name),
            kind: spec.kind.expect("full form"),
            grammar: leak(spec.grammar.as_deref().expect("full form")),
            discipline,
            writer: leak(spec.writer.as_deref().expect("full form")),
            source_tree_sha256: leak(spec.source_tree_sha256.as_deref().expect("full form")),
            max_dsl_bytes: spec.max_dsl_bytes.expect("full form"),
            max_artifact_bytes: spec.max_artifact_bytes.expect("full form"),
            max_steps: spec.max_steps.expect("full form"),
        };
        // ADR-0078 SA-2, re-pinned here: a zero ceiling is refused by the field's name.
        for (field, value) in
            [("max_dsl_bytes", tm.max_dsl_bytes), ("max_artifact_bytes", tm.max_artifact_bytes), ("max_steps", tm.max_steps)]
        {
            if value == 0 {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(format!("transformer.{field}"), "a zero ceiling is no bound (ADR-0078 SA-2)"),
                    Structural,
                ));
            }
        }
        if let Err(e) = check_declared_bounds(&tm) {
            return Ok(KindOutcomeV1::at(cx.refuse("transformer", e.to_string()), Structural));
        }
        cx.pass("transformer.bounds");
        let id = transformer_id(&tm);
        (id, tm.name, tm.source_tree_sha256)
    } else {
        // The name form: the manifest this build publishes under the name, under this tree or a
        // listed earlier one — whichever hashes to what the person declared.
        let Some(transformer) = transformer_by_name(&spec.name) else {
            cx.fail("transformer.name", format!("`{}` is not a transformer this build ships", spec.name));
            return Ok(KindOutcomeV1::at(
                PalwExtensionClassificationV1::node_extension(format!(
                    "transformer `{}` — not in this build, so its manifest cannot be looked up by name; give every preimage field, or verify on a build that publishes it (ADR-0078 SA-5)",
                    spec.name
                )),
                Structural,
            ));
        };
        let mut tm = transformer.manifest();
        let mut candidates = vec![(transformer_id(&tm), tm.source_tree_sha256)];
        for &prior in PRIOR_SOURCE_TREES_SHA256_HEX {
            tm.source_tree_sha256 = prior;
            candidates.push((transformer_id(&tm), prior));
        }
        match candidates.iter().find(|(id, _)| *id == declared) {
            Some((id, tree)) => (*id, tm.name, *tree),
            None => {
                let spelled: Vec<String> = candidates.iter().map(|(id, tree)| format!("{id} ({})", tree_label(tree))).collect();
                return Ok(KindOutcomeV1::at(
                    cx.refuse(
                        "declares.object_id",
                        format!(
                            "the manifest published under `{}` hashes to {} — none is the declared {declared}",
                            spec.name,
                            spelled.join(", ")
                        ),
                    ),
                    Structural,
                ));
            }
        }
    };
    cx.record("transformer_id", id);
    cx.record("transformer", name);
    cx.record("source_tree", format!("{tree} ({})", tree_label(tree)));
    if id != declared {
        return Ok(KindOutcomeV1::at(
            cx.refuse(
                "declares.object_id",
                format!("the manifest hashes to {id}, not to the declared {declared} — a transformer id is derived, never declared"),
            ),
            Structural,
        ));
    }
    cx.pass("declares.object_id");
    if let Some(expected) = manifest.verification.expected.get("transformer_id") {
        let expected = parse_hash64("verification.expected.transformer_id", expected)?;
        if expected != id {
            return Ok(KindOutcomeV1::at(cx.refuse("verification.expected.transformer_id", format!("recomputed {id}")), Structural));
        }
        cx.pass("verification.expected.transformer_id");
    }
    if cx.depth == Structural {
        return Ok(KindOutcomeV1::at(expressible(), Structural));
    }

    // ---- Vectors: this build's code over the declared DSL ------------------------------------
    let Some((transformer, resolved_tree)) = transformer_by_id_with_tree(&id) else {
        cx.fail("transformer.resolve", format!("id {id} resolves to nothing in this build"));
        return Ok(KindOutcomeV1::at(
            PalwExtensionClassificationV1::node_extension(format!(
                "transformer `{name}` (id {id}) is not in this build; publish its manifest (ADR-0078 SA-5) — a build that ships it can re-run the vectors"
            )),
            Structural,
        ));
    };
    cx.record(
        "resolved_tree",
        match resolved_tree {
            TransformerIdTree::Current => "current".to_string(),
            TransformerIdTree::Prior(tree) => format!("prior {tree}"),
        },
    );
    cx.pass("transformer.resolve");
    let tm = transformer.manifest();
    let Some(grammar) = grammar_by_name(tm.grammar) else {
        return Ok(KindOutcomeV1::at(
            PalwExtensionClassificationV1::node_extension(format!("grammar `{}` for transformer `{name}`", tm.grammar)),
            Structural,
        ));
    };
    let grammar_id = grammar_id_v1(grammar.name());
    cx.record("grammar", grammar.name());
    cx.record("grammar_id", grammar_id);
    cx.record("kind_id", tm.kind);
    let vectors = manifest.verification.vectors.clone();
    if vectors.is_empty() {
        cx.skip("verification.vectors", "none declared — the transformer resolved and nothing was re-run");
    }
    for (i, vector) in vectors.iter().enumerate() {
        let field = format!("verification.vectors[{i}]");
        let Some((_, dsl)) = cx.read_named_file(&format!("{field}.dsl_path"), &vector.dsl_path, tm.max_dsl_bytes.max(1))? else {
            cx.skip(&field, format!("{}: not readable on this machine", vector.dsl_path));
            return Ok(KindOutcomeV1::stopped(expressible(), Structural, format!("{field}.dsl_path not readable")));
        };
        let canonical = match grammar.canonicalize(&dsl) {
            Ok(canonical) => canonical,
            Err(e) => {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(format!("{field}.dsl_path"), format!("the grammar refuses it: {e}")),
                    Structural,
                ));
            }
        };
        let dsl_hash = dsl_hash_v1(&grammar_id, &canonical);
        let expected_dsl = parse_hash64(&format!("{field}.expected_dsl_hash"), &vector.expected_dsl_hash)?;
        if dsl_hash != expected_dsl {
            return Ok(KindOutcomeV1::at(
                cx.refuse(format!("{field}.expected_dsl_hash"), format!("recomputed {dsl_hash}")),
                Structural,
            ));
        }
        if let Some(work) = transformer.declared_work(&canonical)
            && work > tm.max_steps
        {
            return Ok(KindOutcomeV1::at(
                cx.refuse(
                    format!("{field}.dsl_path"),
                    format!("asks for {work} {}, past max_steps {} (ADR-0078 SA-2)", tm.step_unit(), tm.max_steps),
                ),
                Structural,
            ));
        }
        let artifact = match transformer.run(&canonical) {
            Ok(artifact) => artifact,
            Err(e) => {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(format!("{field}.dsl_path"), format!("the transformer refuses it: {e}")),
                    Structural,
                ));
            }
        };
        let artifact_hash = artifact_hash_v1(&artifact.bytes);
        let expected_artifact = parse_hash64(&format!("{field}.expected_artifact_hash"), &vector.expected_artifact_hash)?;
        if artifact_hash != expected_artifact {
            return Ok(KindOutcomeV1::at(
                cx.refuse(format!("{field}.expected_artifact_hash"), format!("recomputed {artifact_hash}")),
                Structural,
            ));
        }
        if artifact.bytes.len() as u64 != vector.expected_artifact_bytes {
            return Ok(KindOutcomeV1::at(
                cx.refuse(format!("{field}.expected_artifact_bytes"), format!("recomputed {}", artifact.bytes.len())),
                Structural,
            ));
        }
        cx.record(format!("vectors[{i}].artifact_hash"), artifact_hash);
        cx.pass(field);
    }
    cx.record("vectors_run", vectors.len());
    let _ = Hash64::default();
    let depth = if cx.depth == Vectors { Vectors } else { Full };
    Ok(KindOutcomeV1::at(expressible(), depth))
}
