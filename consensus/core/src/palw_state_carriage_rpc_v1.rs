//! **Op 243: the exact canonical carriage of the PALW state a pinned header commits, in bounded byte pages** — what
//! `palw-class panel-lifetime-v2 --carriage` restores offline (`PalwStateCarriageV2::into_state_v3` against a caller-pinned root).
//!
//! The served state is the one op 213 / op 242 serve: the post-state of the pinned block's SELECTED PARENT, refused unless it hashes
//! to exactly the root the pinned header commits. A page is never authority: [`PalwStateCarriageAssemblerV1`] holds the client to one
//! pinned header, one total length and a contiguous cursor, and only [`PalwStateCarriageAssemblerV1::finish`] — after the whole byte
//! string decodes canonically, restores through the same `into_state_v3` the offline CLI runs, and the restored state's committed
//! root equals the pinned header's — yields anything a caller may write or sign against.
use crate::{
    Hash64,
    config::params::Params,
    header::Header,
    palw_fork_choice_commitment_v1::{
        PalwForkChoiceOpeningV1, PalwForkChoicePointV1, palw_fork_choice_committed_at_v1,
    },
    palw_state_proof_v1::state_root_of_pinned_header_v1,
    palw_state_v2::{PalwChainStateV2, PalwStateCarriageV2},
};

/// The least bytes a page may ask for.
pub const PALW_CARRIAGE_RPC_MIN_PAGE_BYTES_V1: usize = 1024;
/// The most bytes a page may carry.
pub const PALW_CARRIAGE_RPC_MAX_PAGE_BYTES_V1: usize = 1 << 20;
/// The largest carriage served or assembled — the offline CLI's own read cap (`palw-class panel-lifetime-v2 --carriage`, 512 MiB).
pub const PALW_CARRIAGE_RPC_MAX_TOTAL_BYTES_V1: u64 = 512 << 20;

/// One page of op 243.
#[derive(Clone, Debug)]
pub struct PalwStateCarriagePageV1 {
    /// The pinned block's header (the client recomputes its hash).
    pub header: Header,
    /// Its selected parent's header: the point the served state is the post-state of.
    pub parent_header: Header,
    /// Past `palw_fork_choice_commitment_v1` the header commits the envelope; the opening names the ADR-0043 root inside it.
    pub fork_choice: Option<PalwForkChoiceOpeningV1>,
    /// The serving node's ADR-0043 root of the state (informational: the client derives it, never reads it from here).
    pub state_root: Hash64,
    pub total_bytes: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
}

/// The canonical carriage bytes of a state: exactly what a pruning snapshot and `--carriage` hold.
pub fn palw_state_carriage_bytes_v1(state: &PalwChainStateV2) -> Result<Vec<u8>, String> {
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(state)).map_err(|e| format!("carriage encoding: {e}"))?;
    if bytes.len() as u64 > PALW_CARRIAGE_RPC_MAX_TOTAL_BYTES_V1 {
        return Err(format!("the carriage is {} bytes, over the {PALW_CARRIAGE_RPC_MAX_TOTAL_BYTES_V1} op 243 serves", bytes.len()));
    }
    Ok(bytes)
}

/// The `[offset, offset + limit)` window of `carriage` (shorter at the end). `offset == total` is refused unless the carriage is
/// empty, so a cursor past the end is never answered with an empty "success".
pub fn palw_state_carriage_window_v1(carriage: &[u8], offset: u64, limit: usize) -> Result<Vec<u8>, String> {
    if !(PALW_CARRIAGE_RPC_MIN_PAGE_BYTES_V1..=PALW_CARRIAGE_RPC_MAX_PAGE_BYTES_V1).contains(&limit) {
        return Err("carriage page byte limit must be 1024..1048576".into());
    }
    let total = carriage.len() as u64;
    if offset > total || (offset == total && total != 0) {
        return Err("carriage cursor is outside the pinned snapshot".into());
    }
    let start = offset as usize;
    let end = start.saturating_add(limit).min(carriage.len());
    Ok(carriage[start..end].to_vec())
}

/// What [`PalwStateCarriageAssemblerV1::finish`] proves: the restored state, the exact bytes it was restored from (to be written
/// as-is), its ADR-0043 root (`--state-root` of the offline CLI) and the root the pinned header commits.
pub struct PalwStateCarriageVerifiedV1 {
    pub state: PalwChainStateV2,
    pub carriage: Vec<u8>,
    pub state_root: Hash64,
    pub committed_root: Hash64,
    pub parent: Hash64,
    pub parent_daa: u64,
}

/// **The client half of op 243.** Built from the first page (offset 0), fed every page in cursor order, finished once.
pub struct PalwStateCarriageAssemblerV1 {
    pinned: Hash64,
    committed: Hash64,
    #[expect(dead_code, reason = "kept with the assembler's pinned page; not read yet")]
    header: Header,
    parent_header: Header,
    fork_choice: Option<PalwForkChoiceOpeningV1>,
    total: u64,
    bytes: Vec<u8>,
}

impl PalwStateCarriageAssemblerV1 {
    /// Checks the first page's header against the pinned block hash and the parent header against the header's direct parents, and
    /// bounds the total by `max_total` (at most [`PALW_CARRIAGE_RPC_MAX_TOTAL_BYTES_V1`]). The bytes are pushed with [`Self::push`].
    pub fn new(pinned: Hash64, first: &PalwStateCarriagePageV1, max_total: u64) -> Result<Self, String> {
        if first.offset != 0 {
            return Err("the first carriage page must start at offset 0".into());
        }
        if max_total == 0 || max_total > PALW_CARRIAGE_RPC_MAX_TOTAL_BYTES_V1 || first.total_bytes > max_total {
            return Err(format!("the served carriage ({} bytes) exceeds the client's bound of {max_total} bytes", first.total_bytes));
        }
        let committed = state_root_of_pinned_header_v1(&first.header, pinned).map_err(|e| e.to_string())?;
        let parent = crate::hashing::header::hash(&first.parent_header);
        if !first.header.parents_by_level.get(0).is_some_and(|parents| parents.contains(&parent)) {
            return Err("the carriage's parent header is not a parent of the pinned block".into());
        }
        Ok(Self {
            pinned,
            committed,
            header: first.header.clone(),
            parent_header: first.parent_header.clone(),
            fork_choice: first.fork_choice,
            total: first.total_bytes,
            bytes: Vec::new(),
        })
    }

    /// The next offset to ask for.
    pub fn cursor(&self) -> u64 {
        self.bytes.len() as u64
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn is_complete(&self) -> bool {
        self.cursor() == self.total
    }

    /// One page, in order: the same pinned header, parent, opening and total, starting exactly at the cursor, non-empty while bytes
    /// remain, never past the total.
    pub fn push(&mut self, page: &PalwStateCarriagePageV1) -> Result<(), String> {
        if crate::hashing::header::hash(&page.header) != self.pinned
            || crate::hashing::header::hash(&page.parent_header) != crate::hashing::header::hash(&self.parent_header)
            || page.fork_choice != self.fork_choice
            || page.total_bytes != self.total
            || page.offset != self.cursor()
        {
            return Err("a carriage page changed its pinned snapshot or cursor".into());
        }
        if page.bytes.len() > PALW_CARRIAGE_RPC_MAX_PAGE_BYTES_V1 {
            return Err("a carriage page exceeds the page bound".into());
        }
        let end = self.cursor().checked_add(page.bytes.len() as u64).ok_or("carriage cursor overflow")?;
        if end > self.total || (page.bytes.is_empty() && !self.is_complete()) {
            return Err("a carriage page overruns the total or makes no progress".into());
        }
        self.bytes.extend_from_slice(&page.bytes);
        Ok(())
    }

    /// **The only output that is authority.** The assembled bytes must (1) be complete, (2) decode as a `PalwStateCarriageV2` that
    /// re-encodes to exactly these bytes, (3) restore through `into_state_v3` — the offline CLI's restore, with the network's
    /// `palw_uncertified_weightless` read at the carriage's own point and `palw_canonical_work_daa` — against the ADR-0043 root the
    /// pinned header commits (through the verified fork-choice opening past its fence), (4) be the post-state of the pinned block's
    /// parent, and (5) commit, in the header's form, exactly the pinned header's root. Anything else writes nothing.
    pub fn finish(self, params: &Params) -> Result<PalwStateCarriageVerifiedV1, String> {
        if !self.is_complete() {
            return Err("the carriage is incomplete; nothing is authenticated".into());
        }
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return Err("this network has no compiled native PALW consensus".into());
        };
        let fence = params.palw_fork_choice_commitment_v1;
        let parent = crate::hashing::header::hash(&self.parent_header);
        let point = PalwForkChoicePointV1 {
            block: parent,
            daa_score: self.parent_header.daa_score,
            blue_score: self.parent_header.blue_score,
        };
        // RFC-0018 GAP-R4: past rule E the header commits a rule-E leaf (v2/v3), which a v1 opening cannot open. There the opening's
        // inner root is only the restore target: the restored state's own committed root (in the form its point selects) must equal
        // the header's below, and that envelope binds the inner root — so nothing unverified is accepted.
        let rule_e_form = matches!(
            crate::palw_fork_choice_rule_e_leaf_v3::palw_rule_e_commit_form_v1(params, self.parent_header.daa_score),
            crate::palw_fork_choice_rule_e_leaf_v3::PalwCommitFormV1::RuleEV2 | crate::palw_fork_choice_rule_e_leaf_v3::PalwCommitFormV1::RuleEV3
        );
        let inner = match &self.fork_choice {
            Some(opening) if rule_e_form => opening.inner_root,
            Some(opening) => {
                opening.verify(&self.committed, &point, fence).map_err(|e| e.to_string())?;
                opening.inner_root
            }
            None if palw_fork_choice_committed_at_v1(fence, Some(self.parent_header.daa_score)) => {
                return Err("the carriage omits the fork-choice opening its header's root needs".into());
            }
            None => self.committed,
        };
        let carriage: PalwStateCarriageV2 =
            borsh::from_slice(&self.bytes).map_err(|e| format!("the carriage does not decode: {e}"))?;
        if borsh::to_vec(&carriage).map_err(|e| e.to_string())? != self.bytes {
            return Err("the carriage is not in its canonical encoding".into());
        }
        let daa = carriage.last_point.as_ref().map(|p| p.daa_score).unwrap_or(0);
        let armed = params.palw_uncertified_weightless.is_some_and(|a| a.is_active(daa));
        let state = carriage
            .into_state_v3(&bundle.state, Some(inner), armed, params.palw_canonical_work_daa())
            .map_err(|e| format!("the carriage does not restore to the pinned root: {e}"))?;
        if state.last_point().map(|p| p.block) != Some(parent) {
            return Err("the restored state is not the post-state of the pinned block's selected parent".into());
        }
        // RFC-0018 GAP-R4: the form the restored state's point selects under the reader's ruleset (equal to the v1 form wherever rule
        // E is not in force).
        let committed = crate::palw_fork_choice_rule_e_leaf_v3::palw_committed_state_root_v2(&state, params);
        if committed != self.committed {
            return Err(format!("the restored state commits {committed}, the pinned header commits {}", self.committed));
        }
        Ok(PalwStateCarriageVerifiedV1 {
            state_root: state.state_root(),
            state,
            carriage: self.bytes,
            committed_root: committed,
            parent,
            parent_daa: self.parent_header.daa_score,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_are_bounded_contiguous_and_never_past_the_end() {
        let carriage: Vec<u8> = (0..3000u32).map(|i| i as u8).collect();
        assert!(palw_state_carriage_window_v1(&carriage, 0, 1023).is_err());
        assert!(palw_state_carriage_window_v1(&carriage, 0, (1 << 20) + 1).is_err());
        assert_eq!(palw_state_carriage_window_v1(&carriage, 0, 1024).unwrap(), carriage[..1024]);
        assert_eq!(palw_state_carriage_window_v1(&carriage, 2048, 1024).unwrap(), carriage[2048..]);
        assert!(palw_state_carriage_window_v1(&carriage, 3000, 1024).is_err());
        assert!(palw_state_carriage_window_v1(&carriage, 3001, 1024).is_err());
        assert!(palw_state_carriage_window_v1(&[], 0, 1024).unwrap().is_empty());
    }

    #[test]
    fn genesis_carriage_round_trips_canonically() {
        let state = PalwChainStateV2::genesis();
        let bytes = palw_state_carriage_bytes_v1(&state).unwrap();
        let decoded: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(borsh::to_vec(&decoded).unwrap(), bytes);
    }
}
