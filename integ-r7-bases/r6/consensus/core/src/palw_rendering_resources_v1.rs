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
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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
    /// Every independently priced dimension must fit. Geometry fields are
    /// capacities here; the native seed/job checks separately fix their identity.
    pub fn covers_v1(&self, execution: Self) -> bool {
        self.cover(execution) == *self
    }
    /// Prefix length and vocab come from the authenticated binding/profile, never a declaration.
    /// Conservative bound allows all-vocabulary work. The refutation court itself needs only
    /// nominated witness/beat lanes; this helper does not implement an all-vocabulary census.
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
    /// One owner chooses one court. Every component covers the worst of each eligible
    /// predicate; this does not price many independent owners as a single shared deposit.
    pub fn cover(mut self, other: Self) -> Self {
        self.prefix_tokens = self.prefix_tokens.max(other.prefix_tokens);
        self.vocab = self.vocab.max(other.vocab);
        self.maximum_stream_bytes = self.maximum_stream_bytes.max(other.maximum_stream_bytes);
        self.maximum_stream_leaves = self.maximum_stream_leaves.max(other.maximum_stream_leaves);
        self.maximum_eog_bytes = self.maximum_eog_bytes.max(other.maximum_eog_bytes);
        self.maximum_moves = self.maximum_moves.max(other.maximum_moves);
        self.maximum_parser_steps = self.maximum_parser_steps.max(other.maximum_parser_steps);
        self.maximum_blake_blocks = self.maximum_blake_blocks.max(other.maximum_blake_blocks);
        self.inclusion_daa = self.inclusion_daa.max(other.inclusion_daa);
        self.rent_sompi = self.rent_sompi.max(other.rent_sompi);
        self.deposit_sompi = self.deposit_sompi.max(other.deposit_sompi);
        self.row_bytes = self.row_bytes.max(other.row_bytes);
        self.resident_ram_bytes = self.resident_ram_bytes.max(other.resident_ram_bytes);
        self
    }
    /// Sequential required predicates consume independent work and retain their
    /// immutable evidence together. A component-wise maximum does not fund them.
    pub fn checked_sum_v1(mut self, other: Self) -> Result<Self, String> {
        self.prefix_tokens = self.prefix_tokens.max(other.prefix_tokens);
        self.vocab = self.vocab.max(other.vocab);
        macro_rules! add {
            ($($field:ident),+ $(,)?) => { $(
                self.$field = self.$field.checked_add(other.$field)
                    .ok_or_else(|| format!("required-family sum overflows {}", stringify!($field)))?;
            )+ };
        }
        add!(maximum_stream_bytes, maximum_stream_leaves, maximum_eog_bytes, maximum_moves,
            maximum_parser_steps, maximum_blake_blocks, inclusion_daa, rent_sompi,
            deposit_sompi, row_bytes, resident_ram_bytes);
        Ok(self)
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
    #[test]
    fn allocation_coverage_checks_every_geometry_work_capital_and_memory_dimension() {
        let plan = PalwRenderingResourcePlanV1::derive(4, 17, prices()).unwrap();
        assert!(plan.covers_v1(plan));
        macro_rules! reject_deficit {($($field:ident),+ $(,)?)=>{$(
            let mut short=plan;short.$field-=1;
            assert!(!short.covers_v1(plan),"{} is not an optional allocation dimension",stringify!($field));
        )+};}
        reject_deficit!(
            prefix_tokens,
            vocab,
            maximum_stream_bytes,
            maximum_stream_leaves,
            maximum_eog_bytes,
            maximum_moves,
            maximum_parser_steps,
            maximum_blake_blocks,
            inclusion_daa,
            rent_sompi,
            deposit_sompi,
            row_bytes,
            resident_ram_bytes
        );
    }
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
    /// A paid owner may enlarge future work only before the ORIGINAL opening deadline.
    /// The opening/acceptance clocks never move, so no replay buys a later opening door.
    pub fn extend_paid_work_v1(&self, now: u64, actual: PalwRenderingResourcePlanV1) -> Result<Self, String> {
        if now < self.accepted_daa || now >= self.open_until_daa {
            return Err("paid work extension is outside immutable opening clock".into());
        }
        let collection = self.collect_until_daa.checked_sub(self.stream_until_daa).ok_or("bad collection clock")?;
        let retention = self.material_until_daa.checked_sub(self.collect_until_daa).ok_or("bad retention clock")?;
        let exit = self.bond_exit_after_daa.checked_sub(self.material_until_daa).ok_or("bad exit clock")?;
        let plan = self.plan.cover(actual);
        let stream = self.stream_until_daa.max(plan.deadline(self.open_until_daa)?);
        let collect = stream.checked_add(collection).ok_or("paid collection extension overflows")?;
        let material = collect.checked_add(retention).ok_or("paid material extension overflows")?;
        let bond_exit = material.checked_add(exit).ok_or("paid participant exit extension overflows")?;
        Ok(Self {
            plan,
            stream_until_daa: stream,
            collect_until_daa: collect,
            material_until_daa: material,
            bond_exit_after_daa: bond_exit,
            ..*self
        })
    }
    /// Acceptance must use these prospective terms for EVERY liable producer/Panel bond and
    /// each mandatory public material row; a shorter ordinary clock cannot silently substitute.
    pub fn check_holds(&self, material_until: u64, producer_exit_after: u64, panel_exit_after: &[u64]) -> Result<(), String> {
        if panel_exit_after.is_empty()
            || material_until < self.material_until_daa
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
    fn paid_extension_preserves_first_opening_clock_and_monotonic_future_work() {
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let small = PalwTokenizerResourcePlanV1::derive_with_key_bound(16, 0, config.prices)
            .unwrap()
            .court_envelope(17, config.prices)
            .unwrap();
        let large = PalwTokenizerResourcePlanV1::derive_with_key_bound(1 << 30, 1024, config.prices)
            .unwrap()
            .court_envelope(17, config.prices)
            .unwrap();
        let old = PalwRenderingClaimTermsV1::derive(100, small, 100, 100, 20).unwrap();
        assert!(old.extend_paid_work_v1(99, large).is_err());
        assert!(old.extend_paid_work_v1(old.open_until_daa, large).is_err());
        assert!(old.extend_paid_work_v1(old.open_until_daa + 1, large).is_err());
        let new = old.extend_paid_work_v1(old.open_until_daa - 1, large).unwrap();
        assert_eq!((new.accepted_daa, new.open_until_daa), (old.accepted_daa, old.open_until_daa));
        assert!(new.stream_until_daa > old.stream_until_daa);
        assert!(new.collect_until_daa > old.collect_until_daa);
        assert!(new.material_until_daa > old.material_until_daa);
        assert!(new.bond_exit_after_daa > old.bond_exit_after_daa);
        assert!(new.check_holds(old.material_until_daa, old.bond_exit_after_daa, &[old.bond_exit_after_daa; 5]).is_err());
        assert!(new.check_holds(new.material_until_daa, new.bond_exit_after_daa, &[new.bond_exit_after_daa; 5]).is_ok());
        assert_eq!(new.extend_paid_work_v1(100, small).unwrap(), new);
        let mut impossible = large;
        impossible.inclusion_daa = u64::MAX;
        assert!(old.extend_paid_work_v1(100, impossible).is_err());
    }
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

/// Normative prospective prices/windows. A dormant chain supplies None in fold extras.
/// These terms stamp future claims; they never reinterpret an earlier claim's obligations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedPublicCourtConfigV1 {
    pub prices: PalwRenderingResourcePricesV1,
    pub acquisition_daa: u64,
    pub collection_daa: u64,
    pub reorg_retention_daa: u64,
    pub maximum_owned_rows_per_reserver: u32,
}
impl PalwStagedPublicCourtConfigV1 {
    /// Independently fingerprinted normative future contract; caller arms only its own fence.
    pub const fn network_v1() -> Self {
        Self {
            prices: PalwRenderingResourcePricesV1 {
                inclusion_margin_daa: 4,
                rent_per_stream_byte: 1,
                rent_per_parser_step: 1,
                rent_per_row_byte: 1,
                deposit_per_move: 1,
                reorg_replay_moves: 64,
            },
            acquisition_daa: 4_096,
            collection_daa: 4_096,
            reorg_retention_daa: 256,
            maximum_owned_rows_per_reserver: 1,
        }
    }
    pub fn check(&self) -> Result<(), String> {
        if self.prices.inclusion_margin_daa == 0
            || self.prices.rent_per_stream_byte == 0
            || self.prices.rent_per_parser_step == 0
            || self.prices.rent_per_row_byte == 0
            || self.prices.deposit_per_move == 0
            || self.acquisition_daa == 0
            || self.collection_daa == 0
            || self.maximum_owned_rows_per_reserver == 0
        {
            return Err("prospective court needs positive priced work, finite windows and owner capacity".into());
        }
        Ok(())
    }
    pub fn claim_terms(&self, accepted_daa: u64, prefix_tokens: u32, vocab: u32) -> Result<PalwRenderingClaimTermsV1, String> {
        self.check()?;
        let plan = PalwRenderingResourcePlanV1::derive(prefix_tokens, vocab, self.prices)?;
        PalwRenderingClaimTermsV1::derive(accepted_daa, plan, self.acquisition_daa, self.collection_daa, self.reorg_retention_daa)
    }
}

/// Exact signed raw tokenizer length is a prospective PREIMAGE OBLIGATION, not evidence that
/// any prefix is genuine. The raw legacy length-prefixed hash must authenticate EOF before
/// derivation can expose a result. An owner unable to answer that exact preimage defaults DA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTokenizerResourcePlanV1 {
    pub exact_raw_bytes: u64,
    pub maximum_key_proofs: u64,
    pub maximum_moves: u64,
    pub maximum_carrier_bytes: u64,
    pub maximum_parser_steps: u64,
    pub maximum_blake_blocks: u64,
    pub inclusion_daa: u64,
    pub rent_sompi: u64,
    pub deposit_sompi: u64,
    pub resident_ram_bytes: u64,
}
impl PalwTokenizerResourcePlanV1 {
    /// Complete raw derivation plus one largest legal ORIGINAL table rendering. The declared
    /// table length is not trusted to reduce this envelope before its old flat leaf authenticates.
    pub fn court_envelope(self, vocab: u32, p: PalwRenderingResourcePricesV1) -> Result<PalwRenderingResourcePlanV1, String> {
        let table = crate::palw_fp_constraint_job_v1::PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 as u128;
        let table_moves = table.div_ceil(65536) + 2;
        let moves = u128::from(self.maximum_moves) + table_moves;
        let row = 65536u128 + 2048;
        let seed = crate::palw_terminal_carriage_v1::PALW_TERMINAL_MAX_BYTES_V1 as u128;
        let table_carriers = table + table_moves * 5632 + seed;
        Ok(PalwRenderingResourcePlanV1 {
            prefix_tokens: 0,
            vocab,
            maximum_stream_bytes: narrow(u128::from(self.exact_raw_bytes) + table)?,
            maximum_stream_leaves: 1,
            maximum_eog_bytes: 0,
            maximum_moves: narrow(moves)?,
            maximum_parser_steps: self.maximum_parser_steps,
            maximum_blake_blocks: narrow(u128::from(self.maximum_blake_blocks) + table.div_ceil(128) * 2 + 8)?,
            inclusion_daa: narrow(moves * u128::from(p.inclusion_margin_daa))?,
            rent_sompi: narrow(
                u128::from(self.rent_sompi)
                    + table_carriers * u128::from(p.rent_per_stream_byte)
                    + row * u128::from(p.rent_per_row_byte),
            )?,
            deposit_sompi: narrow(moves * u128::from(p.deposit_per_move))?,
            row_bytes: narrow(row)?,
            resident_ram_bytes: self.resident_ram_bytes.checked_add(2048).ok_or("tokenizer RAM overflow")?,
        })
    }
    pub fn derive(exact_raw_bytes: u64, p: PalwRenderingResourcePricesV1) -> Result<Self, String> {
        Self::derive_with_key_bound(exact_raw_bytes, exact_raw_bytes, p)
    }
    /// The signed bound is normative, not a claim that the file is small. At the B+1
    /// parser-derived event the court switches to raw-hash completion and only exposes
    /// objective overrun after authenticated EOF. Thus an understated bound cannot strand
    /// acquisition or grant semantic acceptance without doing the corresponding work.
    pub fn derive_with_key_bound(
        exact_raw_bytes: u64,
        signed_key_bound: u64,
        p: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        if p.inclusion_margin_daa == 0 {
            return Err("tokenizer obligation requires finite positive inclusion margin".into());
        }
        let raw = u128::from(exact_raw_bytes);
        let proofs = raw.min(u128::from(signed_key_bound)); // Each proved event consumes raw bytes.
        // One extra bounded move records the actual B+1 pending event before hash-only EOF.
        let overrun_move = u128::from(signed_key_bound < exact_raw_bytes);
        let part = 65_536u128;
        // Every key pause can split a raw part. Include seed/seal/result and finite reorg replay.
        let moves = raw.div_ceil(part) + 2 * proofs + 3 + overrun_move + u128::from(p.reorg_replay_moves);
        // All512 hash siblings retain full consensus collision security. Include signature/header
        // for EACH fragmented raw or key carrier rather than pricing just the raw model file.
        let carriers = raw + proofs * (512 * 64 + 512) + moves * 5_632;
        // Fixed JSON depth127 and incremental key/UTF8 grammar. Both membership verdicts and
        // an insertion walk use at most1536 keyed nodes;6 blocks/node is conservative.
        let parser_steps = raw * 256 + proofs * 1_536;
        let blake_blocks = raw.div_ceil(128) * 8 + proofs * 9_216 + 32;
        let resident = 196_608u128; // <64KiB parser +65KiB rawpart +33KiB proof +hash/key scratch.
        let rent = carriers * u128::from(p.rent_per_stream_byte)
            + parser_steps * u128::from(p.rent_per_parser_step)
            + resident * u128::from(p.rent_per_row_byte);
        Ok(Self {
            exact_raw_bytes,
            maximum_key_proofs: narrow(proofs)?,
            maximum_moves: narrow(moves)?,
            maximum_carrier_bytes: narrow(carriers)?,
            maximum_parser_steps: narrow(parser_steps)?,
            maximum_blake_blocks: narrow(blake_blocks)?,
            inclusion_daa: narrow(moves * u128::from(p.inclusion_margin_daa))?,
            rent_sompi: narrow(rent)?,
            deposit_sompi: narrow(moves * u128::from(p.deposit_per_move))?,
            resident_ram_bytes: narrow(resident)?,
        })
    }
}
#[cfg(test)]
mod tokenizer_resource_tests {
    use super::*;
    #[test]
    fn signed_event_bound_prices_complete_honest_or_authenticated_overrun_route() {
        let prices = PalwStagedPublicCourtConfigV1::network_v1().prices;
        let raw = 256 * 1024 * 1024;
        let bounded = PalwTokenizerResourcePlanV1::derive_with_key_bound(raw, 1024, prices).unwrap();
        let legacy = PalwTokenizerResourcePlanV1::derive(raw, prices).unwrap();
        assert_eq!(bounded.maximum_key_proofs, 1024);
        assert!(bounded.maximum_carrier_bytes < 400_000_000);
        assert!(bounded.inclusion_daa < 30_000);
        assert!(bounded.maximum_moves < legacy.maximum_moves);
        let none = PalwTokenizerResourcePlanV1::derive_with_key_bound(raw, 0, prices).unwrap();
        assert_eq!(none.maximum_key_proofs, 0);
        assert!(none.maximum_carrier_bytes >= raw);
        // Huge overstatement cannot require more key proofs than exist in the raw preimage.
        assert_eq!(PalwTokenizerResourcePlanV1::derive_with_key_bound(1, u64::MAX, prices).unwrap().maximum_key_proofs, 1);
    }
    #[test]
    fn raw_flat_hash_is_not_assigned_an_invented_file_cap_and_all_key_carriers_are_priced() {
        let prices = PalwStagedPublicCourtConfigV1::network_v1().prices;
        let plan = PalwTokenizerResourcePlanV1::derive(256 * 1024 * 1024, prices).unwrap();
        assert_eq!(plan.maximum_key_proofs, 256 * 1024 * 1024);
        assert!(plan.maximum_carrier_bytes > 8_000_000_000_000);
        assert!(plan.resident_ram_bytes < 256 * 1024);
        assert!(plan.inclusion_daa > 2_000_000_000);
        assert!(PalwTokenizerResourcePlanV1::derive(u64::MAX, prices).is_err());
        assert!(
            PalwTokenizerResourcePlanV1::derive(10, PalwRenderingResourcePricesV1 { rent_per_parser_step: u64::MAX, ..prices })
                .is_err()
        );
        let smaller = PalwTokenizerResourcePlanV1::derive(1, prices).unwrap();
        assert!(smaller.maximum_moves < plan.maximum_moves);
        // This is only a finite prospective obligation; no result authenticates before hash EOF.
        assert_eq!(PalwTokenizerResourcePlanV1::derive(0, prices).unwrap().maximum_key_proofs, 0);
    }
}
