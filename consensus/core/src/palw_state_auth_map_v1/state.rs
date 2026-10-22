//! The state level: a directory of table roots (itself an [`AuthMapV1`]) under a header that chains the state to its parent state
//! root and to the branch it was built on. A row proof is then three openings — header, directory entry, table rows — and a
//! change set is the same three with the writes, from which the verifier computes the child state root itself.

use super::AuthMapErrorV1;
use super::bits::KeyRangeV1;
use super::proof::{AuthMapProofV1, RowUpdateV1, VerifiedRowsV1, verify_rows_v1, verify_update_v1};
use super::trie::{AuthMapV1, AuthTableSpecV1};
use crate::Hash64;
use blake2b_simd::Params;
use std::collections::BTreeMap;

const DOMAIN_STATE: &[u8] = b"misaka-palw/auth-state/v1/root";

/// The directory's own spec: table id `u32::MAX` (reserved), keys `table_id ‖ schema_version` big-endian, values a 64-byte root.
pub const AUTH_STATE_DIRECTORY_SPEC_V1: AuthTableSpecV1 = AuthTableSpecV1::new(u32::MAX, 1, 8, 64);
/// The format this module writes into [`AuthStateHeaderV1::format_version`].
pub const AUTH_STATE_FORMAT_V1: u32 = 1;

/// The directory key of a table: id then version, big-endian, so the directory lists tables in id order, versions together.
pub fn directory_key_v1(spec: &AuthTableSpecV1) -> Vec<u8> {
    let mut k = spec.table_id.to_be_bytes().to_vec();
    k.extend_from_slice(&spec.schema_version.to_be_bytes());
    k
}

/// **What a state root commits besides its tables**: the format, the parent state's root, the branch anchor (the selected parent
/// block the state was built on), and the directory root. Two branches that happen to hold equal tables still commit different
/// roots, so a row proven on one is never accepted on the other.
#[derive(Clone, Copy, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AuthStateHeaderV1 {
    pub format_version: u32,
    pub parent_state_root: Hash64,
    pub branch_anchor: Hash64,
    pub directory_root: Hash64,
}

impl AuthStateHeaderV1 {
    pub fn root(&self) -> Hash64 {
        let mut s = Params::new().hash_length(64).key(DOMAIN_STATE).to_state();
        s.update(&borsh::to_vec(self).expect("a header is borsh-serializable"));
        let mut out = [0u8; 64];
        out.copy_from_slice(s.finalize().as_bytes());
        Hash64::from_bytes(out)
    }
}

/// **The pin a reader holds**: the state root it reads (the parent state of the block being validated) and what that root must
/// chain to. A proof is checked against all of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AuthStatePinV1 {
    pub state_root: Hash64,
    pub parent_state_root: Hash64,
    pub branch_anchor: Hash64,
}

/// **Bounds a verifier applies before it hashes anything.**
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AuthProofLimitsV1 {
    /// Nodes of one table proof.
    pub max_table_nodes: u32,
    /// Nodes of the directory proof.
    pub max_directory_nodes: u32,
    /// Tables one change set may touch.
    pub max_tables: u32,
}

impl AuthProofLimitsV1 {
    /// Point-proof bounds of `spec` (and of the directory) for `tables` tables and `points` keys per table.
    pub const fn for_points(spec: &AuthTableSpecV1, points: u32, tables: u32) -> Self {
        Self {
            max_table_nodes: spec.point_proof_node_bound() * points,
            max_directory_nodes: AUTH_STATE_DIRECTORY_SPEC_V1.point_proof_node_bound() * tables,
            max_tables: tables,
        }
    }
}

/// **A proof of rows of one table in one state.**
#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AuthStateRowProofV1 {
    pub header: AuthStateHeaderV1,
    pub directory: AuthMapProofV1,
    pub table: AuthMapProofV1,
}

/// **Verify rows of `spec`'s table in the pinned state.** Refuses: a header that does not hash to the pinned root; a header that
/// chains to another parent or another branch; another format; a directory without this table (by id AND version); a table proof
/// that does not open the directory's root for it; every table-level failure of [`verify_rows_v1`].
pub fn verify_state_rows_v1(
    pin: &AuthStatePinV1,
    spec: &AuthTableSpecV1,
    proof: &AuthStateRowProofV1,
    ranges: &[KeyRangeV1],
    limits: &AuthProofLimitsV1,
) -> Result<VerifiedRowsV1, AuthMapErrorV1> {
    check_header(pin, &proof.header)?;
    let table_root = table_root_in_directory(&proof.header, spec, &proof.directory, limits)?.ok_or(AuthMapErrorV1::TableAbsent)?;
    verify_rows_v1(spec, &table_root, &proof.table, ranges, limits.max_table_nodes)
}

fn check_header(pin: &AuthStatePinV1, header: &AuthStateHeaderV1) -> Result<(), AuthMapErrorV1> {
    if header.root() != pin.state_root {
        return Err(AuthMapErrorV1::RootMismatch);
    }
    if header.format_version != AUTH_STATE_FORMAT_V1 {
        return Err(AuthMapErrorV1::WrongFormat);
    }
    if header.parent_state_root != pin.parent_state_root || header.branch_anchor != pin.branch_anchor {
        return Err(AuthMapErrorV1::OtherBranch);
    }
    Ok(())
}

fn table_root_in_directory(
    header: &AuthStateHeaderV1,
    spec: &AuthTableSpecV1,
    directory: &AuthMapProofV1,
    limits: &AuthProofLimitsV1,
) -> Result<Option<Hash64>, AuthMapErrorV1> {
    let key = directory_key_v1(spec);
    let dir = verify_rows_v1(
        &AUTH_STATE_DIRECTORY_SPEC_V1,
        &header.directory_root,
        directory,
        &[KeyRangeV1::point(&key)],
        limits.max_directory_nodes,
    )?;
    match dir.get(&key)? {
        None => Ok(None),
        Some(bytes) => {
            let arr: [u8; 64] = bytes.try_into().map_err(|_| AuthMapErrorV1::Malformed)?;
            Ok(Some(Hash64::from_bytes(arr)))
        }
    }
}

/// The writes to one table in a change set, with that table's update proof against the parent state.
#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AuthTableChangeV1 {
    pub spec: AuthTableSpecV1,
    pub updates: Vec<RowUpdateV1>,
    pub proof: AuthMapProofV1,
}

/// **A state change set**: the parent's header, the directory proof of every table touched, each table's writes and update proof,
/// and the child's branch anchor. [`verify_state_change_v1`] computes the child root from it — the only state the writer holds is
/// the parent's root and these rows.
#[derive(Clone, PartialEq, Eq, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AuthStateChangeSetV1 {
    pub parent_header: AuthStateHeaderV1,
    pub directory: AuthMapProofV1,
    pub tables: Vec<AuthTableChangeV1>,
    pub child_branch_anchor: Hash64,
}

/// **Verify a change set against the pinned parent and return the child state root.** A table the directory does not hold starts
/// empty (a first write creates it); a table is never dropped from the directory (an emptied table stays, with the empty root).
pub fn verify_state_change_v1(
    pin: &AuthStatePinV1,
    change: &AuthStateChangeSetV1,
    limits: &AuthProofLimitsV1,
) -> Result<Hash64, AuthMapErrorV1> {
    check_header(pin, &change.parent_header)?;
    if change.tables.len() > limits.max_tables as usize {
        return Err(AuthMapErrorV1::ProofTooLarge);
    }
    let keys: Vec<Vec<u8>> = change.tables.iter().map(|t| directory_key_v1(&t.spec)).collect();
    let ranges: Vec<_> = keys.iter().map(|k| KeyRangeV1::point(k)).collect();
    let dir = verify_rows_v1(
        &AUTH_STATE_DIRECTORY_SPEC_V1,
        &change.parent_header.directory_root,
        &change.directory,
        &ranges,
        limits.max_directory_nodes,
    )?;
    let mut dir_updates = Vec::with_capacity(change.tables.len());
    for (table, key) in change.tables.iter().zip(&keys) {
        table.spec.check()?;
        let old = dir.get(key)?.map(<[u8]>::to_vec);
        let old_root = match &old {
            Some(bytes) => Hash64::from_bytes(bytes.as_slice().try_into().map_err(|_| AuthMapErrorV1::Malformed)?),
            None => AuthMapV1::new(table.spec)?.root(),
        };
        let new_root = verify_update_v1(&table.spec, &old_root, &table.proof, &table.updates, limits.max_table_nodes)?;
        dir_updates.push(RowUpdateV1 { key: key.clone(), old, new: Some(new_root.as_bytes().to_vec()) });
    }
    // The directory update reuses the directory proof just verified (it covers every touched key).
    let directory_root = verify_update_v1(
        &AUTH_STATE_DIRECTORY_SPEC_V1,
        &change.parent_header.directory_root,
        &change.directory,
        &dir_updates,
        limits.max_directory_nodes,
    )?;
    Ok(AuthStateHeaderV1 {
        format_version: AUTH_STATE_FORMAT_V1,
        parent_state_root: pin.state_root,
        branch_anchor: change.child_branch_anchor,
        directory_root,
    }
    .root())
}

/// **The full (writer's) state**: every table, the directory, and the header fields. A full node builds roots and proofs from it; a
/// pruned node never needs it — it holds pins, verified rows and change sets.
#[derive(Clone, Debug)]
pub struct AuthStateV1 {
    parent_state_root: Hash64,
    branch_anchor: Hash64,
    tables: BTreeMap<Vec<u8>, AuthMapV1>,
    directory: AuthMapV1,
}

impl AuthStateV1 {
    pub fn new(parent_state_root: Hash64, branch_anchor: Hash64) -> Self {
        Self {
            parent_state_root,
            branch_anchor,
            tables: BTreeMap::new(),
            directory: AuthMapV1::new(AUTH_STATE_DIRECTORY_SPEC_V1).expect("the directory spec is in bounds"),
        }
    }

    pub fn header(&self) -> AuthStateHeaderV1 {
        AuthStateHeaderV1 {
            format_version: AUTH_STATE_FORMAT_V1,
            parent_state_root: self.parent_state_root,
            branch_anchor: self.branch_anchor,
            directory_root: self.directory.root(),
        }
    }

    pub fn root(&self) -> Hash64 {
        self.header().root()
    }

    /// The pin a reader of this state holds.
    pub fn pin(&self) -> AuthStatePinV1 {
        AuthStatePinV1 { state_root: self.root(), parent_state_root: self.parent_state_root, branch_anchor: self.branch_anchor }
    }

    pub fn table(&self, spec: &AuthTableSpecV1) -> Option<&AuthMapV1> {
        self.tables.get(&directory_key_v1(spec)).filter(|t| t.spec() == spec)
    }

    /// Write one row (creating the table on its first write) and refresh the directory entry.
    pub fn write(&mut self, spec: &AuthTableSpecV1, key: Vec<u8>, value: Option<Vec<u8>>) -> Result<Option<Vec<u8>>, AuthMapErrorV1> {
        spec.check()?;
        if key.len() > spec.max_key_len as usize {
            return Err(AuthMapErrorV1::KeyTooLong);
        }
        if value.as_ref().is_some_and(|v| v.len() > spec.max_value_len as usize) {
            return Err(AuthMapErrorV1::ValueTooLong);
        }
        let dkey = directory_key_v1(spec);
        if self.tables.get(&dkey).is_some_and(|t| t.spec() != spec) {
            return Err(AuthMapErrorV1::SpecOutOfBounds);
        }
        if !self.tables.contains_key(&dkey) {
            self.tables.insert(dkey.clone(), AuthMapV1::new(*spec)?);
        }
        let table = self.tables.get_mut(&dkey).expect("inserted above");
        let old = table.write(key, value)?;
        let root = table.root();
        self.directory.insert(dkey, root.as_bytes().to_vec())?;
        Ok(old)
    }

    pub fn prove_rows(&self, spec: &AuthTableSpecV1, ranges: &[KeyRangeV1]) -> AuthStateRowProofV1 {
        let dkey = directory_key_v1(spec);
        AuthStateRowProofV1 {
            header: self.header(),
            directory: self.directory.prove_keys(&[dkey]),
            table: self.table(spec).map_or_else(AuthMapProofV1::default, |t| t.prove(ranges)),
        }
    }

    /// **Make the child state**: the change set proving `writes` against this state, and the child itself (parent = this root).
    pub fn child(
        &self,
        child_branch_anchor: Hash64,
        writes: &[(AuthTableSpecV1, Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<(AuthStateChangeSetV1, AuthStateV1), AuthMapErrorV1> {
        // One write per key: the first read's old value, the last write's new value.
        let mut by_table: BTreeMap<Vec<u8>, (AuthTableSpecV1, BTreeMap<Vec<u8>, RowUpdateV1>)> = BTreeMap::new();
        for (spec, key, new) in writes {
            let rows = &mut by_table.entry(directory_key_v1(spec)).or_insert_with(|| (*spec, BTreeMap::new())).1;
            rows.entry(key.clone()).and_modify(|u| u.new = new.clone()).or_insert_with(|| RowUpdateV1 {
                key: key.clone(),
                old: self.table(spec).and_then(|t| t.get(key)).map(<[u8]>::to_vec),
                new: new.clone(),
            });
        }
        let dkeys: Vec<_> = by_table.keys().cloned().collect();
        let tables = by_table
            .values()
            .map(|(spec, rows)| {
                let keys: Vec<_> = rows.keys().cloned().collect();
                let proof = self.table(spec).map_or_else(AuthMapProofV1::default, |t| t.prove_keys(&keys));
                AuthTableChangeV1 { spec: *spec, updates: rows.values().cloned().collect(), proof }
            })
            .collect();
        let change = AuthStateChangeSetV1 {
            parent_header: self.header(),
            directory: self.directory.prove_keys(&dkeys),
            tables,
            child_branch_anchor,
        };
        let mut child = self.clone();
        child.parent_state_root = self.root();
        child.branch_anchor = child_branch_anchor;
        for (spec, key, new) in writes {
            child.write(spec, key.clone(), new.clone())?;
        }
        Ok((change, child))
    }
}
