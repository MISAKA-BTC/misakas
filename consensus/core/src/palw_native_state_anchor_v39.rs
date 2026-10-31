//! Owning39 bounded state-envelope opening. The exact child and selected-parent
//! headers and the auditor's own ruleset select the grammar. No raw-root fallback.
use crate::{
    Hash64,
    config::params::Params,
    header::Header,
    palw_fork_choice_commitment_v1::{PalwForkChoiceLeafV1, PalwForkChoiceOpeningV1, PalwForkChoicePointV1},
    palw_state_proof_v1::{PalwStateOpeningV1, state_root_of_pinned_header_v1},
};
use borsh::BorshSerialize;
pub const NATIVE_ANCHOR_LABEL_V39: &[u8] = b"kernel-anchor-v39";
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeStateEnvelopeV39 {
    pub version: u16,
    pub leaf: Vec<u8>,
}
impl NativeStateEnvelopeV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() < 6 || raw.len() > 1030 || raw[..2] != 39u16.to_le_bytes() {
            return Err("anchor39 bounded exact envelope version".into());
        }
        let n = u32::from_le_bytes(raw[2..6].try_into().unwrap()) as usize;
        if n > 1024 || n != raw.len() - 6 {
            return Err("anchor39 bounded exact leaf extent".into());
        }
        Ok(Self { version: 39, leaf: raw[6..].to_vec() })
    }
}
/// This authority means only an inner state root under these exact pinned
/// headers, never header-chain validity or model/reward qualification.
pub struct CheckedNativeStateAnchorV39 {
    inner: Hash64,
    child: Hash64,
    parent: Hash64,
    daa: u64,
}
impl CheckedNativeStateAnchorV39 {
    pub fn inner(&self) -> Hash64 {
        self.inner
    }
    pub fn child(&self) -> Hash64 {
        self.child
    }
    pub fn parent(&self) -> Hash64 {
        self.parent
    }
    pub fn daa(&self) -> u64 {
        self.daa
    }
}
pub fn check_native_state_anchor_v39(
    child: &Header,
    pinned_child: Hash64,
    parent: &Header,
    pinned_parent: Hash64,
    params: &Params,
    state: &PalwStateOpeningV1,
    envelope: &NativeStateEnvelopeV39,
) -> Result<CheckedNativeStateAnchorV39, String> {
    if envelope.version != 39 || envelope.leaf.len() > 1024 || state.preimage.len() > 4 << 20 {
        return Err("anchor39 original state/leaf profile differs".into());
    }
    let committed = state_root_of_pinned_header_v1(child, pinned_child).map_err(|e| e.to_string())?;
    if crate::hashing::header::hash(parent) != pinned_parent
        || !child.parents_by_level.get(0).is_some_and(|p| p.contains(&pinned_parent))
        || parent.daa_score > child.daa_score
    {
        return Err("anchor39 original child/selected-parent headers differ".into());
    }
    let inner = state.root();
    if params.palw_fork_choice_commitment_active_at(parent.daa_score) {
        if params.palw_fork_choice_rule_e_active_at(parent.daa_score) {
            // RFC-0018 GAP-R4: the rule-E leaf in the grammar the auditor's own ruleset selects at the parent's point (v2, or v3 with
            // rule E v2 capital and Gate C certification state) — the verifier node, L2 client and light client share.
            crate::palw_fork_choice_rule_e_leaf_v3::palw_verify_committed_leaf_v2(
                params,
                &committed,
                &PalwForkChoicePointV1 { block: pinned_parent, daa_score: parent.daa_score, blue_score: parent.blue_score },
                &envelope.leaf,
                &inner,
            )
            .map_err(|e| format!("anchor39 original Rule-E leaf: {e:?}"))?;
        } else {
            let leaf = PalwForkChoiceLeafV1::decode(&envelope.leaf).ok_or("anchor39 original V1 leaf grammar")?;
            PalwForkChoiceOpeningV1 { leaf, inner_root: inner }
                .verify(
                    &committed,
                    &PalwForkChoicePointV1 { block: pinned_parent, daa_score: parent.daa_score, blue_score: parent.blue_score },
                    params.palw_fork_choice_commitment_v1,
                )
                .map_err(|e| e.to_string())?;
        }
    } else if !envelope.leaf.is_empty() || inner != committed {
        return Err("anchor39 unexpected leaf or original raw root differs below owning fence".into());
    }
    Ok(CheckedNativeStateAnchorV39 { inner, child: pinned_child, parent: pinned_parent, daa: parent.daa_score })
}
