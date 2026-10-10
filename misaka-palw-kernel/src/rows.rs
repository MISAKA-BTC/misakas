//! **The ledger as rows: what a consensus consumer stores per block, applies and reverts, carries in a snapshot, and roots.**
//!
//! An ADDITIVE view of [`KernelLedgerV1`] (the frozen per-object API is untouched). Every collection of the ledger is a table of
//! `(key bytes → row bytes)` — the very Borsh bytes [`crate::state`]'s root hashes — so
//!
//! * a consumer keeps the rows (a `BTreeMap<(table, key), bytes>`), journals a block as the rows it changed ([`diff_rows`]), applies
//!   and reverts that journal without decoding anything, and carries the map as its snapshot;
//! * the ledger itself is rebuilt on demand from the rows ([`KernelLedgerV1::from_rows`]: the derived caches of a class — the
//!   decoded program, the descriptor and the gate's bounds — are pure functions of its registration record and the policy);
//! * the root is computed from the rows alone ([`root_of_rows`]) and **equals [`KernelLedgerV1::root`]** of the ledger they
//!   describe, so a fresh verifier that rebuilds the ledger from the rows a node serves checks them against the root the chain
//!   committed.
//!
//! The scalar part of the state — the clock and the burn checksum — is [`LedgerScalarsV1`]; the policy, the schedule and the
//! implemented descriptors are configuration (the consumer's consensus constants), not rows.

use std::collections::BTreeMap;

use borsh::BorshSerialize;
use misaka_palw_tir::pipeline::{TirPipelineV1, stream_stage};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::program_v2::TirProgramV2;

use crate::descriptor::{KernelDescriptorV1, KernelScheduleV1};
use crate::gate::{public_pipeline_prosecution_complete_v1, public_prosecution_complete_v1};
use crate::hash::{Digest, finish, id, keyed, object_id};
use crate::ledger::{BondRowV1, ClaimRowV1, ClassRowV1, DemandKeyV1, DemandRowV1, KernelLedgerV1, LedgerPolicyV1, PipelineClassRowV1};
use crate::opv::{OPV_POLICY_DOMAIN_V1, OPV_STATE_VERSION_V2, OpvClaimRowV1, OpvPolicyV1, StateRootPartsV2};
use crate::pipeline::pipeline_root_v1;
use crate::pipeline_public::PipelineClassV1;
use crate::public::{ProfileMaterialV1, ServedPositionV1};
use crate::state::{
    ClassRecordV1, LEDGER_HEADER_DOMAIN_V1, LEDGER_LEAF_DOMAIN_V1, LEDGER_ROOT_DOMAIN_V1, LEDGER_STATE_VERSION_V1, LedgerHeaderV1,
    PipelineClassRecordV1, StateRootPartsV1,
};

/// The tables, in the order of [`StateRootPartsV1`]. Declared numbers: never renumbered.
pub const TABLE_BONDS_V1: u8 = 1;
pub const TABLE_CLASSES_V1: u8 = 2;
pub const TABLE_PIPELINE_CLASSES_V1: u8 = 3;
pub const TABLE_JOBS_V1: u8 = 4;
pub const TABLE_PIPELINE_JOBS_V1: u8 = 5;
pub const TABLE_CLAIMS_V1: u8 = 6;
pub const TABLE_DEMANDS_V1: u8 = 7;
pub const TABLE_SERVED_V1: u8 = 8;
pub const TABLE_ATTESTED_V1: u8 = 9;
/// `job → the claim holding it` (one claim per job; in the state root since the C4 fix F-C4-03).
pub const TABLE_JOB_CLAIMS_V1: u8 = 10;
/// `(job, producer) → the producer's unrevealed claim seal` (seal-then-reveal; in the state root since the C4 fix F-C4-06).
pub const TABLE_SEALS_V1: u8 = 11;
/// RFC-0015 (OPV): the class ids the network's policy admitted, the ids registered under the mode, and the OPV claims' fixed facts.
/// Absent (no rows) on a ledger with no OPV policy. The policy itself is configuration (the consumer's header), like the ledger's.
pub const TABLE_OPV_ADMITTED_V1: u8 = 12;
pub const TABLE_OPV_CLASSES_V1: u8 = 13;
pub const TABLE_OPV_CLAIMS_V1: u8 = 14;
/// GAP-R7: `(claim, accuser) → the accuser's unrevealed proof seal` (in the state root since the G14-R4 fix).
pub const TABLE_PROOF_SEALS_V1: u8 = 15;
/// GAP-5: `job → its poster's escrow` (the Final reward's funding; in the state root since the G14-R4 fix).
pub const TABLE_JOB_ESCROWS_V1: u8 = 16;
/// `(claim, stage, position) → the demand bonds of a served position awaiting their fate` (in the state root since G14-R4).
pub const TABLE_SERVED_DEMAND_BONDS_V1: u8 = 17;
/// C4R4 F-C4R4-08: `job → the bond that posted it`, written past `palw_panel_free_v1` and kept (in the beacon-seal extension). 19 is
/// G14-R4's (unused), 20–21 K2S's.
pub const TABLE_JOB_POSTERS_V1: u8 = 18;
/// RFC-0004 Part II: `class → ComputationSpecV1` (typed classes), `job → SpecJobV1`, `memory class → MemoryLineV1`. Each table is in
/// the root only when non-empty ([`crate::spec::typed_root_v1`]), so every ledger without a typed row roots as before.
pub const TABLE_SPEC_CLASSES_V1: u8 = 22;
pub const TABLE_SPEC_JOBS_V1: u8 = 23;
pub const TABLE_MEMORY_LINES_V1: u8 = 24;
/// The typed tables, in root order.
pub const TYPED_TABLES_V1: [u8; 3] = [TABLE_SPEC_CLASSES_V1, TABLE_SPEC_JOBS_V1, TABLE_MEMORY_LINES_V1];
/// OPV-BOOT GAP-B1a: `claim id → the salt of its seal` (claim seal v2), written past `palw_panel_free_v1`.
pub const TABLE_CLAIM_BEACON_SALTS_V1: u8 = 25;
/// OPV-BOOT GAP-B1a: `(job, producer, sealed_daa) → a claim seal that expired unrevealed past `palw_panel_free_v1``.
pub const TABLE_FORFEITED_CLAIM_SEALS_V1: u8 = 26;
/// The domain of the root extension tables 25, 26 and 18 add (absent while all are empty: every older root is unchanged).
pub const BEACON_SEAL_ROOT_EXTENSION_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/ledger-beacon-seal-extension/v1";
/// **Table 27** (`palw_verifier_pay_v1`, G14R round 3): check-fee escrows, claim draws and held default shares, key `(kind, digest)`.
pub const TABLE_VERIFIER_PAY_V1: u8 = 27;
/// The root extension table 27 adds once it holds a row (every older root unchanged).
pub const VERIFIER_PAY_ROOT_EXTENSION_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/ledger-verifier-pay-extension/v1";

/// `(table, borsh(key)) → borsh(row)`.
pub type RowKeyV1 = (u8, Vec<u8>);
pub type LedgerRowsV1 = BTreeMap<RowKeyV1, Vec<u8>>;

/// The ledger's two scalars: the clock and the burn checksum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, borsh::BorshDeserialize)]
pub struct LedgerScalarsV1 {
    pub daa: u64,
    pub burned: u64,
}

fn bytes_of<T: BorshSerialize>(v: &T) -> Vec<u8> {
    borsh::to_vec(v).expect("in-memory borsh")
}

fn collection_domain(table: u8) -> Vec<u8> {
    let name = match table {
        TABLE_BONDS_V1 => "bonds",
        TABLE_CLASSES_V1 => "classes",
        TABLE_PIPELINE_CLASSES_V1 => "pipeline-classes",
        TABLE_JOBS_V1 => "jobs",
        TABLE_PIPELINE_JOBS_V1 => "pipeline-jobs",
        TABLE_CLAIMS_V1 => "claims",
        TABLE_DEMANDS_V1 => "demands",
        TABLE_SERVED_V1 => "served",
        TABLE_JOB_CLAIMS_V1 => "job-claims",
        TABLE_SEALS_V1 => "seals",
        TABLE_OPV_ADMITTED_V1 => "opv-admitted",
        TABLE_OPV_CLASSES_V1 => "opv-classes",
        TABLE_OPV_CLAIMS_V1 => "opv-claims",
        TABLE_PROOF_SEALS_V1 => "proof-seals",
        TABLE_JOB_ESCROWS_V1 => "job-escrows",
        TABLE_SERVED_DEMAND_BONDS_V1 => "served-demand-bonds",
        TABLE_CLAIM_BEACON_SALTS_V1 => "claim-beacon-salts",
        TABLE_JOB_POSTERS_V1 => "job-posters",
        TABLE_FORFEITED_CLAIM_SEALS_V1 => "forfeited-claim-seals",
        TABLE_VERIFIER_PAY_V1 => "verifier-pay",
        TABLE_SPEC_CLASSES_V1 => "spec-classes",
        TABLE_SPEC_JOBS_V1 => "spec-jobs",
        TABLE_MEMORY_LINES_V1 => "memory-lines",
        _ => "attested-artifacts",
    };
    format!("misaka-palw/kernel/ledger-collection/{name}/v1").into_bytes()
}

impl KernelLedgerV1 {
    /// The ledger's scalars.
    pub fn scalars(&self) -> LedgerScalarsV1 {
        LedgerScalarsV1 { daa: self.daa, burned: self.burned }
    }

    /// **Every row of every table**, as the bytes the root hashes.
    pub fn to_rows(&self) -> LedgerRowsV1 {
        let mut rows = LedgerRowsV1::new();
        for (k, v) in &self.bonds {
            rows.insert((TABLE_BONDS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, c) in &self.classes {
            rows.insert((TABLE_CLASSES_V1, bytes_of(k)), bytes_of(&c.record()));
        }
        for (k, c) in &self.pipeline_classes {
            rows.insert((TABLE_PIPELINE_CLASSES_V1, bytes_of(k)), bytes_of(&c.record()));
        }
        for (k, v) in &self.jobs {
            rows.insert((TABLE_JOBS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.pipeline_jobs {
            rows.insert((TABLE_PIPELINE_JOBS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.claims {
            rows.insert((TABLE_CLAIMS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.demands {
            rows.insert((TABLE_DEMANDS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.served {
            rows.insert((TABLE_SERVED_V1, bytes_of(k)), bytes_of(v));
        }
        for k in &self.attested_artifacts {
            rows.insert((TABLE_ATTESTED_V1, bytes_of(k)), Vec::new());
        }
        for (k, v) in &self.job_claims {
            rows.insert((TABLE_JOB_CLAIMS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.seals {
            rows.insert((TABLE_SEALS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.proof_seals {
            rows.insert((TABLE_PROOF_SEALS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.job_escrows {
            rows.insert((TABLE_JOB_ESCROWS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.served_demands {
            rows.insert((TABLE_SERVED_DEMAND_BONDS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.claim_beacon_salts {
            rows.insert((TABLE_CLAIM_BEACON_SALTS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.job_posters {
            rows.insert((TABLE_JOB_POSTERS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.forfeited_claim_seals {
            rows.insert((TABLE_FORFEITED_CLAIM_SEALS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.verifier_pay {
            rows.insert((TABLE_VERIFIER_PAY_V1, bytes_of(k)), bytes_of(v));
        }
        for k in &self.opv.admitted {
            rows.insert((TABLE_OPV_ADMITTED_V1, bytes_of(k)), Vec::new());
        }
        for k in &self.opv.classes {
            rows.insert((TABLE_OPV_CLASSES_V1, bytes_of(k)), Vec::new());
        }
        for (k, v) in &self.opv.claims {
            rows.insert((TABLE_OPV_CLAIMS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, c) in &self.typed.classes {
            rows.insert((TABLE_SPEC_CLASSES_V1, bytes_of(k)), bytes_of(&c.spec));
        }
        for (k, v) in &self.typed.jobs {
            rows.insert((TABLE_SPEC_JOBS_V1, bytes_of(k)), bytes_of(v));
        }
        for (k, v) in &self.typed.lines {
            rows.insert((TABLE_MEMORY_LINES_V1, bytes_of(k)), bytes_of(v));
        }
        rows
    }

    /// **Rebuild a ledger from rows** over the same configuration (`template` supplies the policy, the schedule and the
    /// implemented descriptors; its own rows are ignored). A row that does not decode, or a class record that does not rebuild,
    /// is an error — a consumer's stored rows are its own consensus state, so that is corruption, not an input.
    pub fn from_rows(template: &KernelLedgerV1, scalars: LedgerScalarsV1, rows: &LedgerRowsV1) -> Result<Self, String> {
        let mut l = KernelLedgerV1::genesis(template.policy, template.schedule.clone(), template.known.clone())?;
        // The OPV policy is configuration (a genesis constant): the template carries it, exactly as it carries the ledger's own.
        if let Some(p) = template.opv.policy {
            l = l.with_opv_policy(p)?;
        }
        l.daa = scalars.daa;
        l.burned = scalars.burned;
        fn dec<T: borsh::BorshDeserialize>(b: &[u8], what: &str) -> Result<T, String> {
            borsh::from_slice(b).map_err(|e| format!("a stored {what} does not decode: {e}"))
        }
        let mut specs: Vec<(Digest, crate::spec::ComputationSpecV1)> = Vec::new();
        for ((table, key), row) in rows {
            match *table {
                TABLE_BONDS_V1 => {
                    l.bonds.insert(dec(key, "bond key")?, dec::<BondRowV1>(row, "bond")?);
                }
                TABLE_CLASSES_V1 => {
                    let record: ClassRecordV1 = dec(row, "class")?;
                    l.classes.insert(dec(key, "class key")?, class_row_of(&l, &record)?);
                }
                TABLE_PIPELINE_CLASSES_V1 => {
                    let record: PipelineClassRecordV1 = dec(row, "pipeline class")?;
                    l.pipeline_classes.insert(dec(key, "class key")?, pipeline_class_row_of(&l, &record)?);
                }
                TABLE_JOBS_V1 => {
                    l.jobs.insert(dec(key, "job key")?, dec(row, "job")?);
                }
                TABLE_PIPELINE_JOBS_V1 => {
                    l.pipeline_jobs.insert(dec(key, "job key")?, dec(row, "pipeline job")?);
                }
                TABLE_CLAIMS_V1 => {
                    l.claims.insert(dec(key, "claim key")?, dec::<ClaimRowV1>(row, "claim")?);
                }
                TABLE_DEMANDS_V1 => {
                    l.demands.insert(dec::<DemandKeyV1>(key, "demand key")?, dec::<DemandRowV1>(row, "demand")?);
                }
                TABLE_SERVED_V1 => {
                    l.served.insert(dec::<DemandKeyV1>(key, "served key")?, dec::<ServedPositionV1>(row, "served position")?);
                }
                TABLE_ATTESTED_V1 => {
                    l.attested_artifacts.insert(dec(key, "artifact root")?);
                }
                TABLE_JOB_CLAIMS_V1 => {
                    l.job_claims.insert(dec(key, "job id")?, dec(row, "job claim")?);
                }
                TABLE_SEALS_V1 => {
                    l.seals.insert(dec(key, "seal key")?, dec(row, "seal")?);
                }
                TABLE_PROOF_SEALS_V1 => {
                    l.proof_seals.insert(dec(key, "proof seal key")?, dec(row, "proof seal")?);
                }
                TABLE_SERVED_DEMAND_BONDS_V1 => {
                    l.served_demands.insert(dec::<DemandKeyV1>(key, "served demand key")?, dec(row, "served demand bonds")?);
                }
                TABLE_CLAIM_BEACON_SALTS_V1 => {
                    l.claim_beacon_salts.insert(dec(key, "claim id")?, dec(row, "claim beacon salt")?);
                }
                TABLE_JOB_POSTERS_V1 => {
                    l.job_posters.insert(dec(key, "job id")?, dec(row, "job poster")?);
                }
                TABLE_FORFEITED_CLAIM_SEALS_V1 => {
                    l.forfeited_claim_seals.insert(
                        dec::<(Digest, Digest, u64)>(key, "forfeited seal key")?,
                        dec::<crate::ledger::ForfeitedSealRowV1>(row, "forfeited seal")?,
                    );
                }
                TABLE_VERIFIER_PAY_V1 => {
                    l.verifier_pay.insert(
                        dec::<(u8, Digest)>(key, "verifier pay key")?,
                        dec::<crate::verifier_pay::VerifierPayRowV1>(row, "verifier pay row")?,
                    );
                }
                TABLE_JOB_ESCROWS_V1 => {
                    l.job_escrows.insert(dec(key, "job escrow key")?, dec::<crate::ledger::JobEscrowRowV1>(row, "job escrow")?);
                }
                TABLE_OPV_ADMITTED_V1 => {
                    l.opv.admitted.insert(dec(key, "admitted class")?);
                }
                TABLE_OPV_CLASSES_V1 => {
                    l.opv.classes.insert(dec(key, "opv class")?);
                }
                TABLE_OPV_CLAIMS_V1 => {
                    l.opv.claims.insert(dec(key, "opv claim key")?, dec::<OpvClaimRowV1>(row, "opv claim")?);
                }
                TABLE_SPEC_CLASSES_V1 => specs.push((dec(key, "spec class key")?, dec(row, "spec class")?)),
                TABLE_SPEC_JOBS_V1 => {
                    l.typed.jobs.insert(dec(key, "spec job key")?, dec(row, "spec job")?);
                }
                TABLE_MEMORY_LINES_V1 => {
                    l.typed.lines.insert(dec(key, "memory line key")?, dec(row, "memory line")?);
                }
                other => return Err(format!("a stored row names no table ({other})")),
            }
        }
        // A composite's derived row reads its components': every other typed class first (a component never is a composite).
        specs.sort_by_key(|(_, spec)| matches!(spec.roots.first(), Some(crate::spec::TypedRootV1::CompositeV1(_))));
        for (id, spec) in specs {
            let row = l.spec_class_row_of(&spec, false).map_err(|e| format!("a stored typed class does not rebuild: {e}"))?;
            l.typed.classes.insert(id, row);
        }
        // The live-claim index is derived (not committed): rebuilt from the rows, exactly as a restored ledger does.
        l.opv_rebuild_live();
        Ok(l)
    }
}

/// A single-program class's derived row from its record: decode the program, resolve the descriptor and recompute the gate's bounds
/// (all pure functions of the record and the policy — what registration computed).
fn class_row_of(l: &KernelLedgerV1, r: &ClassRecordV1) -> Result<ClassRowV1, String> {
    let d: KernelDescriptorV1 = l
        .known
        .iter()
        .find(|d| d.digest() == r.descriptor)
        .cloned()
        .ok_or_else(|| "a stored class names a kernel this binary does not implement".to_string())?;
    let program = TirProgramV1::decode_canonical(&r.program_bytes).map_err(|e| format!("a stored class's program: {e}"))?;
    let nodes: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
    let bounds = public_prosecution_complete_v1(&d, &r.plan, nodes, &ProfileMaterialV1::kernel_route(true), &l.policy.prosecution)
        .map_err(|g| format!("a stored class is no longer publicly prosecutable: {g:?}"))?;
    Ok(ClassRowV1 {
        descriptor: d,
        program_bytes: r.program_bytes.clone(),
        program,
        plan: r.plan.clone(),
        param_commitments: r.param_commitments.clone(),
        network_domain: l.policy.network_domain,
        ruleset_digest: l.policy.ruleset_digest,
        bounds,
    })
}

fn pipeline_class_row_of(l: &KernelLedgerV1, r: &PipelineClassRecordV1) -> Result<PipelineClassRowV1, String> {
    let d: KernelDescriptorV1 = l
        .known
        .iter()
        .find(|d| d.digest() == r.descriptor)
        .cloned()
        .ok_or_else(|| "a stored class names a kernel this binary does not implement".to_string())?;
    let programs = r
        .program_bytes
        .iter()
        .map(|b| TirProgramV2::decode_canonical(b).map_err(|e| format!("a stored pipeline's program: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    let pipeline = TirPipelineV1::decode_canonical(&r.pipeline_bytes, &programs).map_err(|e| format!("a stored pipeline: {e}"))?;
    let bounds = public_pipeline_prosecution_complete_v1(
        &d,
        &r.plan,
        &pipeline,
        &programs,
        &ProfileMaterialV1::kernel_route(true),
        &l.policy.prosecution,
    )
    .map_err(|g| format!("a stored pipeline class is no longer publicly prosecutable: {g:?}"))?;
    let binding = PipelineClassV1 {
        descriptor_digest: d.digest(),
        pipeline_root: pipeline_root_v1(&pipeline, &programs),
        plan_root: r.plan.root(),
        artifact_roots: r.param_commitments.iter().map(|p| p.root()).collect(),
        decode: r.decode,
    };
    let _ = stream_stage(&pipeline);
    Ok(PipelineClassRowV1 {
        descriptor: d,
        pipeline_bytes: r.pipeline_bytes.clone(),
        pipeline,
        program_bytes: r.program_bytes.clone(),
        programs,
        plan: r.plan.clone(),
        param_commitments: r.param_commitments.clone(),
        binding,
        network_domain: l.policy.network_domain,
        ruleset_digest: l.policy.ruleset_digest,
        bounds,
    })
}

/// The decoded sort key of a row: the ledger orders `BTreeMap<DemandKeyV1, _>` by `(claim, stage, position)` NUMERICALLY, which
/// is not the order of the Borsh bytes (the position is little-endian). Every other table's key is a 64-byte digest, whose byte
/// order is its order.
fn sort_key(table: u8, key: &[u8]) -> (Vec<u8>, u8, u32) {
    // `(job, producer, sealed_daa)`: the DAA is little-endian in its Borsh bytes, so it is ordered by its big-endian bytes.
    if table == TABLE_FORFEITED_CLAIM_SEALS_V1 && key.len() == 64 + 64 + 8 {
        let mut daa = [0u8; 8];
        daa.copy_from_slice(&key[128..136]);
        let mut k = key[..128].to_vec();
        k.extend_from_slice(&u64::from_le_bytes(daa).to_be_bytes());
        return (k, 0, 0);
    }
    if matches!(table, TABLE_DEMANDS_V1 | TABLE_SERVED_V1 | TABLE_SERVED_DEMAND_BONDS_V1) && key.len() == 64 + 1 + 4 {
        let stage = key[64];
        let mut p = [0u8; 4];
        p.copy_from_slice(&key[65..69]);
        (key[..64].to_vec(), stage, u32::from_le_bytes(p))
    } else {
        (key.to_vec(), 0, 0)
    }
}

/// **The state root computed from the rows alone** — equal to [`KernelLedgerV1::root`] of the ledger they describe (tested), so a
/// consumer roots its stored rows without rebuilding the ledger.
pub fn root_of_rows(
    policy: &LedgerPolicyV1,
    config_root: Digest,
    scalars: LedgerScalarsV1,
    opv: Option<&OpvPolicyV1>,
    rows: &LedgerRowsV1,
) -> Digest {
    let mut by_table: BTreeMap<u8, Vec<(&Vec<u8>, &Vec<u8>)>> = BTreeMap::new();
    for ((table, key), row) in rows {
        by_table.entry(*table).or_default().push((key, row));
    }
    let coll = |table: u8| -> Digest {
        let mut list = by_table.get(&table).cloned().unwrap_or_default();
        list.sort_by_key(|(k, _)| sort_key(table, k));
        let mut s = keyed(&collection_domain(table));
        s.update(&(list.len() as u64).to_le_bytes());
        for (k, v) in list {
            let mut leaf = (k.len() as u32).to_le_bytes().to_vec();
            leaf.extend_from_slice(k);
            leaf.extend_from_slice(v);
            s.update(&id(LEDGER_LEAF_DOMAIN_V1, &leaf));
        }
        finish(s)
    };
    let header =
        LedgerHeaderV1 { version: LEDGER_STATE_VERSION_V1, policy: *policy, config_root, daa: scalars.daa, burned: scalars.burned };
    let parts = StateRootPartsV1 {
        version: LEDGER_STATE_VERSION_V1,
        header: object_id(LEDGER_HEADER_DOMAIN_V1, &header),
        bonds: coll(TABLE_BONDS_V1),
        classes: coll(TABLE_CLASSES_V1),
        pipeline_classes: coll(TABLE_PIPELINE_CLASSES_V1),
        jobs: coll(TABLE_JOBS_V1),
        pipeline_jobs: coll(TABLE_PIPELINE_JOBS_V1),
        claims: coll(TABLE_CLAIMS_V1),
        demands: coll(TABLE_DEMANDS_V1),
        served: coll(TABLE_SERVED_V1),
        attested_artifacts: coll(TABLE_ATTESTED_V1),
        job_claims: coll(TABLE_JOB_CLAIMS_V1),
        seals: coll(TABLE_SEALS_V1),
        proof_seals: coll(TABLE_PROOF_SEALS_V1),
        job_escrows: coll(TABLE_JOB_ESCROWS_V1),
        served_demands: coll(TABLE_SERVED_DEMAND_BONDS_V1),
    };
    let _ = LEDGER_ROOT_DOMAIN_V1;
    let v1 = parts.root();
    // RFC-0004 Part II: the typed tables, each only when non-empty (an untyped ledger's root is unchanged).
    let typed: Vec<(u8, Digest)> =
        crate::rows::TYPED_TABLES_V1.iter().filter(|t| by_table.contains_key(*t)).map(|t| (*t, coll(*t))).collect();
    // RFC-0015: with no OPV policy the root is the historical one, byte for byte; with one it is the OPV root form.
    let base = match opv {
        None => v1,
        Some(p) => StateRootPartsV2 {
            version: OPV_STATE_VERSION_V2,
            v1,
            opv_policy: object_id(OPV_POLICY_DOMAIN_V1, &Some(*p)),
            opv_admitted: coll(TABLE_OPV_ADMITTED_V1),
            opv_classes: coll(TABLE_OPV_CLASSES_V1),
            opv_claims: coll(TABLE_OPV_CLAIMS_V1),
        }
        .root(),
    };
    // RFC-0004 Part II: the typed tables, each only when non-empty (an untyped ledger's root is unchanged).
    let base = crate::spec::typed_root_v1(base, &typed);
    // OPV-BOOT GAP-B1a / C4R4 F-C4R4-08: tables 25, 26 and 18 extend the root only once any of them holds a row.
    let base = if [TABLE_CLAIM_BEACON_SALTS_V1, TABLE_FORFEITED_CLAIM_SEALS_V1, TABLE_JOB_POSTERS_V1]
        .iter()
        .any(|t| by_table.contains_key(t))
    {
        beacon_seal_root_extension_v1(
            &base,
            &coll(TABLE_CLAIM_BEACON_SALTS_V1),
            &coll(TABLE_FORFEITED_CLAIM_SEALS_V1),
            &coll(TABLE_JOB_POSTERS_V1),
        )
    } else {
        base
    };
    // `palw_verifier_pay_v1`: table 27 extends the root only once it holds a row.
    if by_table.contains_key(&TABLE_VERIFIER_PAY_V1) {
        verifier_pay_root_extension_v1(&base, &coll(TABLE_VERIFIER_PAY_V1))
    } else {
        base
    }
}

/// `H(extension; root ‖ table 27)` (`palw_verifier_pay_v1`).
pub fn verifier_pay_root_extension_v1(base: &Digest, table: &Digest) -> Digest {
    let mut s = keyed(VERIFIER_PAY_ROOT_EXTENSION_DOMAIN_V1);
    s.update(base).update(table);
    finish(s)
}

/// `H(extension; base root ‖ claim beacon salts ‖ forfeited claim seals ‖ job posters)` (OPV-BOOT GAP-B1a, C4R4 F-C4R4-08).
pub fn beacon_seal_root_extension_v1(base: &Digest, salts: &Digest, forfeited: &Digest, posters: &Digest) -> Digest {
    let mut s = keyed(BEACON_SEAL_ROOT_EXTENSION_DOMAIN_V1);
    s.update(base).update(salts).update(forfeited).update(posters);
    finish(s)
}

/// One row's change between two row sets: `(key, old, new)`, in key order.
pub type RowChangeV1 = (RowKeyV1, Option<Vec<u8>>, Option<Vec<u8>>);

/// **The rows `after` changed from `before`** (written, rewritten or dropped), in key order — the block's journal.
pub fn diff_rows(before: &LedgerRowsV1, after: &LedgerRowsV1) -> Vec<RowChangeV1> {
    let mut out = Vec::new();
    for (k, new) in after {
        match before.get(k) {
            Some(old) if old == new => {}
            old => out.push((k.clone(), old.cloned(), Some(new.clone()))),
        }
    }
    for (k, old) in before {
        if !after.contains_key(k) {
            out.push((k.clone(), Some(old.clone()), None));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The schedule a ledger with this configuration reports (a convenience for consumers that keep the configuration separately).
pub fn config_root_of(schedule: &KernelScheduleV1, known: &[KernelDescriptorV1]) -> Digest {
    let mut entries = schedule.entries.clone();
    entries.sort_by_key(|(d, _)| *d);
    let mut known: Vec<Digest> = known.iter().map(|d| d.digest()).collect();
    known.sort();
    object_id(crate::state::LEDGER_CONFIG_DOMAIN_V1, &(entries, known))
}
