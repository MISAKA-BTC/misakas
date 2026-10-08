//! **G14 lane D, phase 3: model onboarding on the real node** — the consensus objects and rows that tie a V2 model-registry class to
//! the kernel route (RFC-0011, RFC-0013, RFC-0015), all dormant behind `Params::palw_probabilistic_constraints_v1` (which no network
//! can arm). Tags 104–108; the rows ride the kernel route's aux tables 36–38 (so the delta entries `KernelRouteRow`, the carriage tail
//! `0xEC` and the `kernel-route/v1` root block already carry, revert and root them — no new delta, tail or root block).
//!
//! ```text
//! V2 class (Registered) ── 104 ArtifactBound ─► Pending ─(window)─► Matured ─(liability)─► Final        105 refutes (slash)
//!                                                  │ Matured/Final: the kernel route may attest `kernel_param_root`
//! kernel RegisterClass over that root ───────────────────────────────────────────────────────────────► kernel class
//! 106 KernelBound: V2 class ↔ kernel class (same program bytes, same artifact, the network's challenge policy)
//! 107 ConformanceCommitted: the RFC-0013 statement per (class, artifact root)
//! activation gate: a kernel-bound class leaves Registered only when the binding is Final, the kernel class stands (the route
//! registered it only after `PUBLIC_PROSECUTION_COMPLETE`) and a conformance commitment is on chain
//! ```
//!
//! # The artifact attestation (least trust)
//!
//! A kernel class registers only over an artifact root the consumer attests public. The V2 registry's `artifact_root` is a Merkle
//! root over `(tensor name, layer, byte offset, bytes)` leaves; the kernel's `ParamCommitmentsV1` root is over per-tensor
//! row/column commitments. They are roots of the SAME bytes in two unrelated hash structures, so no function of the two roots
//! shows they agree, and the fold has no bytes to compare — "same bytes ⇒ both roots" cannot be CHECKED at registration without
//! RFC-0014 §16 availability. What the chain can do, and does here, is make the statement **bonded and refutable**:
//!
//! * `ArtifactBoundV1` (104) is the registrant of an existing V2 class stating that its artifact has kernel root `K`. It reserves a
//!   slice of the registrant's free collateral until a liability horizon ends; `K` is attested to the kernel route only after a
//!   challenge window, and never once refuted.
//! * `ArtifactBindingChallengedV1` (105) is the fraud proof: two openings of the same coordinates — the V2 class's inventory leaf
//!   (a Merkle path to the registered `artifact_root`) and the kernel's tensor row (a path to the bound commitment) — that disagree
//!   in dtype, shape or bytes, or an instance set that differs from the program's declaration. Everything it needs is on chain
//!   (the program, both roots) or in the proof. It slashes the registrant's reservation (half to the challenger).
//!
//! The residual assumption is exactly the OPV one: at least one capable honest party can obtain the artifact bytes within the
//! window. A binding nobody can challenge is a declaration; the fence stays dormant until RFC-0014 §16 closes that.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::merkle::{AXIS_ROW, LayoutV1, TensorOpeningV1};
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::TirProgramV1;
use std::collections::BTreeSet;

use crate::Hash64;
use crate::constants::SOMPI_PER_KASPA;
use crate::palw_artifact::{PalwArtifactOpeningV1, verify_artifact_opening_v1};
use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_tir_artifact_v1::{palw_tir_inventory_leaf_count_v1, palw_tir_leaf_index_v1, palw_tir_param_instances_v1};

/// The consensus tables of the route's aux rows that hold onboarding state (the route's own are 32–35).
pub const PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1: u8 = 36;
pub const PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1: u8 = 37;
pub const PALW_ONBOARDING_TABLE_CONFORMANCE_V1: u8 = 38;

/// The ML-DSA-87 context of every onboarding object's signature.
pub const PALW_ONBOARDING_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/onboarding/object/v1";
const PALW_ONBOARDING_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/onboarding/object-message/v1";
/// The envelope's own message domain and context (RFC-0009 G-EXPIRY / G-RULESET).
pub const PALW_SIGNED_REGISTRATION_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/onboarding/signed-registration/v1";
const PALW_SIGNED_REGISTRATION_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/onboarding/signed-registration-message/v1";

/// **INTERIM onboarding terms** — consensus constants of the (never-armed) route fence, written once here; a real activation would
/// revisit every one. The reservation is what a refuted binding costs; the window is how long a binding is Pending (not yet
/// attested); the liability is how long it stays refutable (and reserved) — it equals the kernel ledger's own liability horizon.
pub const PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1: u64 = 100 * SOMPI_PER_KASPA;
pub const PALW_ONBOARDING_BINDING_WINDOW_DAA_V1: u64 = 40;
pub const PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1: u64 = 200;
/// The challenger's share of a refuted binding's slash (permille); the rest is burned (the slash's burn at release).
pub const PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1: u64 = 500;

/// **The message an onboarding object's signer signs**: `H(domain; network ‖ kind ‖ signer ‖ len ‖ payload)`. `payload` is the
/// object's Borsh with its signature field left out, so the signature covers every other field.
pub fn palw_onboarding_message_v1(network_domain: Hash64, kind: u8, signer: &PalwBondKeyV2, payload: &[u8]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_ONBOARDING_MESSAGE_DOMAIN_V1).to_state();
    s.update(network_domain.as_byte_slice());
    s.update(&[kind]);
    s.update(signer.0.transaction_id.as_byte_slice());
    s.update(&signer.0.index.to_le_bytes());
    s.update(&(payload.len() as u64).to_le_bytes());
    s.update(payload);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The message the envelope's signer signs** (RFC-0009 G-EXPIRY, G-RULESET): the network, the ruleset's `consensus_params_id`,
/// the last DAA at which the registration may be accepted, the signer and the wrapped registration's Borsh. A leaked signed bundle
/// therefore dies at `valid_until_daa`, and one signed for another ruleset is not valid on this one.
pub fn palw_signed_registration_message_v1(
    network_domain: Hash64,
    consensus_params_id: crate::Hash,
    valid_until_daa: u64,
    signer: &PalwBondKeyV2,
    registration_bytes: &[u8],
) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_SIGNED_REGISTRATION_MESSAGE_DOMAIN_V1).to_state();
    s.update(network_domain.as_byte_slice());
    s.update(consensus_params_id.as_bytes().as_slice());
    s.update(&valid_until_daa.to_le_bytes());
    s.update(signer.0.transaction_id.as_byte_slice());
    s.update(&signer.0.index.to_le_bytes());
    s.update(&(registration_bytes.len() as u64).to_le_bytes());
    s.update(registration_bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---- the envelope's fence (RFC-0009 G-EXPIRY / G-RULESET) ------------------------------------------------------------------

impl Params {
    /// Whether the signed-registration envelope (tag 108) may be carried at `daa_score` — never, in this binary.
    pub fn palw_signed_registration_active_at(&self, daa_score: u64) -> bool {
        self.palw_signed_registration_v1.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **The fence's refusal**: it ships beside the kernel route, which no network can arm, so any armed height is refused.
    pub fn validate_palw_signed_registration_v1(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_signed_registration_v1 {
            Some(f) if f != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_signed_registration_v1 cannot be armed: it is part of the kernel route's onboarding surface (G14, RFC-0014 §16), which no network can arm",
            )),
            _ => Ok(()),
        }
    }
}

// ---- rows ---------------------------------------------------------------------------------------------------------------

/// One artifact binding `(V2 class, kernel param root)`. Its state is a function of the DAA (see [`Self::state_at`]); only the
/// refutation and the release of the reservation are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ArtifactBindingRowV1 {
    pub binder: PalwBondKeyV2,
    pub bound_daa: u64,
    /// From this DAA the kernel route may attest the root.
    pub matures_daa: u64,
    /// From this DAA the binding can no longer be refuted and its reservation is released.
    pub final_daa: u64,
    /// What is reserved against the binder's bond now (0 once released or slashed).
    pub reserved: u64,
    pub refuted: bool,
}

/// Where a binding stands at a DAA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactBindingStateV1 {
    /// Within the challenge window: not attested yet.
    Pending,
    /// Attested to the kernel route; still refutable.
    Matured,
    /// Past the liability horizon: attested and no longer refutable.
    Final,
    Refuted,
}

impl ArtifactBindingRowV1 {
    pub fn state_at(&self, daa: u64) -> ArtifactBindingStateV1 {
        if self.refuted {
            ArtifactBindingStateV1::Refuted
        } else if daa < self.matures_daa {
            ArtifactBindingStateV1::Pending
        } else if daa < self.final_daa {
            ArtifactBindingStateV1::Matured
        } else {
            ArtifactBindingStateV1::Final
        }
    }
}

/// The link between a V2 class and its kernel class (`KernelBoundV1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelBindingRowV1 {
    pub kernel_class: Hash64,
    /// The artifact binding this link rests on (the kernel class's `param_commitments.root()`).
    pub kernel_param_root: Hash64,
    /// The kernel class's `VerificationPlanV1::root()`.
    pub plan_root: Hash64,
    /// The network's challenge policy id the class is bound to (equal to the route's policy).
    pub challenge_policy_id: Hash64,
    pub binder: PalwBondKeyV2,
    pub bound_daa: u64,
}

/// A conformance commitment accepted on chain for `(class, artifact root)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConformanceRowV1 {
    /// `ConformanceCommitmentV1::statement_root` (provenance excluded).
    pub statement_root: Hash64,
    pub signer: PalwBondKeyV2,
    pub committed_daa: u64,
}

/// What stops a class from leaving `Registered`, or that nothing does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwOnboardingGateV1 {
    /// The class has no onboarding row (no artifact binding, no kernel binding): the legacy path, unchanged.
    NotKernelBound,
    Ready,
    /// Kernel-bound but not yet eligible; `code` is the RFC-0011 §17 failure / state that names the wait.
    Held { code: &'static str, why: &'static str },
}

// ---- reads (the route's aux rows) -----------------------------------------------------------------------------------------

fn key2(a: &Hash64, b: &Hash64) -> Vec<u8> {
    borsh::to_vec(&(*a, *b)).expect("two digests serialize")
}

impl PalwKernelRouteStateV1 {
    /// The artifact binding of `(class, kernel root)`.
    pub fn artifact_binding_v1(&self, class: &Hash64, kernel_param_root: &Hash64) -> Option<ArtifactBindingRowV1> {
        self.aux_row(PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, &key2(class, kernel_param_root))
    }

    /// Every artifact binding of a V2 class, `(kernel root, row)`, in key order.
    pub fn artifact_bindings_of_v1(&self, class: &Hash64) -> Vec<(Hash64, ArtifactBindingRowV1)> {
        let lo = (PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, key2(class, &Hash64::from_bytes([0; 64])));
        let hi = (PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, key2(class, &Hash64::from_bytes([0xFF; 64])));
        self.aux
            .range(lo..=hi)
            .filter_map(|((_, key), row)| {
                let (_, root) = borsh::from_slice::<(Hash64, Hash64)>(key).ok()?;
                Some((root, borsh::from_slice::<ArtifactBindingRowV1>(row).ok()?))
            })
            .collect()
    }

    pub fn kernel_binding_v1(&self, class: &Hash64) -> Option<KernelBindingRowV1> {
        self.aux_row(PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, &borsh::to_vec(class).expect("a digest serializes"))
    }

    pub fn conformance_v1(&self, class: &Hash64, artifact_root: &Hash64) -> Option<ConformanceRowV1> {
        self.aux_row(PALW_ONBOARDING_TABLE_CONFORMANCE_V1, &key2(class, artifact_root))
    }

    /// **The artifact roots the kernel route may attest at `daa`**: the kernel root of every binding that is Matured or Final — never a
    /// Pending or refuted one. Derived from the rows, so every node reads the same list; this is the route's only attestation source.
    pub fn onboarding_attested_roots_v1(&self, daa: u64) -> Vec<Hash64> {
        let mut roots: Vec<Hash64> = self
            .aux
            .range((PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, Vec::new())..(PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1 + 1, Vec::new()))
            .filter_map(|((_, key), row)| {
                let (_, root) = borsh::from_slice::<(Hash64, Hash64)>(key).ok()?;
                let row = borsh::from_slice::<ArtifactBindingRowV1>(row).ok()?;
                matches!(row.state_at(daa), ArtifactBindingStateV1::Matured | ArtifactBindingStateV1::Final).then_some(root)
            })
            .collect();
        roots.sort();
        roots.dedup();
        roots
    }

    /// What the onboarding objects hold against `bond`'s collateral (summed over its bindings): the term V2's committed-collateral
    /// ledger and both withdrawal gates add beside the kernel ledger's own reservations.
    pub fn onboarding_reserved_v1(&self, bond: &PalwBondKeyV2) -> u64 {
        self.aux
            .range((PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, Vec::new())..(PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1 + 1, Vec::new()))
            .filter_map(|(_, row)| borsh::from_slice::<ArtifactBindingRowV1>(row).ok())
            .filter(|row| row.binder == *bond)
            .map(|row| row.reserved)
            .fold(0u64, |acc, r| acc.saturating_add(r))
    }

    /// Whether the route's ledger holds the kernel class (it registered only after `PUBLIC_PROSECUTION_COMPLETE`).
    pub fn kernel_class_record_v1(&self, kernel_class: &Hash64) -> Option<misaka_palw_kernel::state::ClassRecordV1> {
        let key = borsh::to_vec(&kernel_class.as_bytes()).expect("a digest serializes");
        let row = self.rows.get(&(misaka_palw_kernel::rows::TABLE_CLASSES_V1, key))?;
        borsh::from_slice(row).ok()
    }

    /// **The activation gate of a V2 class** (`activate_due_classes`): a class with no kernel binding follows the legacy path; a
    /// kernel-bound one is held until its artifact binding is Final (past the refutation horizon), the kernel class stands, and a
    /// conformance commitment is on chain for its `(class, artifact root)`.
    pub fn onboarding_gate_v1(&self, class: &Hash64, artifact_root: &Hash64, daa: u64) -> PalwOnboardingGateV1 {
        let Some(binding) = self.kernel_binding_v1(class) else {
            // A class with an artifact binding (live or refuted) has begun onboarding: it is held until it is kernel-bound. Only a class
            // with no onboarding row at all follows the legacy path.
            return if self.artifact_bindings_of_v1(class).is_empty() {
                PalwOnboardingGateV1::NotKernelBound
            } else {
                PalwOnboardingGateV1::Held {
                    code: "REGISTERED_DORMANT",
                    why: "the class has an artifact binding but no kernel binding (tag 106) yet",
                }
            };
        };
        if self.kernel_class_record_v1(&binding.kernel_class).is_none() {
            return PalwOnboardingGateV1::Held {
                code: "PUBLIC_PROSECUTION_INCOMPLETE",
                why: "the kernel class the V2 class is bound to is not registered in the route",
            };
        }
        match self.artifact_binding_v1(class, &binding.kernel_param_root).map(|row| row.state_at(daa)) {
            Some(ArtifactBindingStateV1::Final) => {}
            Some(ArtifactBindingStateV1::Refuted) | None => {
                return PalwOnboardingGateV1::Held { code: "AVAILABILITY_REQUIRED", why: "the artifact binding is refuted or missing" };
            }
            Some(_) => {
                return PalwOnboardingGateV1::Held {
                    code: "AVAILABILITY_REQUIRED",
                    why: "the artifact binding is inside its refutation horizon",
                };
            }
        }
        if self.conformance_v1(class, artifact_root).is_none() {
            return PalwOnboardingGateV1::Held {
                code: "REGISTERED_DORMANT",
                why: "no conformance commitment is on chain for this (class, artifact root)",
            };
        }
        PalwOnboardingGateV1::Ready
    }
}

// ---- the read model (RPC op 230) -------------------------------------------------------------------------------------------

/// **Where a V2 class stands on the onboarding path** — every row the route holds for it and the gate's verdict with its reason, so a
/// registrant, a verifier and an explorer read one answer. Nothing here is private: it is all chain state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingReadV1 {
    pub class_id: Hash64,
    /// The V2 class's status (`Registered { .. }`, `Active`, ...).
    pub status: String,
    pub artifact_root: Hash64,
    pub registrant: Option<PalwBondKeyV2>,
    /// `(kernel param root, row, state at the tip)`.
    pub artifact_bindings: Vec<(Hash64, ArtifactBindingRowV1, &'static str)>,
    pub kernel_binding: Option<KernelBindingRowV1>,
    pub conformance: Option<ConformanceRowV1>,
    pub gate: PalwOnboardingGateV1,
    /// The route's committed ledger root (the anchor for the rows).
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// **The onboarding read of a V2 class** at `daa` (`None`: no such class).
    pub fn onboarding_read_v1(&self, class: &Hash64, daa: u64) -> Option<OnboardingReadV1> {
        let state = self.class(class)?;
        let route = self.kernel_route();
        let bindings = route
            .map(|r| {
                r.artifact_bindings_of_v1(class)
                    .into_iter()
                    .map(|(root, row)| {
                        let name = match row.state_at(daa) {
                            ArtifactBindingStateV1::Pending => "Pending",
                            ArtifactBindingStateV1::Matured => "Matured",
                            ArtifactBindingStateV1::Final => "Final",
                            ArtifactBindingStateV1::Refuted => "Refuted",
                        };
                        (root, row, name)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(OnboardingReadV1 {
            class_id: *class,
            status: format!("{:?}", state.status),
            artifact_root: state.artifact_root,
            registrant: state.registrant_bond,
            artifact_bindings: bindings,
            kernel_binding: route.and_then(|r| r.kernel_binding_v1(class)),
            conformance: route.and_then(|r| r.conformance_v1(class, &state.artifact_root)),
            gate: route.map(|r| r.onboarding_gate_v1(class, &state.artifact_root, daa)).unwrap_or(PalwOnboardingGateV1::NotKernelBound),
            ledger_root: route.map(|r| r.ledger_root()).unwrap_or_default(),
            aux_root: route.map(|r| r.aux_root()).unwrap_or_default(),
        })
    }
}

// ---- the fraud proof ----------------------------------------------------------------------------------------------------------

/// **Proof that a bound kernel artifact is not the V2 class's artifact.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ArtifactMismatchProofV1 {
    /// The bound commitments' instance set is not the program's declared instance set (a tensor missing or surplus).
    Instances { commitments: ParamCommitmentsV1 },
    /// One tensor instance: the kernel's committed dtype / shape is not the declaration's, or its row bytes differ from the V2
    /// inventory's leaf over the same bytes.
    Row { commitments: ParamCommitmentsV1, param: u16, layer: Option<u16>, kernel_row: TensorOpeningV1, v2_opening: PalwArtifactOpeningV1 },
}

/// **Verify a refutation** against what the chain holds: the V2 class's program and registered `artifact_root`, and the bound kernel
/// root. `Ok(())` means the binding is FALSE (a fraud proven); `Err` says why the proof proves nothing. Pure and total: every path
/// is a `Result`, no allocation is proportional to an attacker-chosen shape beyond the opening's own bounded fields.
pub fn verify_artifact_mismatch_v1(
    program: &TirProgramV1,
    v2_artifact_root: Hash64,
    kernel_param_root: Hash64,
    proof: &ArtifactMismatchProofV1,
) -> Result<(), &'static str> {
    let commitments = match proof {
        ArtifactMismatchProofV1::Instances { commitments } | ArtifactMismatchProofV1::Row { commitments, .. } => commitments,
    };
    if commitments.root() != kernel_param_root.as_bytes() {
        return Err("the carried commitments do not root to the bound kernel root");
    }
    let declared: BTreeSet<(u16, Option<u16>)> = palw_tir_param_instances_v1(program)
        .into_iter()
        .enumerate()
        .flat_map(|(j, layers)| layers.into_iter().map(move |l| (j as u16, l)))
        .collect();
    match proof {
        ArtifactMismatchProofV1::Instances { commitments } => {
            let committed: BTreeSet<(u16, Option<u16>)> = commitments.by_instance.keys().copied().collect();
            if committed == declared { Err("the bound commitments hold exactly the program's declared instances") } else { Ok(()) }
        }
        ArtifactMismatchProofV1::Row { commitments, param, layer, kernel_row, v2_opening } => {
            if !declared.contains(&(*param, *layer)) {
                return Err("the program declares no such tensor instance");
            }
            let decl = program.params.get(*param as usize).ok_or("the program declares no such param")?;
            let Some(commitment) = commitments.by_instance.get(&(*param, *layer)) else {
                return Err("the bound commitments hold no such instance (use the Instances proof)");
            };
            if !kernel_row.authenticates(commitment) {
                return Err("the kernel opening does not authenticate against the bound commitment");
            }
            // The V2 opening is a leaf of the registered inventory, at the leaf the program's declarations put these bytes in.
            verify_artifact_opening_v1(v2_opening, v2_artifact_root).map_err(|_| "the V2 opening does not reach the registered artifact root")?;
            let op = &v2_opening.operand;
            if op.tensor_name != decl.name || op.layer != *layer {
                return Err("the V2 opening names another tensor instance");
            }
            if Some(v2_opening.leaf_index) != palw_tir_leaf_index_v1(program, *param, *layer, op.row_start as u64)
                || palw_tir_inventory_leaf_count_v1(program).ok() != Some(v2_opening.leaf_count)
            {
                return Err("the V2 opening is not at the leaf the program's layout puts these bytes in");
            }
            // (a) the kernel's committed tensor is not the declared one.
            let declared_shape: Vec<u64> = decl.shape.iter().map(|d| *d as u64).collect();
            if kernel_row.dtype != decl.dtype.tag() || kernel_row.shape != declared_shape {
                return Ok(());
            }
            // (b) the same bytes, two answers: compare the overlap of the kernel row's byte range with the V2 leaf's.
            if kernel_row.axis != AXIS_ROW {
                return Err("only a row opening of the kernel tensor is compared");
            }
            let shape = kernel_row.shape_usize().ok_or("the kernel opening's shape is not addressable")?;
            let layout = LayoutV1::try_of(&shape).ok_or("the kernel opening's shape overflows")?;
            let width = decl.dtype.width() as u64;
            let row_start = kernel_row
                .index
                .checked_mul(layout.row_len)
                .and_then(|e| e.checked_mul(width))
                .ok_or("the kernel row's byte offset overflows")?;
            let row_end = row_start.checked_add(kernel_row.values.len() as u64 * width).ok_or("the kernel row's byte range overflows")?;
            let (leaf_start, leaf_end) = (op.row_start as u64, (op.row_start as u64).saturating_add(op.bytes.len() as u64));
            let (lo, hi) = (row_start.max(leaf_start), row_end.min(leaf_end));
            if lo >= hi {
                return Err("the two openings cover no common byte");
            }
            for at in lo..hi {
                let kernel_byte = {
                    let element = ((at - row_start) / width) as usize;
                    let within = ((at - row_start) % width) as usize;
                    kernel_row.values[element].to_le_bytes()[within]
                };
                if op.bytes[(at - leaf_start) as usize] != kernel_byte {
                    return Ok(());
                }
            }
            Err("the two openings agree on every byte they share")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The object tags are the lead's allocation (104–108; 109 stays unallocated) and are declared, not inferred.
    #[test]
    fn the_onboarding_object_tags_are_the_allocated_ones() {
        use crate::palw_state_v2::PalwConsensusObjectV2 as O;
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(1), 0));
        let h = Hash64::from_bytes([1; 64]);
        let commitment = misaka_palw_challenge::ConformanceCommitmentV1 {
            version: 1,
            chain_genesis: [0; 64],
            ruleset_id: [0; 64],
            subject_kind: misaka_palw_challenge::SubjectKindV1::ModelConformance,
            candidate_id: [0; 64],
            kernel_descriptor_id: [0; 64],
            challenge_policy_id: [0; 64],
            artifact_root: [0; 64],
            program_root: [0; 64],
            source_root: misaka_palw_challenge::RootV1::Absent,
            tokenizer_or_input_schema_root: misaka_palw_challenge::RootV1::Absent,
            layout_root: [0; 64],
            verification_plan_root: [0; 64],
            constraint_root: misaka_palw_challenge::RootV1::Absent,
            implementation_set_root: [0; 64],
            test_scope_root: [0; 64],
            calibration_id: misaka_palw_challenge::RootV1::Absent,
            input_and_state_binding_root: misaka_palw_challenge::RootV1::Absent,
            resource_profile_id: [0; 64],
            commitment_object_id: None,
            canonical_commitment_position: None,
        };
        let bound = O::ArtifactBoundV1 { v2_class: h, kernel_param_root: h, signer: bond, signature: vec![1] };
        let objects = [
            (bound.clone(), 104u8),
            (
                O::ArtifactBindingChallengedV1 {
                    v2_class: h,
                    kernel_param_root: h,
                    challenger: bond,
                    proof: Box::new(ArtifactMismatchProofV1::Instances { commitments: ParamCommitmentsV1::default() }),
                    signature: vec![1],
                },
                105,
            ),
            (O::KernelBoundV1 { v2_class: h, kernel_class: h, challenge_policy_id: h, signer: bond, signature: vec![1] }, 106),
            (O::ConformanceCommittedV1 { commitment: Box::new(commitment), signer: bond, signature: vec![1] }, 107),
            (
                O::SignedRegistrationV1 {
                    registration: Box::new(bound),
                    valid_until_daa: 9,
                    consensus_params_id: crate::Hash::from_bytes([2; 32]),
                    signer: bond,
                    signature: vec![1],
                },
                108,
            ),
        ];
        for (object, tag) in objects {
            assert_eq!(borsh::to_vec(&object).unwrap()[0], tag, "{object:?}");
            assert_eq!(borsh::from_slice::<O>(&borsh::to_vec(&object).unwrap()).unwrap(), object, "round trip, tag {tag}");
            assert_eq!(crate::palw_state_v2::palw_object_is_onboarding_v1(&object), (104..=107).contains(&tag));
            assert_eq!(crate::palw_state_v2::palw_object_is_signed_registration_v1(&object), tag == 108);
        }
    }

    #[test]
    fn the_tables_are_the_allocated_ones_and_a_binding_reads_its_state_from_the_clock() {
        assert_eq!(
            (
                PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1,
                PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1,
                PALW_ONBOARDING_TABLE_CONFORMANCE_V1
            ),
            (36, 37, 38)
        );
        let row = ArtifactBindingRowV1 {
            binder: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(1), 0)),
            bound_daa: 10,
            matures_daa: 50,
            final_daa: 210,
            reserved: 7,
            refuted: false,
        };
        assert_eq!(row.state_at(49), ArtifactBindingStateV1::Pending);
        assert_eq!(row.state_at(50), ArtifactBindingStateV1::Matured);
        assert_eq!(row.state_at(209), ArtifactBindingStateV1::Matured);
        assert_eq!(row.state_at(210), ArtifactBindingStateV1::Final);
        assert_eq!(ArtifactBindingRowV1 { refuted: true, ..row }.state_at(300), ArtifactBindingStateV1::Refuted);
    }
}
