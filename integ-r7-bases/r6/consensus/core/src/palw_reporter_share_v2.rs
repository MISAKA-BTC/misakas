//! **ADR-0032's 2026-10-10 amendment behind its own dormant fence: the PALW reporter share, 10% → 49%** (lane INTF).
//!
//! The amendment sets R-core's R-1 reporter reward (`⌊r × max(0, collected − X)⌋`) and DA-6's refuted-session cost
//! (`min(⌈r × S_P(stage)⌉, min_collateral)`) to `r` = 4,900 bps. Its own text requires "a versioned migration that does not
//! reinterpret the past 10% accounting", and testnet-12 has run R-core+ from genesis at 1,000 bps. So the share is a fence:
//!
//! * **below `Params::palw_reporter_share_v2`** — every block of every shipped network — `r` is
//!   [`PALW_RCORE_REPORTER_REWARD_BPS_V1`] (1,000 bps), byte for byte what int-12 folds;
//! * **at or past it** `r` is [`PALW_RCORE_REPORTER_REWARD_BPS_V2`] (4,900 bps).
//!
//! **Which DAA fixes an amount.** R-1's reward is fixed at the conviction's close (the block that consumes the offence and opens
//! the reward; the consumed record's `accepted_daa`), DA-6's exposure at the session's open (the accusation block; the exposure is
//! stored on the session). Neither is re-priced later — a sweep, an award, a refund, a burn, a reorg replay or a reload reads the
//! stored amount — so a reward or an exposure written below the fence keeps its 10% amount for its whole life, and one written at or
//! past it is 49% from the start. The fold reads the share through the bundle's mirror
//! ([`crate::palw_state_v2::PalwStateParamsV2::reporter_reward_bps_at`]).
//!
//! **What does not move.** The DNS/PoS slashing split (`DnsParams::slashing_reporter_reward_bps`, 10% on testnet-12) is a separate
//! account and is not tied to this share (T24 keeps the two apart). The share is not a reward or consensus-weight source of its
//! own: it divides what a conviction already collected (the rest stays burned), so it draws on no issuance budget.
//!
//! **The fence**: `None` on every preset and in no flag-day list; hashed Some-only — with the 4,900 bps it arms beside the height —
//! in the params and schedule ids (so a 10% and a 49% build never announce one armed ruleset, the amendment's handshake rule, and
//! testnet-12's live ids stay int-12's); collapsed from `Some(never())`; a `palw_fences_v1` entry and a fork-id probe arm; and
//! **refused when armed** ([`Params::validate_palw_reporter_share_v2`]) until the full-activation release names its height.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
pub use crate::palw_state_v2::{PALW_RCORE_REPORTER_REWARD_BPS_V1, PALW_RCORE_REPORTER_REWARD_BPS_V2};

impl Params {
    /// `palw_reporter_share_v2`, resolved: `Some` only on a `ConsensusV2` network with a real height (a `never()` value is dormant).
    pub fn palw_reporter_share_v2_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_reporter_share_v2.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// **Is ADR-0032's amended share in force at `daa_score`?** `false` on every preset.
    pub fn palw_reporter_share_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_reporter_share_v2_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The PALW reporter share, in basis points, in force at `daa_score`** — 1,000 below the fence (every block of every
    /// preset), 4,900 at or past it. What an operator's view prints for the tip; the fold reads the bundle's mirror.
    pub fn palw_reporter_share_bps_at(&self, daa_score: u64) -> u16 {
        if self.palw_reporter_share_v2_active_at(daa_score) {
            PALW_RCORE_REPORTER_REWARD_BPS_V2
        } else {
            PALW_RCORE_REPORTER_REWARD_BPS_V1
        }
    }

    /// **The fence's mirror** on the V2 bundle's state params (`reporter_share_v2_from_daa`), which the fold reads. Written here and
    /// nothing else; `None` where the fence is not armed (or is `never()`).
    pub fn sync_palw_reporter_share_v2(&mut self) {
        let from_daa = self.palw_reporter_share_v2.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_reporter_share_v2_from_daa(from_daa);
        }
    }

    /// **The fence's refusals**, asked by [`Params::validate_palw_v2`]: a V2 bundle whose mirror is not the fence's; arming on a
    /// ruleset that is not `ConsensusV2`; arming without `palw_rcore_plus` in force at or below it (R-1 and DA-6 exist only there);
    /// and — until the full-activation release names its height (the Lead's registry) — **any arming at all**, as every other unarmed
    /// economic fence. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_reporter_share_v2(&self) -> Result<(), PalwModeV2Error> {
        let armed = self.palw_reporter_share_v2.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.reporter_share_v2_from_daa(),
            _ if armed.is_some() => {
                return Err(PalwModeV2Error::Invalid("palw_reporter_share_v2 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
            }
            _ => None,
        };
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_reporter_share_v2 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_reporter_share_v2 after \
                 the bundle is assembled",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !self.palw_rcore_plus.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_reporter_share_v2 needs palw_rcore_plus in force at or below it: it re-prices R-core's R-1 reward and DA-6 exposure",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_reporter_share_v2 cannot be armed yet: ADR-0032's 49% reporter share re-prices every R-core conviction's reward and \
             every DA session's exposure, and its height is the full-activation release's to name (lane INTF)",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{
        devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_launch_params_v1, palw_t12_shipped_params,
    };

    fn refusal(p: &Params) -> String {
        match p.validate_palw_reporter_share_v2() {
            Err(PalwModeV2Error::Invalid(why)) => why.to_string(),
            other => panic!("expected a named refusal, got {other:?}"),
        }
    }

    /// **Dormant everywhere**: `None` on every preset, 10% at every height, the mirror unset, named by the exhaustive list.
    #[test]
    fn the_share_fence_is_dormant_on_every_preset() {
        for (name, p) in [
            ("testnet-12", palw_t12_shipped_params()),
            ("testnet-12 launch", palw_t12_launch_params_v1()),
            ("testnet-11", palw_rc_shipped_params()),
            ("devnet", devnet_shipped_params()),
            ("mainnet", mainnet_shipped_params()),
        ] {
            assert_eq!(p.palw_reporter_share_v2, None, "{name}: dormant");
            assert!(!p.palw_reporter_share_v2_active_at(u64::MAX), "{name}: never in force");
            assert_eq!(p.palw_reporter_share_bps_at(u64::MAX), 1_000, "{name}: the historical 10% share");
            p.validate_palw_reporter_share_v2().expect("a dormant preset has nothing to refuse");
            assert!(p.palw_fences_v1().contains(&("palw_reporter_share_v2", None)), "{name}: the exhaustive list names it");
            if let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
                assert_eq!(bundle.state.reporter_share_v2_from_daa(), None, "{name}: no mirror");
                assert_eq!(bundle.state.reporter_reward_bps_at(u64::MAX), 1_000, "{name}: the fold reads 10%");
            }
        }
    }

    /// **Some(never()) is absence; a height is refused, named; the ids move only by Some.**
    #[test]
    fn arming_is_refused_and_only_a_real_height_moves_the_ids() {
        let t12 = palw_t12_shipped_params();
        let ids = |p: &Params| (p.consensus_params_id(), p.consensus_identity_id(), p.consensus_schedule_id());

        let mut never = t12.clone();
        never.palw_reporter_share_v2 = Some(ForkActivation::never());
        never.sync_palw_reporter_share_v2();
        never.validate_palw_reporter_share_v2().expect("never() is dormant");
        never.validate_palw_v2().expect("and the ruleset validates");
        assert_eq!(never.consensus_identity_id(), t12.consensus_identity_id(), "Some(never()) collapses for the identity");
        assert_eq!(never.consensus_params_id(), t12.consensus_params_id(), "and is not hashed into the params id");
        assert!(!never.palw_reporter_share_v2_active_at(u64::MAX));

        let mut armed = t12.clone();
        armed.palw_reporter_share_v2 = Some(ForkActivation::new(9_000_000));
        armed.sync_palw_reporter_share_v2();
        assert!(refusal(&armed).contains("cannot be armed yet"), "{}", refusal(&armed));
        assert!(armed.validate_palw_v2().is_err(), "validate_palw_v2 refuses it too");
        assert!(!armed.palw_reporter_share_v2_active_at(8_999_999) && armed.palw_reporter_share_v2_active_at(9_000_000));
        assert_eq!((armed.palw_reporter_share_bps_at(8_999_999), armed.palw_reporter_share_bps_at(9_000_000)), (1_000, 4_900));
        let (p0, i0, s0) = ids(&t12);
        let (p1, i1, s1) = ids(&armed);
        assert_ne!(p0, p1, "the params id names the armed share");
        assert_eq!(i0, i1, "a future height is not yet a rule for the identity");
        assert_ne!(s0, s1, "the schedule names it");

        // An unsynced mirror, and arming without R-core+, are refused by name.
        let mut unsynced = t12.clone();
        unsynced.palw_reporter_share_v2 = Some(ForkActivation::new(9_000_000));
        assert!(refusal(&unsynced).contains("disagrees with the V2 bundle's mirror"));
        let mut no_rcore = armed.clone();
        no_rcore.palw_rcore_plus = None;
        assert!(refusal(&no_rcore).contains("needs palw_rcore_plus"));
        let mut mainnet = mainnet_shipped_params();
        mainnet.palw_reporter_share_v2 = Some(ForkActivation::new(9_000_000));
        assert!(refusal(&mainnet).contains("ConsensusV2 rule"));
    }
}
