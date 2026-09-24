//! **SEAT-0: a seat licenses only material bound to the claim** (f1c_f1m spec §4-bis.10).
//!
//! The grind. An attempt's lottery ticket hashes its `execution_root`, and the seat's material
//! check (`base0_material_tail_matches_v1`, reached from every backend's `verify_material`)
//! compared the binding's `committed_execution_root` with the claim's and never rebuilt it from the
//! binding's parts. The root was a free 512-bit field: re-roll it, announce it, and every honest
//! seat signs `Valid`. The activation leg, the checkpoint interval and the decode ids were free the
//! same way, each with a real preimage, and on dense material so were the logits rows.
//!
//! Every attack here starts from an HONEST run of the shipped producer, is shown to be licensed by
//! the seat check as it stood before SEAT-0 (`licensed_before_seat0`, the old body verbatim), and
//! is then refused by the backend verb kaspad calls — `PalwExecutionBackendV1::verify_material`,
//! through the free-prompt capture envelope exactly as `palw_panel`'s `fp_capture_view` unwraps it —
//! with the SEAT-0 rule that refused it named. The honest half is the one the live fleet depends
//! on: every honest material of every family the chain uses keeps the verdict it had.
//!
//! * T1 — an unbound execution root (no preimage at all).
//! * T2 — a moved activation leg, re-bound (a real preimage, zero execution).
//! * T3 — a moved checkpoint interval, the leg re-chained (on the floor: every checkpoint dropped).
//! * T4 — a decode id the rule does not select from its own row, the trace re-rooted.
//! * T5 — a decode id outside the vocabulary, the trace re-rooted.
//! * R1 — dense: a logits row bent above its argmax with the id following it (so T4 passes).
//! * H  — dense: honest logits beside a lying head tile (the drill's corruption at a head leaf).
//! * X  — an extra decode row appended, id and all, the trace re-rooted.

use std::sync::Arc;

use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_context_ladder::{PalwCheckpointCadenceV1, palw_checkpoint_cadence_v1};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_CANONICAL, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3,
    fp_job_id_v3, palw_fp_capture_decode_v1, palw_fp_capture_encode_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_step::{PALW_STEP_MAX_LEAVES, PalwShapeProfileV3, PalwStepCoordinateV1, canonical_step_leaf_index};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_MAX_LEAVES, PalwStepBindingV2, checkpoint_genesis_prev_v2, checkpoint_leaf_hash_v2, checkpoint_leg_root_v2,
    execution_commitment_root_v2, step_leg_root_v1, verify_binding_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    base0_decode_token_select_v1, base0_logits_trace_root_v1, tiled_logits_scheme_id_v1, tiled_logits_trace_root_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::backend::Base0Backend;
use misaka_palw_base0::legs::Base0CheckpointCaptureV1;
use misaka_palw_base0::produce::{
    Base0FpMaterialV2, Base0RetainedMaterialV1, Base0SeatFamilyV1, Base0SeatRefusalV1, PALW_BASE0_FP_MATERIAL_MAGIC_V2,
    base0_dense_step_leaves_capped_v1, base0_dense_step_root_capped_v1, base0_fp_material_decode_v2, base0_fp_material_encode_v2,
    base0_logits_head_v1, base0_material_decode_v1, base0_seat_rules_v1,
};
use misaka_palw_base0::qwen25_a16_backend::{Qwen25A16Backend, a16_execute_free_prompt_streaming_v1};
use misaka_palw_base0::qwen36_backend::Qwen36Backend;

const NETWORK: &[u8] = b"misaka-palw-rc";

// ------------------------------------------------------------------------------------------------
// The material, both retentions behind one handle
// ------------------------------------------------------------------------------------------------

#[derive(Clone)]
enum Mat {
    Fold(Base0FpMaterialV2),
    Dense(Base0RetainedMaterialV1),
}

impl Mat {
    fn decode(bytes: &[u8]) -> Self {
        if let Ok(m) = base0_fp_material_decode_v2(bytes) {
            return Mat::Fold(m);
        }
        Mat::Dense(base0_material_decode_v1(bytes).expect("a retention this family wrote decodes"))
    }
    fn encode(&self) -> Vec<u8> {
        match self {
            Mat::Fold(m) => {
                let mut out = PALW_BASE0_FP_MATERIAL_MAGIC_V2.to_vec();
                out.extend_from_slice(&borsh::to_vec(m).expect("serializes"));
                out
            }
            Mat::Dense(t) => borsh::to_vec(t).expect("serializes"),
        }
    }
    fn binding(&self) -> &PalwStepBindingV2 {
        match self {
            Mat::Fold(m) => &m.binding,
            Mat::Dense(t) => &t.0,
        }
    }
    fn binding_mut(&mut self) -> &mut PalwStepBindingV2 {
        match self {
            Mat::Fold(m) => &mut m.binding,
            Mat::Dense(t) => &mut t.0,
        }
    }
    fn rows(&self) -> &[Vec<i32>] {
        match self {
            Mat::Fold(m) => &m.logits_rows,
            Mat::Dense(t) => &t.2,
        }
    }
    fn ids(&self) -> &[u32] {
        match self {
            Mat::Fold(m) => &m.generated_token_ids,
            Mat::Dense(t) => &t.3,
        }
    }
    fn rows_ids_mut(&mut self) -> (&mut Vec<Vec<i32>>, &mut Vec<u32>) {
        match self {
            Mat::Fold(m) => (&mut m.logits_rows, &mut m.generated_token_ids),
            Mat::Dense(t) => (&mut t.2, &mut t.3),
        }
    }
    fn is_dense(&self) -> bool {
        matches!(self, Mat::Dense(_))
    }
    /// The dense retention's committed leaves (rebuilt from its tiles, as the seat does).
    fn dense_leaves(&self) -> Option<Vec<Hash64>> {
        match self {
            Mat::Dense(t) => base0_dense_step_leaves_capped_v1(&t.0, &t.1, PALW_STEP_LEG_MAX_LEAVES),
            Mat::Fold(_) => None,
        }
    }
}

/// The committed execution root, recomputed from the binding's own parts — `verify_binding`'s
/// recipe, so an attack below that edits a part re-binds to a root with a real preimage.
fn rebind(b: &mut PalwStepBindingV2) {
    let ctx_hash = b.job_context.context_hash();
    let profile_hash = b.shape_profile.shape_profile_id();
    let decode_calls = b.job_context.exact_decode_tokens.saturating_sub(1);
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, b.step_leaf_count, &b.step_merkle_root);
    let checkpoint_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &b.checkpoint_profile.profile_hash(),
        &b.state_chunk_map_id,
        decode_calls,
        b.checkpoint_count,
        &b.checkpoint_merkle_root,
    );
    b.committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &b.full_logits_trace_root, &b.activation_leg_root, &checkpoint_root, &step_root);
    assert!(verify_binding_v1(b).is_ok(), "a re-bound binding is its own root's preimage");
}

/// The trace root re-derived from the (edited) rows and ids under the class's scheme, and re-bound.
fn retrace(m: &mut Mat) {
    let b = m.binding().clone();
    let trace = if b.shape_profile.logits_scheme_id == tiled_logits_scheme_id_v1() {
        tiled_logits_trace_root_v1(&b.job_context, m.rows(), m.ids()).expect("rows with lanes build the tiled tree")
    } else {
        base0_logits_trace_root_v1(&b.job_context, m.rows(), m.ids())
    };
    m.binding_mut().full_logits_trace_root = trace;
    rebind(m.binding_mut());
}

// ------------------------------------------------------------------------------------------------
// The seat check as it stood before SEAT-0 — `base0_material_matches_claim_capped_v1` /
// `base0_fp_material_matches_claim_v2` and the old `base0_material_tail_matches_v1`, verbatim in
// logic. It is how each attack below is shown to have been LICENSED, and how every honest material
// is shown to keep its verdict.
// ------------------------------------------------------------------------------------------------

fn licensed_before_seat0(bytes: &[u8], claim: PalwClaimRootsV1) -> bool {
    let (binding, rows, ids, chunks, leaves) = match Mat::decode(bytes) {
        Mat::Fold(m) => {
            let tree_ok = m.step_tree.validate_v1().is_ok()
                && m.step_tree.leaf_count() == m.binding.step_leaf_count
                && m.step_tree.root().is_ok_and(|root| root == m.binding.step_merkle_root);
            if !tree_ok {
                return false;
            }
            (m.binding, m.logits_rows, m.generated_token_ids, m.checkpoint_chunks, m.checkpoint_leaves)
        }
        Mat::Dense((binding, tiles, rows, ids, chunks)) => {
            if base0_dense_step_root_capped_v1(&binding, &tiles, PALW_STEP_LEG_MAX_LEAVES) != Some(binding.step_merkle_root) {
                return false;
            }
            (binding, rows, ids, chunks, Vec::new())
        }
    };
    let trace = if binding.shape_profile.logits_scheme_id == tiled_logits_scheme_id_v1() {
        match tiled_logits_trace_root_v1(&binding.job_context, &rows, &ids) {
            Some(root) => root,
            None => return false,
        }
    } else {
        base0_logits_trace_root_v1(&binding.job_context, &rows, &ids)
    };
    if trace != binding.full_logits_trace_root {
        return false;
    }
    let rebuilt = match palw_checkpoint_cadence_v1(&binding.shape_profile) {
        PalwCheckpointCadenceV1::PerDecodeCall => Base0CheckpointCaptureV1::from_chunks_v1(
            &binding.job_context,
            &binding.shape_profile,
            &binding.checkpoint_profile,
            &chunks,
        ),
        PalwCheckpointCadenceV1::PerPosition => {
            if !chunks.is_empty() {
                return false;
            }
            Base0CheckpointCaptureV1::from_leaves_v1(
                &binding.job_context,
                &binding.shape_profile,
                &binding.checkpoint_profile,
                &leaves,
            )
        }
    };
    let Ok(rebuilt) = rebuilt else {
        return false;
    };
    rebuilt.merkle_root == binding.checkpoint_merkle_root
        && rebuilt.leaf_hashes.len() as u32 == binding.checkpoint_count
        && binding.committed_execution_root == claim.execution_root
        && binding.full_logits_trace_root == claim.trace_root
}

// ------------------------------------------------------------------------------------------------
// One honest claim, and the seat kaspad runs over it
// ------------------------------------------------------------------------------------------------

/// A free-prompt claim's question, as `palw_panel` holds it: the job, the prompt ids and the form
/// the class commits them under — what `fp_capture_view` needs to unwrap the served envelope.
#[derive(Clone)]
struct FpQuestion {
    job: PalwFreePromptJobV3,
    ids: Vec<u32>,
    form: PalwPromptIdsFormV1,
}

struct Claim<'a> {
    label: String,
    backend: &'a dyn PalwExecutionBackendV1,
    family: Base0SeatFamilyV1,
    /// The honest retention, as the producer serves it (the capture, not the envelope).
    material: Vec<u8>,
    roots: PalwClaimRootsV1,
    fp: Option<FpQuestion>,
}

impl Claim<'_> {
    /// **kaspad's seat verb.** The attempt lane hands `verify_material` the served bytes; the
    /// free-prompt lane serves an `FPC1` envelope and the seat unwraps it through
    /// `palw_fp_capture_decode_v1` under the class's form first (`palw_panel::fp_capture_view`).
    fn seat(&self, capture: &[u8], roots: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        match &self.fp {
            None => self.backend.verify_material(capture, roots),
            Some(q) => {
                let served = palw_fp_capture_encode_v1(&q.job, &q.ids, capture);
                let view = palw_fp_capture_decode_v1(&served, q.form).expect("the served envelope admits its own ids").capture;
                assert_eq!(view, capture, "the envelope carries the capture byte for byte");
                self.backend.verify_material(&view, roots)
            }
        }
    }

    fn honest(&self) -> Mat {
        Mat::decode(&self.material)
    }

    /// The honest half: the seat licenses it, the pre-SEAT-0 check licensed it too, and every
    /// SEAT-0 rule passes — with rule 5 armed on dense material (the head is known).
    fn assert_honest_matches(&self) {
        {
            let m = self.honest();
            let b = m.binding();
            eprintln!(
                "{}: {} retention, {:?} cadence, D={}, {} step leaves, {} checkpoints at interval {}",
                self.label,
                if m.is_dense() { "dense" } else { "fold" },
                palw_checkpoint_cadence_v1(&b.shape_profile),
                m.ids().len(),
                b.step_leaf_count,
                b.checkpoint_count,
                b.checkpoint_profile.checkpoint_interval
            );
        }
        assert!(licensed_before_seat0(&self.material, self.roots), "{}: the pre-SEAT-0 seat licensed the honest run", self.label);
        assert_eq!(
            self.seat(&self.material, self.roots),
            PalwMaterialVerdictV1::Matches,
            "{}: an honest claim is licensed",
            self.label
        );
        let m = self.honest();
        let leaves = m.dense_leaves();
        assert_eq!(
            base0_seat_rules_v1(m.binding(), m.rows(), m.ids(), leaves.as_deref(), self.family),
            Ok(()),
            "{}: every SEAT-0 rule holds on honest material",
            self.label
        );
        assert!(
            base0_logits_head_v1(&m.binding().shape_profile).is_some(),
            "{}: this class's head is one SEAT-0 knows, so rule 5 is armed for its dense material",
            self.label
        );
    }

    /// The attack half: licensed before SEAT-0, a `Mismatch` through kaspad's verb now, refused by
    /// exactly the named rule.
    fn assert_refused(&self, what: &str, forged: &Mat, roots: PalwClaimRootsV1, rule: Base0SeatRefusalV1) {
        let bytes = forged.encode();
        assert!(
            licensed_before_seat0(&bytes, roots),
            "{} {what}: the attack must be one the pre-SEAT-0 seat LICENSED, or this test proves nothing",
            self.label
        );
        assert_eq!(self.seat(&bytes, roots), PalwMaterialVerdictV1::Mismatch, "{} {what}: kaspad's seat verb refuses it", self.label);
        let leaves = forged.dense_leaves();
        assert_eq!(
            base0_seat_rules_v1(forged.binding(), forged.rows(), forged.ids(), leaves.as_deref(), self.family),
            Err(rule),
            "{} {what}: refused by the rule named for it",
            self.label
        );
    }

    fn roots_of(&self, m: &Mat) -> PalwClaimRootsV1 {
        PalwClaimRootsV1 {
            execution_root: m.binding().committed_execution_root,
            trace_root: m.binding().full_logits_trace_root,
            ..self.roots
        }
    }

    /// T1–T5 and X on either retention; R1 and H on dense material.
    fn assert_every_attack_refused(&self) {
        let honest = self.honest();
        let vocab = honest.binding().shape_profile.vocab_size;

        // T1: the committed root is a free field — no preimage at all.
        let mut t1 = honest.clone();
        t1.binding_mut().committed_execution_root = Hash64::from_u64_word(0xF4EE_0001);
        assert!(verify_binding_v1(t1.binding()).is_err());
        let roots = self.roots_of(&t1);
        self.assert_refused("T1 unbound root", &t1, roots, Base0SeatRefusalV1::BindingNotItsOwnRoot);

        // T2: the activation leg moved and re-bound — a real preimage over no execution.
        let mut t2 = honest.clone();
        t2.binding_mut().activation_leg_root = Hash64::from_u64_word(0xAC7);
        rebind(t2.binding_mut());
        let roots = self.roots_of(&t2);
        self.assert_refused("T2 activation leg", &t2, roots, Base0SeatRefusalV1::ActivationLegNotTheFamilys);

        // T3: the checkpoint interval moved and the leg re-chained under it.
        let t3 = self.moved_checkpoint_interval(&honest);
        let roots = self.roots_of(&t3);
        self.assert_refused("T3 checkpoint interval", &t3, roots, Base0SeatRefusalV1::CheckpointProfileNotTheFamilys);

        // T4: token 0 is a real lane, not its row's selection; the trace re-rooted.
        let mut t4 = honest.clone();
        {
            let (rows, ids) = t4.rows_ids_mut();
            let selected = base0_decode_token_select_v1(&rows[0]) as u32;
            assert_eq!(ids[0], selected, "{}: the honest token is its row's selection", self.label);
            ids[0] = (selected + 1) % vocab;
        }
        retrace(&mut t4);
        let roots = self.roots_of(&t4);
        self.assert_refused("T4 token not selected", &t4, roots, Base0SeatRefusalV1::TokenNotSelected { position: 0 });

        // T5: token 0 outside the vocabulary; the trace re-rooted.
        let mut t5 = honest.clone();
        t5.rows_ids_mut().1[0] = vocab + 3;
        retrace(&mut t5);
        let roots = self.roots_of(&t5);
        self.assert_refused("T5 token out of vocabulary", &t5, roots, Base0SeatRefusalV1::TokenOutOfVocab { position: 0 });

        // X: one more decode row than the job has, its id selected from it; the trace re-rooted.
        let mut x = honest.clone();
        {
            let (rows, ids) = x.rows_ids_mut();
            let mut extra = rows[rows.len() - 1].clone();
            extra.rotate_left(1);
            ids.push(base0_decode_token_select_v1(&extra) as u32);
            rows.push(extra);
        }
        retrace(&mut x);
        let roots = self.roots_of(&x);
        self.assert_refused("X extra decode row", &x, roots, Base0SeatRefusalV1::DecodeNotTheJobs);

        if honest.is_dense() {
            // R1: the LAST row bent above its argmax, the id following it — the step tree is
            // untouched (the last id feeds no later call) and T4's rule is satisfied.
            let mut r1 = honest.clone();
            let last = {
                let (rows, ids) = r1.rows_ids_mut();
                let last = rows.len() - 1;
                let row = &mut rows[last];
                let a = base0_decode_token_select_v1(row);
                let k = (a + 1) % row.len();
                row[k] = row[a].checked_add(1).expect("an honest logit below i32::MAX");
                assert_eq!(base0_decode_token_select_v1(row), k);
                ids[last] = k as u32;
                last as u32
            };
            retrace(&mut r1);
            let roots = self.roots_of(&r1);
            self.assert_refused("R1 bent logits row", &r1, roots, Base0SeatRefusalV1::LogitsNotTheHeadOutput { row: last });

            // H: honest logits beside a lying head tile — the drill's corruption, aimed at the head.
            let mut h = honest.clone();
            {
                let b = h.binding().clone();
                let head = base0_logits_head_v1(&b.shape_profile).expect("a known head");
                let coord = PalwStepCoordinateV1 {
                    call_index: 0,
                    node_slot: head.slot,
                    position: b.job_context.declared_prefill_tokens - 1,
                    tile_index: 0,
                };
                let index = canonical_step_leaf_index(&b.shape_profile, &b.job_context, &coord).expect("the head's first tile");
                let Mat::Dense(t) = &mut h else { unreachable!() };
                let tile = t.1.iter_mut().rev().find(|(i, _)| *i == index).expect("the dense capture holds the head tile");
                tile.1.values_le[0] = tile.1.values_le[0].wrapping_add(1);
                let root = base0_dense_step_root_capped_v1(&t.0, &t.1, PALW_STEP_LEG_MAX_LEAVES).expect("roots");
                t.0.step_merkle_root = root;
            }
            rebind(h.binding_mut());
            let roots = self.roots_of(&h);
            self.assert_refused("H lying head tile", &h, roots, Base0SeatRefusalV1::LogitsNotTheHeadOutput { row: 0 });
        }
    }

    /// T3's forgery, under whichever cadence the class runs.
    ///
    /// Per decode call (the floor, A16 v2, Qwen3.6 v2): the interval goes to `u32::MAX`, so the
    /// canonical count is zero whatever the decode length — every checkpoint the class owes is
    /// DROPPED, the served chunks with it, and the leg is the empty one. Per position (the held
    /// maps): the count does not depend on the interval, so the leaves are re-chained under the new
    /// profile's hash, which is all a producer needs to move it.
    fn moved_checkpoint_interval(&self, honest: &Mat) -> Mat {
        let mut m = honest.clone();
        let b = m.binding().clone();
        let mut moved = b.checkpoint_profile.clone();
        match palw_checkpoint_cadence_v1(&b.shape_profile) {
            PalwCheckpointCadenceV1::PerDecodeCall => {
                moved.checkpoint_interval = u32::MAX;
                match &mut m {
                    Mat::Dense(t) => t.4.clear(),
                    Mat::Fold(f) => {
                        f.checkpoint_chunks.clear();
                        f.checkpoint_leaves.clear();
                    }
                }
                let rebuilt = Base0CheckpointCaptureV1::from_chunks_v1(&b.job_context, &b.shape_profile, &moved, &[])
                    .expect("the empty leg rebuilds");
                let binding = m.binding_mut();
                binding.checkpoint_count = rebuilt.leaf_hashes.len() as u32;
                binding.checkpoint_merkle_root = rebuilt.merkle_root;
                binding.checkpoint_profile = moved;
            }
            PalwCheckpointCadenceV1::PerPosition => {
                moved.checkpoint_interval = moved.checkpoint_interval.wrapping_add(6);
                let Mat::Fold(f) = &mut m else { panic!("{}: a per-position class is attacked through its fold", self.label) };
                let ctx_hash = b.job_context.context_hash();
                let moved_hash = moved.profile_hash();
                let mut prev = checkpoint_genesis_prev_v2(&ctx_hash);
                for leaf in f.checkpoint_leaves.iter_mut() {
                    leaf.prev_checkpoint_leaf_hash = prev;
                    prev = checkpoint_leaf_hash_v2(&ctx_hash, &moved_hash, &b.state_chunk_map_id, leaf);
                }
                let rebuilt = Base0CheckpointCaptureV1::from_leaves_v1(&b.job_context, &b.shape_profile, &moved, &f.checkpoint_leaves)
                    .expect("the re-chained leg rebuilds");
                f.binding.checkpoint_count = rebuilt.leaf_hashes.len() as u32;
                f.binding.checkpoint_merkle_root = rebuilt.merkle_root;
                f.binding.checkpoint_profile = moved;
            }
        }
        rebind(m.binding_mut());
        assert_ne!(m.binding().committed_execution_root, honest.binding().committed_execution_root, "the root moved");
        m
    }
}

// ------------------------------------------------------------------------------------------------
// Fixtures: the families the live chain uses, built the way kaspad's SDK builds their backends
// ------------------------------------------------------------------------------------------------

/// The floor, resolved from nothing as kaspad's dense lineage resolves it, at the network's ladder
/// and prompt form.
fn floor_backend(form: PalwPromptIdsFormV1) -> Base0Backend {
    use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
    let court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 4, 2).expect("the shipped court");
    let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is registered");
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
    Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves from nothing"))
        .with_step_ladder_cap(court.max_step_leaf_count())
        .with_prompt_ids_form(form)
}

/// The held A16 fixture (`qwen25_a16_backend`'s `fixture()`: graph-v7, the held map), at `vocab` —
/// 8,292 is ragged against the 4,096-lane logits tile and against nothing else.
fn a16_geometry(vocab: u32) -> kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
    kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: vocab,
        n_ctx: 128,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    }
}

fn a16_artifact(vocab: u32) -> Arc<Base0ArtifactV1> {
    let g = a16_geometry(vocab);
    let shape = Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: g.rms_eps_q,
    };
    Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    )
}

fn a16_backend(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> Qwen25A16Backend {
    Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), canonical)
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES)
        .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
}

/// The Qwen3.6 dev fixture and its geometry (`fuzz_qwen36`'s tiny class).
fn qwen36_fixture()
-> (Arc<misaka_palw_base0::qwen36::Qwen36ArtifactV1>, kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1) {
    let geometry = kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
        layer_count: 4,
        full_attention_interval: 4,
        hidden_dim: 32,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 16,
        rope_dims: 4,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: 2,
        gdn_v_heads: 4,
        gdn_head_dim: 8,
        gdn_conv_kernel: 4,
        n_experts: 8,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        attn_output_gate: 1,
        vocab_size: 64,
        n_ctx: 8,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    };
    (Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8)), geometry)
}

/// A free-prompt job of `class`, its prompt committed under `form` (the class's own).
fn fp_job(class: &PalwShapeProfileV3, form: PalwPromptIdsFormV1, prompt: &[usize], decode: u32, prompt_mode: u8) -> FpQuestion {
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: class.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 4242,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(form, &ids).expect("the ids commit"),
        prompt_tokens: prompt.len() as u32,
        decode_token_limit: decode,
        max_context_tokens: class.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
    };
    FpQuestion { job, ids, form }
}

/// An attempt claim, produced by the backend's own `execute` of the anchor's job at the draw.
fn attempt_claim<'a>(
    label: String,
    backend: &'a dyn PalwExecutionBackendV1,
    family: Base0SeatFamilyV1,
    anchor: Hash64,
    draw: bool,
) -> Claim<'a> {
    let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
    let job = palw_attempt_job_v1(job, draw);
    let out = backend.execute(&job, &prompt).expect("the honest attempt runs");
    Claim {
        label,
        backend,
        family,
        material: out.material,
        roots: PalwClaimRootsV1 {
            execution_root: out.execution_root,
            trace_root: out.trace_root,
            anchor,
            attempt_draw: Some(draw),
            output_root: None,
        },
        fp: None,
    }
}

/// A free-prompt claim, produced by the backend's own `execute_free_prompt`.
fn fp_claim<'a>(label: String, backend: &'a dyn PalwExecutionBackendV1, family: Base0SeatFamilyV1, q: FpQuestion) -> Claim<'a> {
    let prompt: Vec<usize> = q.ids.iter().map(|t| *t as usize).collect();
    let run = backend.execute_free_prompt(&q.job, &prompt).expect("the honest free-prompt run");
    Claim {
        label,
        backend,
        family,
        material: run.outcome.material,
        roots: PalwClaimRootsV1 {
            execution_root: run.outcome.execution_root,
            trace_root: run.outcome.trace_root,
            anchor: fp_job_id_v3(&q.job),
            attempt_draw: None,
            output_root: None,
        },
        fp: Some(q),
    }
}

// ------------------------------------------------------------------------------------------------
// The floor: attempts (several anchors, both draws, both prompt forms) and free prompts
// ------------------------------------------------------------------------------------------------

#[test]
fn the_floor_licenses_every_honest_attempt_and_refuses_every_forgery() {
    for form in [PalwPromptIdsFormV1::MerkleV1, PalwPromptIdsFormV1::Flat] {
        let backend = floor_backend(form);
        for (n, anchor) in [0x5EA7_0001u64, 0x5EA7_0002, 0xF100, 0xDEAD_BEEF, 0x0123_4567_89AB_CDEF, u64::MAX].into_iter().enumerate()
        {
            for draw in [true, false] {
                let claim = attempt_claim(
                    format!("floor {form:?} anchor#{n} draw={draw}"),
                    &backend,
                    Base0SeatFamilyV1::IntegerKv,
                    Hash64::from_u64_word(anchor),
                    draw,
                );
                claim.assert_honest_matches();
                assert_eq!(claim.honest().ids().len(), if draw { 1 } else { 4 }, "{}: the job the block asked for", claim.label);
                // The forgeries on a few anchors; the honest half on all of them.
                if n < 2 {
                    claim.assert_every_attack_refused();
                }
            }
        }
    }
}

#[test]
fn the_floor_licenses_every_honest_free_prompt_and_refuses_every_forgery() {
    for form in [PalwPromptIdsFormV1::MerkleV1, PalwPromptIdsFormV1::Flat] {
        let backend = floor_backend(form);
        let class_form = backend.prompt_ids_form();
        let vocab = backend.profile().vocab_size as usize;
        for (prompt_len, decode, mode) in
            [(1usize, 11u32, PALW_FP_PROMPT_MODE_USER), (6, 2, PALW_FP_PROMPT_MODE_USER), (5, 4, PALW_FP_PROMPT_MODE_CANONICAL)]
        {
            let prompt: Vec<usize> = (0..prompt_len).map(|i| (i * 7919 + 1013) % vocab).collect();
            let q = fp_job(backend.profile(), class_form, &prompt, decode, mode);
            let claim =
                fp_claim(format!("floor FP {form:?} {prompt_len}+{decode} mode {mode}"), &backend, Base0SeatFamilyV1::IntegerKv, q);
            assert!(claim.honest().is_dense(), "the floor retains its free-prompt runs dense");
            claim.assert_honest_matches();
            claim.assert_every_attack_refused();
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The A16 tier: dense attempts, the fold (free prompt, and an attempt served as a fold), the
// held map
// ------------------------------------------------------------------------------------------------

#[test]
fn the_a16_tier_licenses_honest_dense_material_and_refuses_every_forgery() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2;
    for vocab in [128u32, 8_292] {
        let artifact = a16_artifact(vocab);
        // The per-call map, whose dense retention a seat can check (a held map's dense retention
        // carries no checkpoint leaves; see the held test below).
        let profile = qwen25_a16_profile_v2(a16_geometry(vocab)).expect("the v2 row projects");
        let backend = a16_backend(&artifact, &profile, (15, 2));
        for (anchor, draw) in [(0xA16_0001u64, true), (0xA16_0002, false), (0xA16_0003, true)] {
            let claim = attempt_claim(
                format!("A16 v2 V={vocab} dense anchor {anchor:#x} draw={draw}"),
                &backend,
                Base0SeatFamilyV1::IntegerKv,
                Hash64::from_u64_word(anchor),
                draw,
            );
            assert!(claim.honest().is_dense(), "an A16 attempt is retained dense");
            claim.assert_honest_matches();
            claim.assert_every_attack_refused();
        }
    }
}

#[test]
fn the_a16_fold_licenses_honest_free_prompts_and_refuses_every_forgery() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v2, qwen25_a16_profile_v7};
    for vocab in [128u32, 8_292] {
        let artifact = a16_artifact(vocab);
        let held = qwen25_a16_profile_v7(a16_geometry(vocab)).expect("the held graph-v7 row projects");
        assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&held));
        let per_call = qwen25_a16_profile_v2(a16_geometry(vocab)).expect("the v2 row projects");
        for (label, profile) in [("held v7", held), ("v2", per_call)] {
            let backend = a16_backend(&artifact, &profile, qwen25_a16_held_canonical_v1(profile.n_ctx));
            let form = backend.prompt_ids_form();
            for (prompt_len, decode) in [(12usize, 4u32), (1, 3), (33, 2)] {
                let prompt: Vec<usize> = (0..prompt_len).map(|i| (i * 7919 + 1013) % vocab as usize).collect();
                let q = fp_job(&profile, form, &prompt, decode, PALW_FP_PROMPT_MODE_USER);
                let claim =
                    fp_claim(format!("A16 {label} V={vocab} FP {prompt_len}+{decode}"), &backend, Base0SeatFamilyV1::IntegerKv, q);
                assert!(!claim.honest().is_dense(), "the A16 free-prompt lane retains a fold");
                claim.assert_honest_matches();
                claim.assert_every_attack_refused();
            }
        }
    }
}

/// **An attempt served as a fold.** On this line a HELD class's attempt folds
/// (`palw_attempt_capture_folds_v1`), so there the fold is the honest producer's own material:
/// licensed, and T1–T5 refused on it by the rule named for each. On a class whose attempts keep
/// their tiles no producer writes one, and SEAT-0's head rule reads dense leaves only, so a fold's
/// selecting row would be a free field (`seat0_review_regressions`): the seat refuses it outright —
/// even the honest one, though every SEAT-0 rule holds on it — and T1–T5 are still refused.
#[test]
fn an_attempt_served_as_a_fold_is_held_to_the_same_rules() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v2, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1;
    let vocab = 8_292u32;
    let artifact = a16_artifact(vocab);
    let held = qwen25_a16_profile_v7(a16_geometry(vocab)).expect("the held graph-v7 row projects");
    let per_call = qwen25_a16_profile_v2(a16_geometry(vocab)).expect("the v2 row projects");
    for (label, profile) in [("held v7", held), ("v2", per_call)] {
        let folds = palw_attempt_capture_folds_v1(&profile);
        assert_eq!(folds, label == "held v7", "{label}: only the held row's attempt folds");
        let backend = a16_backend(&artifact, &profile, qwen25_a16_held_canonical_v1(profile.n_ctx));
        let plan = misaka_palw_base0::engine_a16::A16Engine::new(&artifact)
            .expect("an A16 artifact")
            .plan_from_profile(&profile)
            .expect("the registered graph compiles");
        for (anchor, draw) in [(0xF01D_0001u64, true), (0xF01D_0002, false)] {
            let anchor = Hash64::from_u64_word(anchor);
            let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
            let job = palw_attempt_job_v1(job, draw);
            let run = a16_execute_free_prompt_streaming_v1(
                &artifact,
                &profile,
                Some(&plan),
                &job,
                &prompt,
                backend.step_ladder_cap(),
                &mut |_| {},
            )
            .expect("the fold runs the attempt job");
            let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
            let claim = Claim {
                label: format!("A16 {label} attempt-as-fold draw={draw}"),
                backend: &backend,
                family: Base0SeatFamilyV1::IntegerKv,
                material: base0_fp_material_encode_v2(&run, &ids).expect("the fold retains"),
                roots: PalwClaimRootsV1 {
                    execution_root: run.execution_root,
                    trace_root: run.trace_root,
                    anchor,
                    attempt_draw: Some(draw),
                    output_root: None,
                },
                fp: None,
            };
            if folds {
                claim.assert_honest_matches();
            } else {
                assert!(licensed_before_seat0(&claim.material, claim.roots), "{}: the pre-SEAT-0 seat licensed it", claim.label);
                assert_eq!(
                    claim.seat(&claim.material, claim.roots),
                    PalwMaterialVerdictV1::Mismatch,
                    "{}: an attempt of a class whose attempts keep their tiles is not licensed as a fold, honest or not",
                    claim.label
                );
                let m = claim.honest();
                assert_eq!(base0_seat_rules_v1(m.binding(), m.rows(), m.ids(), None, claim.family), Ok(()), "{}", claim.label);
            }
            claim.assert_every_attack_refused();
        }
    }
}

/// **The held map's attempt keeps its verdict.** On this line a held class's attempt FOLDS
/// (`palw_attempt_capture_folds_v1`; the dense sink of a 2M attempt is terabytes), so its material
/// is the fold and is licensed by the material route — before SEAT-0 and after. (On the live
/// 2bd134ec line the same row kept dense tiles, retained no checkpoint chunks and was never
/// licensed by material.) Pinned so the claim "SEAT-0 moves no honest verdict" covers this row too,
/// rather than being true of it by omission.
#[test]
fn the_held_maps_dense_retention_keeps_the_verdict_it_had() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    let artifact = a16_artifact(128);
    let profile = qwen25_a16_profile_v7(a16_geometry(128)).expect("the held graph-v7 row projects");
    let backend = a16_backend(&artifact, &profile, qwen25_a16_held_canonical_v1(profile.n_ctx));
    for draw in [true, false] {
        let claim = attempt_claim(
            format!("A16 held v7 dense draw={draw}"),
            &backend,
            Base0SeatFamilyV1::IntegerKv,
            Hash64::from_u64_word(0x4E1D),
            draw,
        );
        let before = licensed_before_seat0(&claim.material, claim.roots);
        let now = claim.seat(&claim.material, claim.roots) == PalwMaterialVerdictV1::Matches;
        eprintln!("{}: licensed before SEAT-0 = {before}, now = {now}", claim.label);
        assert_eq!(now, before, "{}: SEAT-0 does not move this verdict", claim.label);
        // And SEAT-0 itself has nothing against it: every rule, the head's included, holds.
        let m = claim.honest();
        let leaves = m.dense_leaves();
        assert_eq!(base0_seat_rules_v1(m.binding(), m.rows(), m.ids(), leaves.as_deref(), claim.family), Ok(()), "{}", claim.label);
    }
}

// ------------------------------------------------------------------------------------------------
// Qwen3.6: the family whose checkpoint cadence is not the integer-kv one
// ------------------------------------------------------------------------------------------------

#[test]
fn the_qwen36_hybrid_licenses_its_honest_material_and_refuses_every_forgery() {
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_profile_v2, qwen36_profile_v7};
    let (artifact, geometry) = qwen36_fixture();
    let v2 = qwen36_profile_v2(geometry).expect("the corrected tables project");
    let v7 = qwen36_profile_v7(geometry).expect("the held graph-v7 projection");
    // The all-attention (qwen3moe) member this backend also serves: no recurrence heads at all.
    let moe_artifact = Arc::new(misaka_palw_base0::qwen36::qwen3moe_dev_fixture(3, 8));
    let moe = qwen36_profile_v2(kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
        layer_count: 3,
        full_attention_interval: 1,
        rope_dims: 16,
        gdn_k_heads: 0,
        gdn_v_heads: 0,
        gdn_head_dim: 0,
        gdn_conv_kernel: 0,
        shared_dim: 0,
        attn_output_gate: 0,
        ..geometry
    })
    .expect("the stripped geometry projects");
    for (label, artifact, profile, canonical) in
        [("v2", &artifact, v2, (3, 4)), ("held v7", &artifact, v7, (3, 4)), ("qwen3moe v2", &moe_artifact, moe, (4, 2))]
    {
        let backend = Qwen36Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), profile.clone(), canonical)
            .expect("servable")
            .with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES)
            .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1);
        // J7 is the FAMILY's: this class's producer checkpoints at `n_ctx`, not at 1 — and that is
        // true of the all-attention member too, whose profile declares no recurrence heads. A rule
        // read off the profile ("recurrence heads ⇒ `n_ctx`, else 1") would refuse every honest
        // qwen3moe producer; the seat's backend names the family instead.
        assert_eq!(
            Base0SeatFamilyV1::Qwen36.checkpoint_profile_v1(&profile).checkpoint_interval,
            profile.n_ctx,
            "{label}: the hybrid family's interval"
        );
        if label.starts_with("qwen3moe") {
            assert_eq!(profile.gdn_heads, 0, "the all-attention member declares no recurrence heads");
            assert_ne!(profile.n_ctx, 1);
        }
        let per_call = palw_checkpoint_cadence_v1(&profile) == PalwCheckpointCadenceV1::PerDecodeCall;
        for (anchor, draw) in [(0x0936_0001u64, true), (0x0936_0002, false)] {
            let claim = attempt_claim(
                format!("Qwen3.6 {label} dense draw={draw}"),
                &backend,
                Base0SeatFamilyV1::Qwen36,
                Hash64::from_u64_word(anchor),
                draw,
            );
            if per_call {
                claim.assert_honest_matches();
                claim.assert_every_attack_refused();
                // The integer-kv rule is NOT this family's: a seat that applied it would refuse an honest producer.
                let m = claim.honest();
                let leaves = m.dense_leaves();
                assert_eq!(
                    base0_seat_rules_v1(m.binding(), m.rows(), m.ids(), leaves.as_deref(), Base0SeatFamilyV1::IntegerKv),
                    Err(Base0SeatRefusalV1::CheckpointProfileNotTheFamilys),
                    "{}: the family is the seat's to name",
                    claim.label
                );
            } else {
                let before = licensed_before_seat0(&claim.material, claim.roots);
                let now = claim.seat(&claim.material, claim.roots) == PalwMaterialVerdictV1::Matches;
                eprintln!("{}: licensed before SEAT-0 = {before}, now = {now}", claim.label);
                assert_eq!(now, before, "{}: SEAT-0 does not move this verdict", claim.label);
                // SEAT-0 itself holds against it, the head rule on the hybrid's dense tiles included.
                let m = claim.honest();
                let leaves = m.dense_leaves();
                assert_eq!(
                    base0_seat_rules_v1(m.binding(), m.rows(), m.ids(), leaves.as_deref(), claim.family),
                    Ok(()),
                    "{}",
                    claim.label
                );
            }
        }
        let form = backend.prompt_ids_form();
        let q = fp_job(&profile, form, &[3, 1, 4], 4, PALW_FP_PROMPT_MODE_USER);
        let claim = fp_claim(format!("Qwen3.6 {label} FP"), &backend, Base0SeatFamilyV1::Qwen36, q);
        assert!(!claim.honest().is_dense(), "the hybrid's free-prompt lane retains a fold");
        claim.assert_honest_matches();
        claim.assert_every_attack_refused();
    }
}

// ------------------------------------------------------------------------------------------------
// The rules' own edges
// ------------------------------------------------------------------------------------------------

/// **The head rule is armed exactly on the classes whose head it knows** — the floor, the A16
/// tier and Qwen3.6, at their fixture geometries and at the real t12 widths — and never guesses
/// for a class whose last node is not a vocab-wide integer matmul.
#[test]
fn the_head_rule_knows_the_live_heads_and_nothing_else() {
    use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7;
    let floor =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor's profile");
    let (_, q36) = qwen36_fixture();
    let mut known = vec![
        ("floor", floor),
        ("A16 held v7 fixture", qwen25_a16_profile_v7(a16_geometry(8_292)).expect("v7")),
        ("Qwen3.6 held v7 fixture", qwen36_profile_v7(q36).expect("v7")),
    ];
    // The t12 rows as `qwen25_a16_held_registration_v1` / `qwen36_held_registration_v1` build
    // them: the real geometries, at the dense widths the chain carries and the hybrid's 512.
    for n_ctx in [8_192u32, 2_097_152] {
        let geometry = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B };
        known.push(("A16 held v7 at a t12 width", qwen25_a16_artifact_row_profile_v7(geometry).expect("the held row")));
    }
    {
        use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps};
        let geometry = qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B });
        known.push(("Qwen3.6 held v7 at t12's 512", qwen36_profile_v7(geometry).expect("the held hybrid row")));
    }
    for (label, profile) in &known {
        let head = base0_logits_head_v1(profile).unwrap_or_else(|| panic!("{label}: the head is known"));
        assert_eq!(head.slot, profile.global_node_count() - 1, "{label}: the head is the last slot");
    }
    // A class whose last node is anything else is not this seat's to guess about.
    let mut other = known[0].1.clone();
    other.post_nodes.last_mut().expect("a post table").op_kind = kaspa_consensus_core::palw_step::PalwStepOpKindV1::RmsNorm;
    assert_eq!(base0_logits_head_v1(&other), None);
    let mut narrow = known[0].1.clone();
    narrow.post_nodes.last_mut().expect("a post table").out_len =
        kaspa_consensus_core::palw_step::PalwStepOutLenV1::Fixed { elements: known[0].1.vocab_size - 1 };
    assert_eq!(base0_logits_head_v1(&narrow), None);
    let mut foreign = known[0].1.clone();
    foreign.post_nodes.last_mut().expect("a post table").kernel_semantics_id = Hash64::from_u64_word(0xF0);
    assert_eq!(base0_logits_head_v1(&foreign), None);
}

/// **A stranger's malformed material is a refusal, never a panic** — SEAT-0's rules read a
/// stranger's binding, rows and ids, so every shape they index by must be refused first.
#[test]
fn malformed_material_is_refused_not_fatal() {
    let backend = floor_backend(PalwPromptIdsFormV1::MerkleV1);
    let claim = attempt_claim("floor malformed".into(), &backend, Base0SeatFamilyV1::IntegerKv, Hash64::from_u64_word(0xBAD5), false);
    let honest = claim.honest();
    let leaves = honest.dense_leaves().expect("dense");
    let family = Base0SeatFamilyV1::IntegerKv;
    let b = honest.binding();
    let (rows, ids) = (honest.rows(), honest.ids());
    // No rows, no ids; rows without ids; an empty row; a row wider than the vocabulary; a leaf
    // vector shorter than the head's coordinates.
    assert_eq!(base0_seat_rules_v1(b, &[], &[], Some(&leaves), family), Err(Base0SeatRefusalV1::DecodeNotTheJobs));
    assert_eq!(base0_seat_rules_v1(b, rows, &[], Some(&leaves), family), Err(Base0SeatRefusalV1::DecodeNotTheJobs));
    let mut empty_row = rows.to_vec();
    empty_row[0].clear();
    assert_eq!(
        base0_seat_rules_v1(b, &empty_row, ids, Some(&leaves), family),
        Err(Base0SeatRefusalV1::TokenNotSelected { position: 0 })
    );
    let mut wide = rows.to_vec();
    wide[1].push(i32::MIN);
    assert_eq!(base0_seat_rules_v1(b, &wide, ids, Some(&leaves), family), Err(Base0SeatRefusalV1::LogitsNotTheHeadOutput { row: 1 }));
    assert_eq!(
        base0_seat_rules_v1(b, rows, ids, Some(&leaves[..1]), family),
        Err(Base0SeatRefusalV1::LogitsNotTheHeadOutput { row: 0 })
    );
    // A context with no prefill has no last prefill position: a self-consistent binding over one
    // (its own activation leg, re-bound) reaches the head rule, which refuses it rather than
    // underflowing.
    let mut no_prefill = b.clone();
    no_prefill.job_context.declared_prefill_tokens = 0;
    no_prefill.activation_leg_root = misaka_palw_base0::produce::base0_activation_leg_root_v1(&no_prefill.job_context);
    rebind(&mut no_prefill);
    assert_eq!(
        base0_seat_rules_v1(&no_prefill, rows, ids, Some(&leaves), family),
        Err(Base0SeatRefusalV1::LogitsNotTheHeadOutput { row: 0 })
    );
    // And through kaspad's verb, bytes that are the honest material with a byte flipped anywhere
    // are never licensed and never take the process down.
    let bytes = &claim.material;
    for at in (0..bytes.len()).step_by(bytes.len() / 97 + 1) {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0x41;
        assert_ne!(
            claim.seat(&flipped, claim.roots),
            PalwMaterialVerdictV1::Matches,
            "a flipped byte at {at} is not the claim's material"
        );
    }
}
