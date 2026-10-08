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
use crate::ledger::{ClaimRowV1, DemandKeyV1, DemandRowV1, KernelLedgerV1, LedgerPolicyV1, PipelineClassRowV1, ClassRowV1, BondRowV1};
use crate::pipeline::{pipeline_root_v1};
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
        rows
    }

    /// **Rebuild a ledger from rows** over the same configuration (`template` supplies the policy, the schedule and the
    /// implemented descriptors; its own rows are ignored). A row that does not decode, or a class record that does not rebuild,
    /// is an error — a consumer's stored rows are its own consensus state, so that is corruption, not an input.
    pub fn from_rows(template: &KernelLedgerV1, scalars: LedgerScalarsV1, rows: &LedgerRowsV1) -> Result<Self, String> {
        let mut l = KernelLedgerV1::genesis(template.policy, template.schedule.clone(), template.known.clone())?;
        l.daa = scalars.daa;
        l.burned = scalars.burned;
        fn dec<T: borsh::BorshDeserialize>(b: &[u8], what: &str) -> Result<T, String> {
            borsh::from_slice(b).map_err(|e| format!("a stored {what} does not decode: {e}"))
        }
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
                other => return Err(format!("a stored row names no table ({other})")),
            }
        }
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
    if matches!(table, TABLE_DEMANDS_V1 | TABLE_SERVED_V1) && key.len() == 64 + 1 + 4 {
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
pub fn root_of_rows(policy: &LedgerPolicyV1, config_root: Digest, scalars: LedgerScalarsV1, rows: &LedgerRowsV1) -> Digest {
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
    let header = LedgerHeaderV1 {
        version: LEDGER_STATE_VERSION_V1,
        policy: *policy,
        config_root,
        daa: scalars.daa,
        burned: scalars.burned,
    };
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
    };
    let _ = LEDGER_ROOT_DOMAIN_V1;
    parts.root()
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
