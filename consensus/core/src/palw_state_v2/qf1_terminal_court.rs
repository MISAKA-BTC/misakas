//! Ordered terminal Court writes for the existing private MOE2 candidate path.
use super::*;
use crate::palw_tb1::qf1_terminal_carrier::Qf1TerminalRuling;

const TABLE: u8 = 212;

#[derive(borsh::BorshDeserialize, borsh::BorshSerialize, Default)]
struct CourtStatus {
    pending: u32,
    refuted: bool,
}

#[derive(Clone)]
pub enum Qf1TerminalUpdate {
    Open(Qf1TerminalRuling),
    Verdict { block: Hash64, marker: [u8; 64] },
}

fn key(kind: u8, digest: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(kind);
    key.extend_from_slice(digest);
    key
}

impl PalwChainStateV2 {
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
    point: &PalwBlockContextV2,
    update: Qf1TerminalUpdate,
) -> Result<(), PalwStateV2Error> {
    let mismatch = PalwStateV2Error::DeltaMismatch;
    if !builder.extras.private_moe2_registration {
        return Err(mismatch("QF1 terminal Court outside the private profile"));
    }
    let (ruling, resolves) = match update {
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
