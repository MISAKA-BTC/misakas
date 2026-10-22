//! Row types of the RFC08 post-Final duty record (aux 144, `0x02 ‖ family ‖ job`), kept at the crate root so the native
//! resource scanners can name them; the fold lives in `palw_state_v2::palw_native_post_final_duty_v1` (G14 r6 D4).
use borsh::{BorshDeserialize, BorshSerialize};
use crate::palw_court_duty_clock_v1::CourtDutyClockV1;
use crate::palw_state_v2::PalwBondKeyV2;

/// One actor's duty in a post-Final accusation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePostFinalDutyV1 {
    pub clock: CourtDutyClockV1,
    /// The first DAA inside the clock's eligible window at which the court service capacity left room for one packet.
    pub supplied_daa: Option<u64>,
    /// The DAA and digest of the actor's answer, if it answered in time.
    pub answered: Option<(u64, [u8; 64])>,
}

/// A job's post-Final accusation and its duties (aux 144, `0x02 ‖ family ‖ job`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativePostFinalDutyRowV1 {
    pub version: u8,
    pub family: u8,
    pub job: [u8; 64],
    pub claim: [u8; 64],
    pub accusation: [u8; 64],
    pub accuser: PalwBondKeyV2,
    pub accused_daa: u64,
    pub duties: Vec<NativePostFinalDutyV1>,
}

