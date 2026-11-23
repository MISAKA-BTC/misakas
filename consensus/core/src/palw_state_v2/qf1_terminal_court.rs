//! Ordered terminal Court writes for the existing private MOE2 candidate path.
use super::*;
use crate::palw_tb1::{
    panel_reply_carrier::{PanelReplayDuty, PublishedPanelApproval},
    qf1_original_carrier::Qf1OriginalAdmission,
    qf1_terminal_carrier::Qf1TerminalRuling,
};

const TABLE: u8 = 212;

#[derive(borsh::BorshDeserialize, borsh::BorshSerialize, Default)]
struct CourtStatus {
    pending: u32,
    refuted: bool,
}

#[derive(Clone)]
pub enum Qf1TerminalUpdate {
    Original(Qf1OriginalAdmission),
    PanelDraw(PanelReplayDuty),
    PanelApproval(PublishedPanelApproval),
    Open(Qf1TerminalRuling),
    Verdict { block: Hash64, marker: [u8; 64] },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qf1OriginalVerification {
    AwaitingReplay,
    ReplayApproved,
    Unverified,
}

fn key(kind: u8, digest: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(kind);
    key.extend_from_slice(digest);
    key
}

impl PalwChainStateV2 {
    pub fn qf1_panel_reply_for_candidate(
        &self,
        chain: [u8; 64],
        original: Hash64,
        selected_parent: Hash64,
        candidate: &crate::block::Block,
    ) -> Option<PublishedPanelApproval> {
        if self.last_point()?.block != selected_parent {
            return None;
        }
        let route = self.kernel_route.as_ref()?;
        let duty: PanelReplayDuty = route.aux_row(TABLE, &key(b'D', &original.as_bytes()))?;
        if duty.original != original || duty.chain != chain {
            return None;
        }
        duty.verify_bound_replies(selected_parent, candidate).ok()
    }

    pub fn qf1_original_verification(&self, original: Hash64) -> Option<Qf1OriginalVerification> {
        self.qf1_original_verification_at(original, self.last_point()?.daa_score)
    }

    pub(super) fn qf1_original_verification_at(&self, original: Hash64, daa: u64) -> Option<Qf1OriginalVerification> {
        let route = self.kernel_route.as_ref()?;
        let (_, deadline): (Hash64, u64) = route.aux_row(TABLE, &key(b'R', &original.as_bytes()))?;
        if route.aux.contains_key(&(TABLE, key(b'A', &original.as_bytes()))) {
            return Some(Qf1OriginalVerification::ReplayApproved);
        }
        Some(if daa < deadline { Qf1OriginalVerification::AwaitingReplay } else { Qf1OriginalVerification::Unverified })
    }

    pub fn qf1_terminal_verdict_pending(&self, marker: &[u8; 64], daa: u64) -> bool {
        self.kernel_route
            .as_ref()
            .and_then(|route| route.aux_row::<Qf1TerminalRuling>(TABLE, &key(b'V', marker)))
            .is_some_and(|ruling| daa < ruling.deadline)
    }

    pub fn qf1_terminal_blocks_award(&self, original: Hash64) -> bool {
        self.kernel_route
            .as_ref()
            .and_then(|route| route.aux_row::<CourtStatus>(TABLE, &key(b'O', &original.as_bytes())))
            .is_some_and(|status| status.pending != 0 || status.refuted)
    }
}

pub(super) fn fold_terminal_court(
    builder: &mut TransitionBuilder<'_>,
    selected_parent: Option<Hash64>,
    point: &PalwBlockContextV2,
    update: Qf1TerminalUpdate,
) -> Result<(), PalwStateV2Error> {
    let mismatch = PalwStateV2Error::DeltaMismatch;
    if !builder.extras.private_moe2_registration {
        return Err(mismatch("QF1 terminal Court outside the private profile"));
    }
    let (ruling, resolves) = match update {
        Qf1TerminalUpdate::Original(original) if original.block == point.block && point.daa_score < original.deadline => {
            super::palw_kernel_route_fold_v1::ensure_route_header(builder, point)?;
            let key = key(b'R', &original.original.as_bytes());
            if builder.state.qf1_original_verification_at(original.original, point.daa_score).is_some() {
                return Ok(());
            }
            let receipt = borsh::to_vec(&(original.block, original.deadline)).expect("fixed QF1 Original receipt");
            return builder.write_kernel_row(TABLE, key, Some(receipt));
        }
        Qf1TerminalUpdate::PanelDraw(draw) if draw.certificate == point.block => {
            let route = builder.state.kernel_route.as_ref().ok_or_else(|| mismatch("QF1 PANEL draw has no Original"))?;
            let original = draw.original.as_bytes();
            let (receipt, deadline): (Hash64, u64) =
                route.aux_row(TABLE, &key(b'R', &original)).ok_or_else(|| mismatch("QF1 PANEL draw has no Original"))?;
            if receipt != draw.original_block || deadline != draw.deadline || point.daa_score >= deadline || draw.selected.is_empty() {
                return Err(mismatch("QF1 PANEL draw differs from the Original receipt or deadline"));
            }
            let key = key(b'D', &original);
            if route.aux.contains_key(&(TABLE, key.clone())) {
                return Err(mismatch("QF1 Original already has a PANEL draw"));
            }
            return builder.write_kernel_row(TABLE, key, Some(borsh::to_vec(&draw).expect("fixed PANEL draw encoding")));
        }
        Qf1TerminalUpdate::PanelApproval(approval) if approval.accepting_block() == point.block => {
            if selected_parent != Some(approval.selected_parent()) {
                return Err(mismatch("QF1 PANEL approval belongs to another selected parent"));
            }
            let route = builder.state.kernel_route.as_ref().ok_or_else(|| mismatch("QF1 PANEL approval has no Original"))?;
            let original = approval.original().as_bytes();
            let (receipt, deadline): (Hash64, u64) =
                route.aux_row(TABLE, &key(b'R', &original)).ok_or_else(|| mismatch("QF1 PANEL approval has no Original"))?;
            if receipt != approval.original_block() || deadline != approval.deadline || point.daa_score >= deadline {
                return Err(mismatch("QF1 PANEL approval differs from the Original receipt or deadline"));
            }
            let draw: PanelReplayDuty =
                route.aux_row(TABLE, &key(b'D', &original)).ok_or_else(|| mismatch("QF1 PANEL draw missing"))?;
            if draw.certificate != approval.certificate || draw.duty != approval.duty {
                return Err(mismatch("QF1 PANEL approval differs from the stored draw"));
            }
            let key = key(b'A', &original);
            if route.aux.contains_key(&(TABLE, key.clone())) {
                return Ok(());
            }
            let receipt = borsh::to_vec(&approval.accepting_block()).expect("fixed QF1 PANEL receipt");
            return builder.write_kernel_row(TABLE, key, Some(receipt));
        }
        Qf1TerminalUpdate::Open(ruling) if ruling.case == point.block && point.daa_score < ruling.deadline => (ruling, false),
        Qf1TerminalUpdate::Verdict { block, marker } if block == point.block => {
            let ruling: Qf1TerminalRuling = builder
                .state
                .kernel_route
                .as_ref()
                .and_then(|route| route.aux_row(TABLE, &key(b'V', &marker)))
                .filter(|ruling: &Qf1TerminalRuling| point.daa_score < ruling.deadline && ruling.case != point.block)
                .ok_or_else(|| mismatch("QF1 verdict has no timely pending case"))?;
            (ruling, true)
        }
        _ => return Err(mismatch("QF1 terminal Court differs from its candidate point")),
    };
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, point)?;
    let status_key = key(b'O', &ruling.original.as_bytes());
    let verdict_key = key(b'V', &ruling.verdict);
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| mismatch("QF1 Court route is missing"))?;
    let mut status: CourtStatus = route.aux_row(TABLE, &status_key).unwrap_or_default();
    if resolves {
        status.pending = status.pending.checked_sub(1).ok_or_else(|| mismatch("QF1 case count underflow"))?;
        status.refuted |= ruling.refuted;
        builder.write_kernel_row(TABLE, verdict_key, None)?;
    } else {
        if route.aux.contains_key(&(TABLE, verdict_key.clone())) {
            return Err(mismatch("duplicate QF1 case"));
        }
        status.pending = status.pending.checked_add(1).ok_or_else(|| mismatch("QF1 case count overflow"))?;
        builder.write_kernel_row(TABLE, verdict_key, Some(borsh::to_vec(&ruling).expect("fixed Court row")))?;
    }
    let status = (status.pending != 0 || status.refuted).then(|| borsh::to_vec(&status).expect("fixed Court status"));
    builder.write_kernel_row(TABLE, status_key, status)
}
