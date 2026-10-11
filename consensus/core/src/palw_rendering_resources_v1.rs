//! Quantitative prospective resource terms for the existing V1 table/constraint stream.
//! This plan deliberately uses GLOBAL authenticated format caps: an owner signature asserting
//! smaller table dimensions does not authenticate them against legacy table leaves.
//! Applying these terms to claims requires a new explicit fence and an outer owned court;
//! existing reservations and their historic deadlines are never extended by this helper.
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRenderingResourcePlanV1 {
    pub prefix_tokens: u32,
    pub vocab: u32,
    pub maximum_stream_bytes: u64,
    pub maximum_stream_leaves: u64,
    pub maximum_eog_bytes: u64,
    pub maximum_moves: u64,
    pub maximum_parser_steps: u64,
    pub maximum_blake_blocks: u64,
    pub inclusion_daa: u64,
    pub rent_sompi: u64,
    pub deposit_sompi: u64,
    /// Canonical stored bytes, distinct from Rust heap resident memory.
    pub row_bytes: u64,
    pub resident_ram_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRenderingResourcePricesV1 {
    /// The maximum permitted time to include one eligible move, stated by the NEW contract.
    /// This is a feasibility assumption on inclusion, not a power to compel miners.
    pub inclusion_margin_daa: u64,
    /// Canonically priced byte processing, independent of a prover's asserted rendering size.
    pub rent_per_stream_byte: u64,
    /// Price of each worst-case bounded constraint edge/epsilon action.
    pub rent_per_parser_step: u64,
    /// Price of the permanent bounded row, charged once by the new declaration.
    pub rent_per_row_byte: u64,
    /// Additional capital locked while the stream can occupy its own reservation.
    pub deposit_per_move: u64,
    /// A finite restoration envelope. Unbounded deep reorgs cannot have finite deadline guarantees.
    pub reorg_replay_moves: u64,
}
fn narrow(value: u128) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| "rendering resource terms exceed the platform's exact u64 envelope".into())
}
impl PalwRenderingResourcePlanV1 {
    /// Prefix length and vocab come from the authenticated binding/profile, never a declaration.
    /// The decision may need EVERY token-table leaf to prove a stop, not just two nominated lanes.
    pub fn derive(prefix_tokens: u32, vocab: u32, p: PalwRenderingResourcePricesV1) -> Result<Self, String> {
        use crate::palw_fp_constraint_job_v1::{PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1, PALW_TOKEN_TABLE_MAX_VOCAB_V1};
        if prefix_tokens as usize >= crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS
            || vocab == 0
            || vocab as usize >= PALW_TOKEN_TABLE_MAX_VOCAB_V1
            || p.inclusion_margin_daa == 0
        {
            return Err("resource terms need authenticated in-format prefix/vocabulary and positive inclusion margin".into());
        }
        let table = PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 as u128;
        let prefix = u128::from(prefix_tokens);
        let vocab128 = u128::from(vocab);
        // Each prefix token can reuse the largest rendering at a different parser state.
        // The final all-vocabulary decision carries at most one entire table's rendering bytes.
        let bytes = (prefix + 1) * table;
        let leaves = prefix + vocab128 + 1; // include authenticated EOG metadata
        let part = crate::palw_rendering_stream_v1::PALW_RENDERING_STREAM_PART_BYTES_V1 as u128;
        // Prefix bodies round independently; per-table ceil sums are <=ceil(total/part)+vocab.
        // Each leaf also needs one declaration/opening. Zero-byte/None leaves still cost a move.
        let body_moves = prefix * table.div_ceil(part) + table.div_ceil(part) + vocab128;
        let eog_bytes = vocab128 * 4;
        let eog_moves = eog_bytes.div_ceil(part);
        let moves = body_moves + 2 * leaves + eog_moves + 2 + u128::from(p.reorg_replay_moves); // opening and terminal
        // Also covers <=13 binary-search membership comparisons per EOG id.
        let parser_steps = (bytes + eog_bytes) * (crate::palw_decode_constraint_v1::PALW_CONSTRAINT_MAX_HOPS_V1 as u128 + 1);
        // Key block, legacy id/presence/length header and final padding per leaf.
        let blake_blocks = (bytes + eog_bytes).div_ceil(128) + leaves * 3;
        // A256KiB public canonical constraint/job plus pin, opening and <1KiB continuation.
        let row_bytes = crate::palw_fp_constraint_v2::PALW_CONSTRAINT_MAX_BYTES_V2 as u128 + 65_536 + 16_384 + 1_024;
        // Each canonical constraint node consumes >=5 wire bytes. Native Vec/node/edge
        // layouts and allocator spare capacity are conservatively covered by32x the grammar
        // cap on supported32/64-bit platforms, plus the bounded move and metadata.
        let resident_ram_bytes = crate::palw_fp_constraint_v2::PALW_CONSTRAINT_MAX_BYTES_V2 as u128 * 32 + 262_144;
        let rent = (bytes + eog_bytes) * u128::from(p.rent_per_stream_byte)
            + parser_steps * u128::from(p.rent_per_parser_step)
            + row_bytes * u128::from(p.rent_per_row_byte);
        let deposit = moves * u128::from(p.deposit_per_move);
        Ok(Self {
            prefix_tokens,
            vocab,
            maximum_stream_bytes: narrow(bytes)?,
            maximum_stream_leaves: narrow(leaves)?,
            maximum_eog_bytes: narrow(eog_bytes)?,
            maximum_moves: narrow(moves)?,
            maximum_parser_steps: narrow(parser_steps)?,
            maximum_blake_blocks: narrow(blake_blocks)?,
            inclusion_daa: narrow(moves * u128::from(p.inclusion_margin_daa))?,
            rent_sompi: narrow(rent)?,
            deposit_sompi: narrow(deposit)?,
            row_bytes: narrow(row_bytes)?,
            resident_ram_bytes: narrow(resident_ram_bytes)?,
        })
    }
    /// Exact prospective duration from this accepted claim, with an explicitly retained reorg
    /// envelope. Saturation is a refusal, never a silently shorter lifetime.
    pub fn deadline(&self, accepted_daa: u64) -> Result<u64, String> {
        accepted_daa.checked_add(self.inclusion_daa).ok_or_else(|| "prospective stream deadline overflows DAA".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn prices() -> PalwRenderingResourcePricesV1 {
        PalwRenderingResourcePricesV1 {
            inclusion_margin_daa: 4,
            rent_per_stream_byte: 1,
            rent_per_parser_step: 1,
            rent_per_row_byte: 1,
            deposit_per_move: 1,
            reorg_replay_moves: 0,
        }
    }
    #[test]
    fn maximum_active_format_produces_quantitative_work_deadline_and_capital() {
        let plan = PalwRenderingResourcePlanV1::derive(4095, (1 << 22) - 1, prices()).unwrap();
        assert_eq!(plan.maximum_stream_bytes, 1u64 << 40);
        assert!(plan.maximum_moves > 16_000_000);
        assert_eq!(plan.inclusion_daa, plan.maximum_moves * 4);
        assert_eq!(plan.maximum_parser_steps, ((1u64 << 40) + plan.maximum_eog_bytes) * 69);
        assert!(plan.resident_ram_bytes < 9 * 1024 * 1024);
        assert!(plan.rent_sompi > 75_000_000_000_000);
        assert!(plan.row_bytes < 350_000);
        assert!(plan.deadline(u64::MAX).is_err());
    }
    #[test]
    fn plan_is_monotone_checked_and_cannot_certify_owner_asserted_small_sizes() {
        let small = PalwRenderingResourcePlanV1::derive(2, 4099, prices()).unwrap();
        assert_eq!(small.maximum_stream_bytes, 3 * 256 * 1024 * 1024);
        let larger = PalwRenderingResourcePlanV1::derive(3, 4100, prices()).unwrap();
        assert!(larger.maximum_moves > small.maximum_moves && larger.rent_sompi > small.rent_sompi);
        assert!(PalwRenderingResourcePlanV1::derive(4096, 4099, prices()).is_err());
        assert!(PalwRenderingResourcePlanV1::derive(1, 1 << 22, prices()).is_err());
        assert!(
            PalwRenderingResourcePlanV1::derive(0, 4099, PalwRenderingResourcePricesV1 { rent_per_parser_step: u64::MAX, ..prices() })
                .is_err()
        );
    }
}

/// Prospective claim-wide liability/material contract, admitted with a new-fence claim.
/// The stream cannot outlive the accused's liability or the public material it needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRenderingClaimTermsV1 {
    pub accepted_daa: u64,
    pub open_until_daa: u64,
    pub stream_until_daa: u64,
    pub collect_until_daa: u64,
    pub material_until_daa: u64,
    pub bond_exit_after_daa: u64,
    pub plan: PalwRenderingResourcePlanV1,
}
impl PalwRenderingClaimTermsV1 {
    /// Acquisition is an explicit inclusion/DA envelope, collection includes terminal and
    /// reporter reveal. Reorg retention is explicit; no unbounded reorg guarantee is asserted.
    pub fn derive(
        accepted_daa: u64,
        plan: PalwRenderingResourcePlanV1,
        acquisition_daa: u64,
        collection_daa: u64,
        reorg_retention_daa: u64,
    ) -> Result<Self, String> {
        if acquisition_daa == 0 || collection_daa == 0 {
            return Err("claim contract requires positive acquisition and collection envelopes".into());
        }
        let add = |a: u64, b: u64| a.checked_add(b).ok_or_else(|| "prospective claim retention/exit clock overflow".to_string());
        let open_until_daa = add(accepted_daa, acquisition_daa)?;
        let stream_until_daa = plan.deadline(open_until_daa)?;
        let collect_until_daa = add(stream_until_daa, collection_daa)?;
        let material_until_daa = add(collect_until_daa, reorg_retention_daa)?;
        Ok(Self {
            accepted_daa,
            open_until_daa,
            stream_until_daa,
            collect_until_daa,
            material_until_daa,
            bond_exit_after_daa: material_until_daa,
            plan,
        })
    }
    /// Acceptance must use these prospective terms for EVERY liable producer/Panel bond and
    /// each mandatory public material row; a shorter ordinary clock cannot silently substitute.
    pub fn check_holds(&self, material_until: u64, producer_exit_after: u64, panel_exit_after: &[u64]) -> Result<(), String> {
        if material_until < self.material_until_daa
            || producer_exit_after < self.bond_exit_after_daa
            || panel_exit_after.iter().any(|at| *at < self.bond_exit_after_daa)
        {
            return Err("prospective stream lacks material retention or accused liability/exit coverage".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod claim_terms_tests {
    use super::*;
    #[test]
    fn long_stream_contract_refuses_any_shorter_liability_or_material_clock_and_overflow() {
        let plan = PalwRenderingResourcePlanV1::derive(
            4095,
            4099,
            PalwRenderingResourcePricesV1 {
                inclusion_margin_daa: 4,
                rent_per_stream_byte: 1,
                rent_per_parser_step: 1,
                rent_per_row_byte: 1,
                deposit_per_move: 1,
                reorg_replay_moves: 20,
            },
        )
        .unwrap();
        let t = PalwRenderingClaimTermsV1::derive(100, plan, 1000, 100, 80).unwrap();
        assert!(t.stream_until_daa > 60_000_000);
        assert!(t.check_holds(1000, 1000, &[1000; 5]).is_err());
        assert!(t.check_holds(t.material_until_daa, t.bond_exit_after_daa, &[t.bond_exit_after_daa; 5]).is_ok());
        assert!(t.check_holds(t.material_until_daa, t.bond_exit_after_daa, &[t.bond_exit_after_daa - 1]).is_err());
        assert!(PalwRenderingClaimTermsV1::derive(u64::MAX, plan, 1000, 100, 80).is_err());
    }
}
