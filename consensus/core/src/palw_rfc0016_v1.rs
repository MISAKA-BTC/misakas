//! **RFC-0016 — bond-weighted physical block opportunity (V+) and elastic Final capacity** (dormant; `palw_model_block_rights_v1`,
//! `palw_elastic_final_capacity_v1`). Implemented dormant, **not activated**: V+ is the adopted DESIGN (user decision 2026-10-15),
//! and every value below is a placeholder until a measured-value record is pinned ([`values::PALW_RFC0016_MEASURED_RECORDS_V1`],
//! empty). Both fences are `None` on every preset, hashed Some-only, collapsed from `Some(never())`, and refused when armed.
//!
//! * [`values`] — the named fenced constants (σ, k, φ, pace margin, entrant reserve, capacity and queue terms, verifier-pay slope), the
//!   measured-value record gate, the V2 mirror and the fences' refusals.
//! * [`rights`] — V+: per-epoch pre-issued model block rights `⌊P_m · (N_exp − Ent)⌋` plus the entrant reserve (deducted from the
//!   fixed total), the admission rule (own right, or a σ-capped reissued one past the checkpoint φ, never past the epoch total), the
//!   pre-Attempt rights quote, the optional bounded ticket tilt `k` (pure; not wired: `k = 1`), the B+ comparison `ψ`, and the narrow
//!   interfaces to ADR-0177's allocation ([`rights::PalwRightsWeightsSourceV1`]) and RFC-0017's eligibility
//!   ([`rights::PalwModelEligibilitySourceV1`]).
//! * [`capacity`] — per-verifier water-filled capacity (each verifier counted once, no cross-model subtraction), the derived long-window
//!   (C7) concurrency with its measured ceiling, the OPV check budget, the bounded deadline-preserving Final queue, the combined
//!   pre-lottery admission quote, and the verifier-share slope hook (M\*-49).
//! * [`report`] — the per-model report rows (targets apart from outcomes) and the outcome rule: an improvement is claimed only from
//!   physical blocks or payout, never from a target share alone.
//!
//! The fold side (reading `PalwChainStateV2`) is `palw_state_v2::palw_rfc0016_fold_v1` (a child module of the state machine).

pub mod capacity;
pub mod report;
pub mod rights;
pub mod values;

pub use values::{PalwRfc0016MirrorV1, PalwRfc0016ValuesV1};
