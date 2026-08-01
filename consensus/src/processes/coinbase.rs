use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet,
    coinbase::*,
    config::params::ForkedParam,
    dns_finality::{
        FeeSplitParams, STAKE_SCORE_SCALE, split_block_reward, split_block_subsidy, split_finality_fees, split_normal_tx_fees,
        worker_inclusion_bounty,
    },
    errors::coinbase::{CoinbaseError, CoinbaseResult},
    subnets,
    tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionOutput},
};
use std::convert::TryInto;

use crate::{constants, model::stores::ghostdag::GhostdagData};

const LENGTH_OF_BLUE_SCORE: usize = size_of::<u64>();
const LENGTH_OF_SUBSIDY: usize = size_of::<u64>();
const LENGTH_OF_SCRIPT_PUB_KEY_VERSION: usize = size_of::<u16>();
const LENGTH_OF_SCRIPT_PUB_KEY_LENGTH: usize = size_of::<u8>();

const MIN_PAYLOAD_LENGTH: usize =
    LENGTH_OF_BLUE_SCORE + LENGTH_OF_SUBSIDY + LENGTH_OF_SCRIPT_PUB_KEY_VERSION + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH;

// We define a year as 365.25 days and a month as 365.25 / 12 = 30.4375
// SECONDS_PER_MONTH = 30.4375 * 24 * 60 * 60
const SECONDS_PER_MONTH: u64 = 2629800;

// kaspa-pq emission: 30 years of additional issuance (360 months) + a
// terminal 0 entry marking the end of issuance.
pub const SUBSIDY_BY_MONTH_TABLE_SIZE: usize = 361;
pub type SubsidyByMonthTable = [u64; SUBSIDY_BY_MONTH_TABLE_SIZE];

#[derive(Clone)]
pub struct CoinbaseManager {
    coinbase_payload_script_public_key_max_len: u8,
    max_coinbase_payload_len: usize,
    deflationary_phase_daa_score: u64,
    pre_deflationary_phase_base_subsidy: u64,
    bps_history: ForkedParam<u64>,

    /// Precomputed subsidy by month tables (for before and after the Crescendo hardfork)
    subsidy_by_month_table_before: SubsidyByMonthTable,
    subsidy_by_month_table_after: SubsidyByMonthTable,

    /// The crescendo activation DAA score where BPS increased from 1 to 10.
    /// This score is required here long-term (and not only for the actual forking), in
    /// order to correctly determine the subsidy month from the live DAA score of the network   
    crescendo_activation_daa_score: u64,
}

/// Struct used to streamline payload parsing
struct PayloadParser<'a> {
    remaining: &'a [u8], // The unparsed remainder
}

impl<'a> PayloadParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { remaining: data }
    }

    /// Returns a slice with the first `n` bytes of `remaining`, while setting `remaining` to the remaining part
    fn take(&mut self, n: usize) -> &[u8] {
        let (segment, remaining) = self.remaining.split_at(n);
        self.remaining = remaining;
        segment
    }
}

impl CoinbaseManager {
    pub fn new(
        coinbase_payload_script_public_key_max_len: u8,
        max_coinbase_payload_len: usize,
        deflationary_phase_daa_score: u64,
        pre_deflationary_phase_base_subsidy: u64,
        bps_history: ForkedParam<u64>,
    ) -> Self {
        // Precomputed subsidy by month table for the actual block per second rate
        // Here values are rounded up so that we keep the same number of rewarding months as in the original 1 BPS table.
        // In a 10 BPS network, the induced increase in total rewards is 51 KAS (see tests::calc_high_bps_total_rewards_delta())
        let subsidy_by_month_table_before: SubsidyByMonthTable =
            core::array::from_fn(|i| SUBSIDY_BY_MONTH_TABLE[i].div_ceil(bps_history.before()));
        let subsidy_by_month_table_after: SubsidyByMonthTable =
            core::array::from_fn(|i| SUBSIDY_BY_MONTH_TABLE[i].div_ceil(bps_history.after()));
        Self {
            coinbase_payload_script_public_key_max_len,
            max_coinbase_payload_len,
            deflationary_phase_daa_score,
            pre_deflationary_phase_base_subsidy,
            bps_history,
            subsidy_by_month_table_before,
            subsidy_by_month_table_after,
            crescendo_activation_daa_score: bps_history.activation().daa_score(),
        }
    }

    #[cfg(test)]
    #[inline]
    pub fn bps(&self) -> ForkedParam<u64> {
        self.bps_history
    }

    pub fn expected_coinbase_transaction<T: AsRef<[u8]>>(
        &self,
        daa_score: u64,
        miner_data: MinerData<T>,
        ghostdag_data: &GhostdagData,
        mergeset_rewards: &BlockHashMap<BlockRewardData>,
        mergeset_non_daa: &BlockHashSet,
        // kaspa-pq Phase 10/11 (ADR-0013 / ADR-0009 Addendum B §B.5): validator
        // reward outputs, pre-computed by the caller from the block's included
        // attestations resolved against its selected-parent bond view, in
        // canonical order. Appended verbatim after the miner outputs. The overlay
        // is genesis-active on every current network (`dns_activation_daa_score` = 0),
        // so these are populated and the coinbase carries them from block 1.
        validator_reward_outputs: &[TransactionOutput],
        // kaspa-pq Phase 13 (ADR-0018 §F): when `Some`, carve each source block's
        // reward into Worker / Validator / Service shares and pay only the Worker
        // share to the miner. The Validator share funds the appended
        // `validator_reward_outputs` (the §E distribution); the Service share and
        // the undistributed validator remainder are burned by don't-mint. `None`
        // → the pre-carve behavior (full subsidy+fees to the miner). The carve
        // applies from genesis on every current network (the caller passes `Some`
        // past `dns_activation_daa_score`, = 0 everywhere today).
        carve: Option<&FeeSplitParams>,
        // kaspa-pq Phase 13 (ADR-0018 §D base inclusion bounty): `(newly_included_stake,
        // expected_stake)` for this block — the stake of attestations it newly includes
        // (caller-computed, post-dedup) and the epoch's expected active stake. When
        // carving, the §D worker-inclusion sub-pool (8% of subsidy) is NOT paid to the
        // source-block miners; instead a stake-proportional bounty goes to THIS block's
        // miner (the includer), unspent remainder burned. `(0, _)` → no bounty. Ignored
        // when `carve` is `None`.
        inclusion: (u128, u128),
    ) -> CoinbaseResult<CoinbaseTransactionTemplate> {
        // §D base inclusion bounty: the worker-inclusion sub-pool summed over the SAME
        // mergeset blue(∩DAA)+red iteration the Worker carve uses (paid to the includer below).
        let mut worker_inclusion_pool = 0u64;
        let mut outputs = Vec::with_capacity(ghostdag_data.mergeset_blues.len() + 1); // + 1 for possible red reward
        let mut miner_script_output_indices = Vec::with_capacity(2); // red reward + optional inclusion bounty

        // Add an output for each mergeset blue block (∩ DAA window), paying to the script reported by the block.
        // Note that combinatorically it is nearly impossible for a blue block to be non-DAA
        for blue in ghostdag_data.mergeset_blues.iter().filter(|h| !mergeset_non_daa.contains(h)) {
            let reward_data = mergeset_rewards.get(blue).unwrap();
            match &reward_data.work_reward_class {
                // algo-3 hash-floor source: the whole Worker share (base + tx-fee worker) is paid to
                // the single source miner — the pre-PALW behavior, kept byte-identical.
                WorkRewardClass::HashMiner => {
                    // §F carve: pay the Worker share EXCLUDING the §D worker-inclusion sub-pool
                    // (carved into `worker_inclusion_pool`, paid to the includer below); else full.
                    // Fees split per class: normal-tx fees at the 90/10 normal ratios, the
                    // finality-class subset (bridge txs, ADR-0018 §F wiring) at the validator-primary
                    // finality ratios — mirroring `split_block_reward` exactly so the Worker carve and
                    // the §E validator pool never drift.
                    let value = match carve {
                        Some(fs) => {
                            let s = split_block_subsidy(reward_data.subsidy, fs);
                            worker_inclusion_pool = worker_inclusion_pool.saturating_add(s.worker_inclusion_sompi);
                            let finality = reward_data.finality_fees.min(reward_data.total_fees);
                            s.worker_base_sompi
                                .saturating_add(split_normal_tx_fees(reward_data.total_fees - finality, fs).worker_sompi)
                                .saturating_add(split_finality_fees(finality, fs).worker_sompi)
                        }
                        None => reward_data.subsidy + reward_data.total_fees,
                    };
                    if value > 0 {
                        outputs.push(TransactionOutput::new(value, reward_data.script_public_key.clone()));
                    }
                }
                // ADR-0039 §17.2/§17.3: algo-4 PALW unique-blue source. The subsidy base (77%) splits
                // between the two providers' one-time scripts (A 38.5% / B 38.5%); the tx-fee Worker
                // share goes to the block assembler (the source coinbase script); inclusion 8% joins
                // the pool exactly as above. Validator 15% is accounted by `coinbase_validator_pool`
                // under the SAME PALW-lane split, so the base/validator never sum past the subsidy.
                // The carve is always present here (PALW activates strictly after DNS finality); this
                // arm is unreachable while the lane is inert (no algo-4 header is minted).
                WorkRewardClass::ReplicaPalw { provider_a_script, provider_b_script, premium_pi_bps, .. } => {
                    let fs = carve.expect("PALW lane requires the DNS fee-split carve (DNS active)");
                    let palw = fs.palw_lane();
                    let s = split_block_subsidy(reward_data.subsidy, &palw);
                    worker_inclusion_pool = worker_inclusion_pool.saturating_add(s.worker_inclusion_sompi);
                    // ADR-0040 §16′: the base splits by the replica premium π rather than a fixed half.
                    //
                    //     σ_A = 1/(1 + m·π)     σ_B = π/(1 + m·π)
                    //
                    // π is a single epoch-state scalar, frozen at the leaf's commit window (NOT at payout
                    // time), so a leaf's split is fixed the moment it is ordered and cannot be re-aimed by
                    // whoever later merges it. At π = 1 this is an equal (1+m)-way split, and for m = 1 the
                    // integer arithmetic reproduces the previous `a = base/2; b = base - a` **byte for
                    // byte** — so a net sitting at the neutral point pays exactly what it paid before.
                    //
                    // Safe to make dynamic because the split ratio is invariant under collusion economics:
                    // in a self-collusion attack the attacker takes the leaf's whole value, so moving A:B
                    // changes forgery EV by zero. The reroll wall, the escrow anchor and the audit wall are
                    // all orthogonal to it (see `palw_premium`).
                    let (a, b, b_remainder) = kaspa_consensus_core::palw_premium::premium_split(
                        s.worker_base_sompi,
                        1, // v1 leaves carry exactly one replica (A + B); LeafV2 carries `replica_count`
                        *premium_pi_bps,
                    );
                    // With m = 1 the remainder folds into the single replica, preserving `a + b == base`.
                    let b = b + b_remainder;
                    if a > 0 {
                        outputs.push(TransactionOutput::new(a, provider_a_script.clone()));
                    }
                    if b > 0 {
                        outputs.push(TransactionOutput::new(b, provider_b_script.clone()));
                    }
                    // tx-fee Worker share → the block assembler (source coinbase script), §17.1.
                    let finality = reward_data.finality_fees.min(reward_data.total_fees);
                    let fee_worker = split_normal_tx_fees(reward_data.total_fees - finality, &palw)
                        .worker_sompi
                        .saturating_add(split_finality_fees(finality, &palw).worker_sompi);
                    if fee_worker > 0 {
                        outputs.push(TransactionOutput::new(fee_worker, reward_data.script_public_key.clone()));
                    }
                }
                // K5 (ADR-0039 §11.3): an algo-4 source minted under a HALTED beacon is paid NOTHING —
                // no provider outputs, no fee-worker output, no inclusion-pool add (the source did work
                // under an untrusted beacon; §17.4 burn-by-don't-mint, never rerouted to the miner).
                //
                // ADR-0040 §5.15.13 (G16): a DUPLICATE-WORK source is paid NOTHING by the SAME
                // arithmetic — the identical computation is not monetised twice. The unminted base is
                // NOT rerouted to the merging miner, or duplicate-work spam would pay the includer.
                WorkRewardClass::ReplicaPalwHalted { .. }
                | WorkRewardClass::ReplicaPalwDuplicateWork { .. }
                // ADR-0040 ECON-03 (THE WIRE): a leaf whose provider bonds do not resolve to active
                // collateral is paid exactly like Halted/duplicate work — nothing, on every axis.
                | WorkRewardClass::ReplicaPalwUnbackedCollateral { .. } => {}
            }
        }

        // Collect all rewards from mergeset reds ∩ DAA window and create a
        // single output rewarding all to the current block (the "merging" block)
        let mut red_reward = 0u64;

        for red in ghostdag_data.mergeset_reds.iter() {
            let reward_data = mergeset_rewards.get(red).unwrap();
            // Reds ∩ DAA earn subsidy + fees; non-DAA reds earn fees only (both fee classes kept).
            let (eff_subsidy, eff_fees) = if mergeset_non_daa.contains(red) {
                (0, reward_data.total_fees)
            } else {
                (reward_data.subsidy, reward_data.total_fees)
            };
            match &reward_data.work_reward_class {
                WorkRewardClass::HashMiner => {
                    // §F carve: accumulate the Worker share EXCLUDING the §D inclusion sub-pool; else full.
                    // Per-class fee split mirrors the blues loop above (and `split_block_reward`).
                    red_reward += match carve {
                        Some(fs) => {
                            let s = split_block_subsidy(eff_subsidy, fs);
                            worker_inclusion_pool = worker_inclusion_pool.saturating_add(s.worker_inclusion_sompi);
                            let finality = reward_data.finality_fees.min(eff_fees);
                            s.worker_base_sompi
                                .saturating_add(split_normal_tx_fees(eff_fees - finality, fs).worker_sompi)
                                .saturating_add(split_finality_fees(finality, fs).worker_sompi)
                        }
                        None => eff_subsidy + eff_fees,
                    };
                }
                // ADR-0039 §17.4: a red / duplicate PALW source pays the provider pair nothing, and the
                // unminted base is NOT redistributed to the current miner (unissued / security reserve)
                // — else duplicate-block spam would pay the includer. It contributes nothing to the red
                // reward or the inclusion pool. The exact red-PALW treatment is finalized alongside the
                // nullifier-dedup coloring (§15.3); inert never produces a PALW red.
                WorkRewardClass::ReplicaPalw { .. } => {}
                // K5: a halted-epoch algo-4 red source pays nothing, exactly like the red ReplicaPalw
                // arm — and so does a G16 duplicate-work source (ADR-0040 §5.15.13).
                WorkRewardClass::ReplicaPalwHalted { .. }
                | WorkRewardClass::ReplicaPalwDuplicateWork { .. }
                // ADR-0040 ECON-03 (THE WIRE): a leaf whose provider bonds do not resolve to active
                // collateral is paid exactly like Halted/duplicate work — nothing, on every axis.
                | WorkRewardClass::ReplicaPalwUnbackedCollateral { .. } => {}
            }
        }

        if red_reward > 0 {
            miner_script_output_indices.push(outputs.len());
            outputs.push(TransactionOutput::new(red_reward, miner_data.script_public_key.clone()));
        }

        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.5): append the
        // validator-side reward outputs after all miner outputs, in the
        // caller-supplied canonical order. Empty while no validator is bonded
        // (the bootstrap state): with no §E recipients the whole validator carve
        // is the "unspent remainder" and is burned by don't-mint — a deliberate
        // bootstrap-period supply reduction (no minting without recipients). Once
        // validators bond, this carries their participation payouts.
        outputs.extend_from_slice(validator_reward_outputs);

        // kaspa-pq Phase 13 (ADR-0018 §D base inclusion bounty): pay THIS block's miner
        // (the includer) a stake-proportional share of the §D worker-inclusion pool for
        // the attestation stake it newly includes, against the epoch's expected stake. No
        // urgency multiplier (1.0×) and no quality-gate bonus yet (those need the
        // epoch-cumulative accumulator). The unspent remainder is burned (don't-mint).
        // Inert when `carve` is `None` (the pool stays 0 and this is skipped).
        if carve.is_some() {
            let (newly_included_stake, expected_stake) = inclusion;
            let bounty = worker_inclusion_bounty(
                worker_inclusion_pool as u128,
                newly_included_stake,
                expected_stake,
                STAKE_SCORE_SCALE,
                false,
                0,
            )
            .min(worker_inclusion_pool as u128) as u64;
            if bounty > 0 {
                miner_script_output_indices.push(outputs.len());
                outputs.push(TransactionOutput::new(bounty, miner_data.script_public_key.clone()));
            }
        }

        // Build the current block's payload
        let subsidy = self.calc_block_subsidy(daa_score);
        let payload = self.serialize_coinbase_payload(&CoinbaseData { blue_score: ghostdag_data.blue_score, subsidy, miner_data })?;

        Ok(CoinbaseTransactionTemplate {
            tx: Transaction::new(constants::TX_VERSION, vec![], outputs, 0, subnets::SUBNETWORK_ID_COINBASE, 0, payload),
            has_red_reward: red_reward > 0,
            miner_script_output_indices,
        })
    }

    /// kaspa-pq Phase 13 (ADR-0018 §F/§E): the validator-side pool funded by this
    /// block's coinbase — Σ of the per-source-block Validator share
    /// (`split_block_reward(..).validator_sompi`) over the SAME mergeset
    /// blue(∩DAA) + red iteration [`Self::expected_coinbase_transaction`] carves
    /// the Worker outputs from, so the pool and the carve never drift. Reds use
    /// their effective subsidy (0 when non-DAA) plus fees, exactly as the Worker
    /// carve does. The §E participation distribution draws from this pool; the
    /// result is fed back as `expected_coinbase_transaction`'s
    /// `validator_reward_outputs`. The caller passes `fee_split` only past
    /// `dns_activation_daa_score` (= 0 everywhere today), so this is active from
    /// genesis on every current network.
    pub fn coinbase_validator_pool(
        &self,
        ghostdag_data: &GhostdagData,
        mergeset_rewards: &BlockHashMap<BlockRewardData>,
        mergeset_non_daa: &BlockHashSet,
        fee_split: &FeeSplitParams,
    ) -> u64 {
        let mut pool = 0u64;
        for blue in ghostdag_data.mergeset_blues.iter().filter(|h| !mergeset_non_daa.contains(h)) {
            let reward_data = mergeset_rewards.get(blue).unwrap();
            // ADR-0039 §17.1: a PALW blue source contributes the lane's 15% validator share (not 30%),
            // matching the base 77% paid to the providers in `expected_coinbase_transaction`, so the
            // per-source shares never sum past the subsidy. HashMiner uses the hash-lane split.
            let validator = match &reward_data.work_reward_class {
                WorkRewardClass::HashMiner => {
                    split_block_reward(reward_data.subsidy, reward_data.total_fees, reward_data.finality_fees, fee_split)
                        .validator_sompi
                }
                WorkRewardClass::ReplicaPalw { .. } => {
                    split_block_reward(reward_data.subsidy, reward_data.total_fees, reward_data.finality_fees, &fee_split.palw_lane())
                        .validator_sompi
                }
                // K5: a halted-epoch algo-4 source contributes 0 to the §E validator pool (nothing
                // minted). ADR-0040 §5.15.13 (G16): a duplicate-work source contributes 0 for the same
                // reason — the whole subsidy is unminted, so the validator share must not be minted
                // either or the coinbase would pay out more than the classification withheld.
                WorkRewardClass::ReplicaPalwHalted { .. }
                | WorkRewardClass::ReplicaPalwDuplicateWork { .. }
                // ADR-0040 ECON-03 (THE WIRE): unresolved provider collateral ⇒ zero, same as above.
                | WorkRewardClass::ReplicaPalwUnbackedCollateral { .. } => 0,
            };
            pool = pool.saturating_add(validator);
        }
        for red in ghostdag_data.mergeset_reds.iter() {
            let reward_data = mergeset_rewards.get(red).unwrap();
            let (eff_subsidy, eff_fees) = if mergeset_non_daa.contains(red) {
                (0, reward_data.total_fees)
            } else {
                (reward_data.subsidy, reward_data.total_fees)
            };
            // §17.4: a red / duplicate PALW source is unminted and contributes nothing to the §E pool,
            // mirroring the red handling in `expected_coinbase_transaction`.
            let validator = match &reward_data.work_reward_class {
                WorkRewardClass::HashMiner => {
                    split_block_reward(eff_subsidy, eff_fees, reward_data.finality_fees, fee_split).validator_sompi
                }
                WorkRewardClass::ReplicaPalw { .. } => 0,
                WorkRewardClass::ReplicaPalwHalted { .. }
                | WorkRewardClass::ReplicaPalwDuplicateWork { .. }
                // ADR-0040 ECON-03 (THE WIRE): unresolved provider collateral ⇒ zero, same as above.
                | WorkRewardClass::ReplicaPalwUnbackedCollateral { .. } => 0,
            };
            pool = pool.saturating_add(validator);
        }
        pool
    }

    pub fn serialize_coinbase_payload<T: AsRef<[u8]>>(&self, data: &CoinbaseData<T>) -> CoinbaseResult<Vec<u8>> {
        let script_pub_key_len = data.miner_data.script_public_key.script().len();
        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len as usize {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }
        let payload: Vec<u8> = data.blue_score.to_le_bytes().iter().copied()                    // Blue score                   (u64)
            .chain(data.subsidy.to_le_bytes().iter().copied())                                  // Subsidy                      (u64)
            .chain(data.miner_data.script_public_key.version().to_le_bytes().iter().copied())   // Script public key version    (u16)
            .chain((script_pub_key_len as u8).to_le_bytes().iter().copied())                    // Script public key length     (u8)
            .chain(data.miner_data.script_public_key.script().iter().copied())                  // Script public key            
            .chain(data.miner_data.extra_data.as_ref().iter().copied())                         // Extra data
            .collect();

        Ok(payload)
    }

    pub fn modify_coinbase_payload<T: AsRef<[u8]>>(&self, mut payload: Vec<u8>, miner_data: &MinerData<T>) -> CoinbaseResult<Vec<u8>> {
        let script_pub_key_len = miner_data.script_public_key.script().len();
        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len as usize {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }

        // Keep only blue score and subsidy. Note that truncate does not modify capacity, so
        // the usual case where the payloads are the same size will not trigger a reallocation
        payload.truncate(LENGTH_OF_BLUE_SCORE + LENGTH_OF_SUBSIDY);
        payload.extend(
            miner_data.script_public_key.version().to_le_bytes().iter().copied() // Script public key version (u16)
                .chain((script_pub_key_len as u8).to_le_bytes().iter().copied()) // Script public key length  (u8)
                .chain(miner_data.script_public_key.script().iter().copied())    // Script public key
                .chain(miner_data.extra_data.as_ref().iter().copied()), // Extra data
        );

        Ok(payload)
    }

    pub fn deserialize_coinbase_payload<'a>(&self, payload: &'a [u8]) -> CoinbaseResult<CoinbaseData<&'a [u8]>> {
        if payload.len() < MIN_PAYLOAD_LENGTH {
            return Err(CoinbaseError::PayloadLenBelowMin(payload.len(), MIN_PAYLOAD_LENGTH));
        }

        if payload.len() > self.max_coinbase_payload_len {
            return Err(CoinbaseError::PayloadLenAboveMax(payload.len(), self.max_coinbase_payload_len));
        }

        let mut parser = PayloadParser::new(payload);

        let blue_score = u64::from_le_bytes(parser.take(LENGTH_OF_BLUE_SCORE).try_into().unwrap());
        let subsidy = u64::from_le_bytes(parser.take(LENGTH_OF_SUBSIDY).try_into().unwrap());
        let script_pub_key_version = u16::from_le_bytes(parser.take(LENGTH_OF_SCRIPT_PUB_KEY_VERSION).try_into().unwrap());
        let script_pub_key_len = u8::from_le_bytes(parser.take(LENGTH_OF_SCRIPT_PUB_KEY_LENGTH).try_into().unwrap());

        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len as usize,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }

        if parser.remaining.len() < script_pub_key_len as usize {
            return Err(CoinbaseError::PayloadCantContainScriptPublicKey(
                payload.len(),
                MIN_PAYLOAD_LENGTH + script_pub_key_len as usize,
            ));
        }

        let script_public_key =
            ScriptPublicKey::new(script_pub_key_version, ScriptVec::from_slice(parser.take(script_pub_key_len as usize)));
        let extra_data = parser.remaining;

        Ok(CoinbaseData { blue_score, subsidy, miner_data: MinerData { script_public_key, extra_data } })
    }

    pub fn calc_block_subsidy(&self, daa_score: u64) -> u64 {
        if daa_score < self.deflationary_phase_daa_score {
            return self.pre_deflationary_phase_base_subsidy;
        }

        let subsidy_month = self.subsidy_month(daa_score) as usize;
        let subsidy_table = if self.bps_history.activation().is_active(daa_score) {
            &self.subsidy_by_month_table_after
        } else {
            &self.subsidy_by_month_table_before
        };
        subsidy_table[subsidy_month.min(subsidy_table.len() - 1)]
    }

    /// Get the subsidy month as function of the current DAA score.
    ///
    /// Note that this function is called only if daa_score >= self.deflationary_phase_daa_score
    fn subsidy_month(&self, daa_score: u64) -> u64 {
        let seconds_since_deflationary_phase_started = if self.crescendo_activation_daa_score < self.deflationary_phase_daa_score {
            // crescendo_activation < deflationary_phase <= daa_score (activated before deflation)
            (daa_score - self.deflationary_phase_daa_score) / self.bps_history.after()
        } else if daa_score < self.crescendo_activation_daa_score {
            // deflationary_phase <= daa_score < crescendo_activation (pre activation)
            (daa_score - self.deflationary_phase_daa_score) / self.bps_history.before()
        } else {
            // Else - deflationary_phase <= crescendo_activation <= daa_score.
            // Count seconds differently before and after Crescendo activation
            (self.crescendo_activation_daa_score - self.deflationary_phase_daa_score) / self.bps_history.before()
                + (daa_score - self.crescendo_activation_daa_score) / self.bps_history.after()
        };

        seconds_since_deflationary_phase_started / SECONDS_PER_MONTH
    }
}

/*
    kaspa-pq (misaka MSK) additional-issuance emission table.

    Tokenomics: 16.013224875B MSK of additional issuance over 30 years, decaying
    at a 1.4%/year exponential rate (annual multiplier q = 0.986), on top of a
    10B genesis premine for a ~26.013224875B final supply. The schedule steps
    once per year (12 identical months), so the table holds 30 yearly rates × 12
    months = 360 entries followed by a terminal 0 (issuance ends after year 30).

    Values are the reward per second (= reward per block at 1 BPS); the manager
    divides each by the actual BPS via `div_ceil` at construction. Each yearly
    rate is `round(E_y · SOMPI_PER_KASPA / SECONDS_PER_YEAR)` with
    `E_y = E_1 · 0.986^(y-1)` and `E_1 = 0.65e9 MSK` (the first-year issuance is
    given directly). This yields exactly 2.05972571 MSK/block in year 1 and
    1.36848463 MSK/block in year 30 at 10 BPS, and a 30-year total of
    ≈ 16.013224875B MSK (1-BPS reference: 16_013_224_874 MSK; see
    `verify_total_emission`).

    HONEST CAP NOTE: each per-block reward is an integer sompi rounded UP
    (`div_ceil` by BPS), so the LIVE cumulative issuance is a few tens of MSK
    above the theoretical 16.013224875B (≈ +44 MSK at 10 BPS). `MAX_SOMPI`
    (26_013_224_875 MSK) is the theoretical max-supply figure, NOT a hard cap
    that stops emission — the terminal 0 does that after year 30. A strict cap
    would need explicit cumulative-cutoff logic (see the constants.rs note).

    To regenerate, recompute the 30 yearly rates with the formula above
    (SECONDS_PER_YEAR = 12 · SECONDS_PER_MONTH = 31_557_600) and repeat each 12×.
*/
#[rustfmt::skip]
const SUBSIDY_BY_MONTH_TABLE: [u64; SUBSIDY_BY_MONTH_TABLE_SIZE] = [
    2059725708, 2059725708, 2059725708, 2059725708, 2059725708, 2059725708, 2059725708, 2059725708, 2059725708, 2059725708, 2059725708, 2059725708, 2030889548, 2030889548, 2030889548, 2030889548, 2030889548, 2030889548, 2030889548, 2030889548, 2030889548, 2030889548, 2030889548, 2030889548, 2002457094,
    2002457094, 2002457094, 2002457094, 2002457094, 2002457094, 2002457094, 2002457094, 2002457094, 2002457094, 2002457094, 2002457094, 1974422695, 1974422695, 1974422695, 1974422695, 1974422695, 1974422695, 1974422695, 1974422695, 1974422695, 1974422695, 1974422695, 1974422695, 1946780777, 1946780777,
    1946780777, 1946780777, 1946780777, 1946780777, 1946780777, 1946780777, 1946780777, 1946780777, 1946780777, 1946780777, 1919525846, 1919525846, 1919525846, 1919525846, 1919525846, 1919525846, 1919525846, 1919525846, 1919525846, 1919525846, 1919525846, 1919525846, 1892652485, 1892652485, 1892652485,
    1892652485, 1892652485, 1892652485, 1892652485, 1892652485, 1892652485, 1892652485, 1892652485, 1892652485, 1866155350, 1866155350, 1866155350, 1866155350, 1866155350, 1866155350, 1866155350, 1866155350, 1866155350, 1866155350, 1866155350, 1866155350, 1840029175, 1840029175, 1840029175, 1840029175,
    1840029175, 1840029175, 1840029175, 1840029175, 1840029175, 1840029175, 1840029175, 1840029175, 1814268766, 1814268766, 1814268766, 1814268766, 1814268766, 1814268766, 1814268766, 1814268766, 1814268766, 1814268766, 1814268766, 1814268766, 1788869004, 1788869004, 1788869004, 1788869004, 1788869004,
    1788869004, 1788869004, 1788869004, 1788869004, 1788869004, 1788869004, 1788869004, 1763824838, 1763824838, 1763824838, 1763824838, 1763824838, 1763824838, 1763824838, 1763824838, 1763824838, 1763824838, 1763824838, 1763824838, 1739131290, 1739131290, 1739131290, 1739131290, 1739131290, 1739131290,
    1739131290, 1739131290, 1739131290, 1739131290, 1739131290, 1739131290, 1714783452, 1714783452, 1714783452, 1714783452, 1714783452, 1714783452, 1714783452, 1714783452, 1714783452, 1714783452, 1714783452, 1714783452, 1690776484, 1690776484, 1690776484, 1690776484, 1690776484, 1690776484, 1690776484,
    1690776484, 1690776484, 1690776484, 1690776484, 1690776484, 1667105613, 1667105613, 1667105613, 1667105613, 1667105613, 1667105613, 1667105613, 1667105613, 1667105613, 1667105613, 1667105613, 1667105613, 1643766134, 1643766134, 1643766134, 1643766134, 1643766134, 1643766134, 1643766134, 1643766134,
    1643766134, 1643766134, 1643766134, 1643766134, 1620753408, 1620753408, 1620753408, 1620753408, 1620753408, 1620753408, 1620753408, 1620753408, 1620753408, 1620753408, 1620753408, 1620753408, 1598062861, 1598062861, 1598062861, 1598062861, 1598062861, 1598062861, 1598062861, 1598062861, 1598062861,
    1598062861, 1598062861, 1598062861, 1575689981, 1575689981, 1575689981, 1575689981, 1575689981, 1575689981, 1575689981, 1575689981, 1575689981, 1575689981, 1575689981, 1575689981, 1553630321, 1553630321, 1553630321, 1553630321, 1553630321, 1553630321, 1553630321, 1553630321, 1553630321, 1553630321,
    1553630321, 1553630321, 1531879496, 1531879496, 1531879496, 1531879496, 1531879496, 1531879496, 1531879496, 1531879496, 1531879496, 1531879496, 1531879496, 1531879496, 1510433183, 1510433183, 1510433183, 1510433183, 1510433183, 1510433183, 1510433183, 1510433183, 1510433183, 1510433183, 1510433183,
    1510433183, 1489287119, 1489287119, 1489287119, 1489287119, 1489287119, 1489287119, 1489287119, 1489287119, 1489287119, 1489287119, 1489287119, 1489287119, 1468437099, 1468437099, 1468437099, 1468437099, 1468437099, 1468437099, 1468437099, 1468437099, 1468437099, 1468437099, 1468437099, 1468437099,
    1447878980, 1447878980, 1447878980, 1447878980, 1447878980, 1447878980, 1447878980, 1447878980, 1447878980, 1447878980, 1447878980, 1447878980, 1427608674, 1427608674, 1427608674, 1427608674, 1427608674, 1427608674, 1427608674, 1427608674, 1427608674, 1427608674, 1427608674, 1427608674, 1407622153,
    1407622153, 1407622153, 1407622153, 1407622153, 1407622153, 1407622153, 1407622153, 1407622153, 1407622153, 1407622153, 1407622153, 1387915442, 1387915442, 1387915442, 1387915442, 1387915442, 1387915442, 1387915442, 1387915442, 1387915442, 1387915442, 1387915442, 1387915442, 1368484626, 1368484626,
    1368484626, 1368484626, 1368484626, 1368484626, 1368484626, 1368484626, 1368484626, 1368484626, 1368484626, 1368484626, 0,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::MAINNET_PARAMS;
    use kaspa_consensus_core::{
        config::params::{Params, SIMNET_PARAMS},
        constants::SOMPI_PER_KASPA,
        network::NetworkId,
        tx::scriptvec,
    };

    #[test]
    fn calc_high_bps_total_rewards_delta() {
        let legacy_cbm = create_legacy_manager();
        let pre_deflationary_rewards = legacy_cbm.pre_deflationary_phase_base_subsidy * legacy_cbm.deflationary_phase_daa_score;
        let total_rewards: u64 = pre_deflationary_rewards + SUBSIDY_BY_MONTH_TABLE.iter().map(|x| x * SECONDS_PER_MONTH).sum::<u64>();
        let testnet_11_bps = SIMNET_PARAMS.bps();
        let total_high_bps_rewards_rounded_up: u64 = pre_deflationary_rewards
            + SUBSIDY_BY_MONTH_TABLE.iter().map(|x| (x.div_ceil(testnet_11_bps) * testnet_11_bps) * SECONDS_PER_MONTH).sum::<u64>();

        let cbm = create_manager(&SIMNET_PARAMS);
        let total_high_bps_rewards: u64 = pre_deflationary_rewards
            + cbm.subsidy_by_month_table_before.iter().map(|x| x * SECONDS_PER_MONTH * cbm.bps().before()).sum::<u64>();
        assert_eq!(total_high_bps_rewards_rounded_up, total_high_bps_rewards, "subsidy adjusted to bps must be rounded up");

        let delta = total_high_bps_rewards as i64 - total_rewards as i64;

        println!("Total rewards: {} sompi => {} KAS", total_rewards, total_rewards / SOMPI_PER_KASPA);
        println!("Total high bps rewards: {} sompi => {} KAS", total_high_bps_rewards, total_high_bps_rewards / SOMPI_PER_KASPA);
        println!("Delta: {} sompi => {} KAS", delta, delta / SOMPI_PER_KASPA as i64);
    }

    #[test]
    fn subsidy_by_month_table_test() {
        let cbm = create_legacy_manager();
        cbm.subsidy_by_month_table_before.iter().enumerate().for_each(|(i, x)| {
            assert_eq!(SUBSIDY_BY_MONTH_TABLE[i], *x, "for 1 BPS, const table and precomputed values must match");
        });

        for network_id in NetworkId::iter() {
            let cbm = create_manager(&network_id.into());
            cbm.subsidy_by_month_table_before.iter().enumerate().for_each(|(i, x)| {
                assert_eq!(
                    SUBSIDY_BY_MONTH_TABLE[i].div_ceil(cbm.bps().before()),
                    *x,
                    "{}: locally computed and precomputed values must match",
                    network_id
                );
            });
            cbm.subsidy_by_month_table_after.iter().enumerate().for_each(|(i, x)| {
                assert_eq!(
                    SUBSIDY_BY_MONTH_TABLE[i].div_ceil(cbm.bps().after()),
                    *x,
                    "{}: locally computed and precomputed values must match",
                    network_id
                );
            });
        }
    }

    /// Verifies the kaspa-pq additional-issuance schedule sums to ~16.013224875B
    /// MSK over 30 years. The per-month table holds reward-per-second values, so
    /// the total issuance is `Σ table[m] * SECONDS_PER_MONTH` (BPS-invariant:
    /// higher BPS divides the per-block reward but produces proportionally more
    /// blocks, up to a small `div_ceil` rounding surplus).
    #[test]
    fn verify_total_emission() {
        // 1 BPS reference total (the clean figure the table is derived from).
        let total_sompi: u128 = SUBSIDY_BY_MONTH_TABLE.iter().map(|&x| x as u128 * SECONDS_PER_MONTH as u128).sum();
        let total_kas = total_sompi / SOMPI_PER_KASPA as u128;
        println!("kaspa-pq additional issuance: {total_sompi} sompi => {total_kas} MSK");

        // Theoretical target 16_013_224_875 MSK; the 1-BPS reference floors to
        // 16_013_224_874 (≈0.15 MSK of per-year rounding), so allow ±1 MSK.
        const TARGET_KAS: u128 = 16_013_224_875;
        let delta_kas = TARGET_KAS as i128 - total_kas as i128;
        assert!(delta_kas.abs() <= 1, "additional issuance {total_kas} MSK deviates from 16.013224875B by {delta_kas} MSK");
        // The clean 1 BPS figure stays within the budget; the live network adds
        // only the small div_ceil rounding surplus checked below.
        assert!(total_kas <= TARGET_KAS, "additional issuance {total_kas} MSK exceeds the 16.013224875B budget");

        // Per-network totals differ from the 1 BPS reference only by the per-month
        // div_ceil rounding surplus: at most (bps-1) sompi/month * SECONDS_PER_MONTH *
        // 360 months ≈ 44 MSK at 10 BPS. Negligible against the ~26B supply and far
        // below the MAX_SOMPI cap.
        for network_id in NetworkId::iter() {
            let cbm = create_manager(&network_id.into());
            let bps = Params::from(network_id).bps();
            let net_total: u128 =
                cbm.subsidy_by_month_table_after.iter().map(|&x| x as u128 * SECONDS_PER_MONTH as u128 * bps as u128).sum();
            let surplus_kas = net_total as i128 / SOMPI_PER_KASPA as i128 - total_kas as i128;
            assert!((0..=128).contains(&surplus_kas), "{network_id}: bps rounding surplus {surplus_kas} MSK out of range");
        }
    }

    #[test]
    fn subsidy_test() {
        // Year-1 per-block subsidy at 10 BPS = table[0].div_ceil(10) = 2.05972571 MSK.
        const YEAR1_PER_BLOCK_10BPS: u64 = 205972571;

        for network_id in NetworkId::iter() {
            let params: Params = network_id.into();
            let cbm = create_manager(&params);
            let bps = params.bps();
            let blocks_per_month = SECONDS_PER_MONTH * bps;

            // kaspa-pq has no flat pre-deflationary phase: the decay table applies from genesis.
            assert_eq!(params.deflationary_phase_daa_score, 0, "{network_id}: expected no pre-deflationary phase");

            // Genesis / year-1 subsidy.
            let expected_year1 = SUBSIDY_BY_MONTH_TABLE[0].div_ceil(bps);
            assert_eq!(cbm.calc_block_subsidy(0), expected_year1, "{network_id}: genesis subsidy");
            if bps == 10 {
                assert_eq!(expected_year1, YEAR1_PER_BLOCK_10BPS, "{network_id}: year-1 per-block subsidy");
            }

            // Every emission month pays table[m].div_ceil(bps), flat within the month
            // (stepped schedule: the same rate holds from the first to the last block of the month).
            // Index-based: `m` is both a table index and a DAA-score multiplier below.
            #[allow(clippy::needless_range_loop)]
            for m in 0..SUBSIDY_BY_MONTH_TABLE_SIZE - 1 {
                let daa = m as u64 * blocks_per_month;
                let expected = SUBSIDY_BY_MONTH_TABLE[m].div_ceil(bps);
                assert_eq!(cbm.calc_block_subsidy(daa), expected, "{network_id}: month {m} start");
                assert_eq!(cbm.calc_block_subsidy(daa + blocks_per_month - 1), expected, "{network_id}: month {m} end");
            }

            // 1.4%/year exponential decay: each year's rate is ~0.986x the previous year's.
            for y in 1..30usize {
                let prev = SUBSIDY_BY_MONTH_TABLE[(y - 1) * 12] as f64;
                let curr = SUBSIDY_BY_MONTH_TABLE[y * 12] as f64;
                let ratio = curr / prev;
                assert!((ratio - 0.986).abs() < 1e-4, "{network_id}: year {y}->{} decay ratio {ratio}", y + 1);
            }

            // Issuance ends after 30 years: month index >= 360 yields zero subsidy.
            let end_daa = (SUBSIDY_BY_MONTH_TABLE_SIZE - 1) as u64 * blocks_per_month;
            assert_eq!(cbm.calc_block_subsidy(end_daa), 0, "{network_id}: end of issuance");
            assert_eq!(cbm.calc_block_subsidy(end_daa + blocks_per_month * 100), 0, "{network_id}: after issuance");
        }
    }

    #[test]
    fn payload_serialization_test() {
        let cbm = create_manager(&MAINNET_PARAMS);

        let script_data = [33u8, 255];
        let extra_data = [2u8, 3];
        let data = CoinbaseData {
            blue_score: 56,
            subsidy: 44000000000,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey::new(0, ScriptVec::from_slice(&script_data)),
                extra_data: &extra_data as &[u8],
            },
        };

        let payload = cbm.serialize_coinbase_payload(&data).unwrap();
        let deserialized_data = cbm.deserialize_coinbase_payload(&payload).unwrap();

        assert_eq!(data, deserialized_data);

        // Test an actual mainnet payload
        let payload_hex =
            "b612c90100000000041a763e07000000000022202b32443ff740012157716d81216d09aebc39e5493c93a7181d92cb756c02c560ac302e31322e382f";
        let mut payload = vec![0u8; payload_hex.len() / 2];
        faster_hex::hex_decode(payload_hex.as_bytes(), &mut payload).unwrap();
        let deserialized_data = cbm.deserialize_coinbase_payload(&payload).unwrap();

        let expected_data = CoinbaseData {
            blue_score: 29954742,
            subsidy: 31112698372,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey::new(
                    0,
                    scriptvec![
                        32, 43, 50, 68, 63, 247, 64, 1, 33, 87, 113, 109, 129, 33, 109, 9, 174, 188, 57, 229, 73, 60, 147, 167, 24,
                        29, 146, 203, 117, 108, 2, 197, 96, 172,
                    ],
                ),
                extra_data: &[48u8, 46, 49, 50, 46, 56, 47] as &[u8],
            },
        };
        assert_eq!(expected_data, deserialized_data);
    }

    /// ADR-0013 Addendum B parity pin: the consensus-core
    /// opcode-literal `p2pkh_mldsa87_spk` (used by the PR-10.5′
    /// coinbase fan-out) must be byte-identical to the canonical
    /// `kaspa_txscript::pay_to_address_script` over the same 64-byte
    /// payload (ADR-0019 §8), and prefix-independent.
    #[test]
    fn validator_reward_spk_matches_pay_to_address_script() {
        use kaspa_addresses::{Address, Prefix, Version};
        use kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk;
        use kaspa_txscript::pay_to_address_script;

        let mut counting = [0u8; 64];
        for (i, b) in counting.iter_mut().enumerate() {
            *b = i as u8;
        }
        for payload in [[0x00u8; 64], [0x11u8; 64], [0xffu8; 64], counting] {
            let core_spk = p2pkh_mldsa87_spk(&payload);
            for prefix in [Prefix::Mainnet, Prefix::Testnet, Prefix::Simnet, Prefix::Devnet] {
                let addr = Address::new(prefix, Version::PubKeyHashMlDsa87, &payload);
                assert_eq!(core_spk, pay_to_address_script(&addr), "prefix {prefix:?} payload {payload:?}");
            }
        }
    }

    #[test]
    fn modify_payload_test() {
        let cbm = create_manager(&MAINNET_PARAMS);

        let script_data = [33u8, 255];
        let extra_data = [2u8, 3, 23, 98];
        let data = CoinbaseData {
            blue_score: 56345,
            subsidy: 44000000000,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey::new(0, ScriptVec::from_slice(&script_data)),
                extra_data: &extra_data,
            },
        };

        let data2 = CoinbaseData {
            blue_score: data.blue_score,
            subsidy: data.subsidy,
            miner_data: MinerData {
                // Modify only miner data
                script_public_key: ScriptPublicKey::new(0, ScriptVec::from_slice(&[33u8, 255, 33])),
                extra_data: &[2u8, 3, 23, 98, 34, 34] as &[u8],
            },
        };

        let mut payload = cbm.serialize_coinbase_payload(&data).unwrap();
        payload = cbm.modify_coinbase_payload(payload, &data2.miner_data).unwrap(); // Update the payload with the modified miner data
        let deserialized_data = cbm.deserialize_coinbase_payload(&payload).unwrap();

        assert_eq!(data2, deserialized_data);
    }

    /// ADR-0039 §17.2/§17.3: an algo-4 PALW unique-blue source pays its subsidy base (77%) to the two
    /// providers (A 38.5% / B 38.5%) and contributes the PALW-lane 15% validator share, while a
    /// hash-floor source is unchanged (62% base, 30% validator). Exercises the otherwise-inert
    /// `ReplicaPalw` arm directly (construction == validation, since both go through this fn).
    #[test]
    fn palw_replica_coinbase_split_and_validator_pool() {
        use kaspa_hashes::Hash64;
        let cbm = create_manager(&MAINNET_PARAMS);
        let spk = |b: u8| ScriptPublicKey::new(0, ScriptVec::from_slice(&[b]));

        // Hash-lane fee split: subsidy 62/8/30, standard fee ratios.
        let fs = FeeSplitParams {
            subsidy_worker_base_bps: 6200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 3000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        };

        // Mergeset: selected parent (hash miner) + one unique-blue PALW source, both subsidy 10_000, no fees.
        let sp = 1u64.into();
        let palw = 2u64.into();
        let mut gd = GhostdagData::new_with_selected_parent(sp, 3);
        gd.add_blue(palw, 0, &Default::default());

        let mut rewards = BlockHashMap::default();
        rewards.insert(sp, BlockRewardData::new(10_000, 0, 0, spk(0x11), WorkRewardClass::HashMiner));
        rewards.insert(
            palw,
            BlockRewardData::new(
                10_000,
                0,
                0,
                spk(0x22), // assembler script (would receive the tx-fee worker share; 0 here)
                WorkRewardClass::ReplicaPalw {
                    batch_id: Hash64::from_bytes([9u8; 64]),
                    leaf_index: 0,
                    provider_a_script: spk(0xaa),
                    provider_b_script: spk(0xbb),
                    premium_pi_bps: kaspa_consensus_core::palw_premium::PALW_PREMIUM_BPS_ONE,
                },
            ),
        );

        let non_daa = BlockHashSet::default();
        let tmpl = cbm
            .expected_coinbase_transaction(0, MinerData::new(spk(0x33), vec![]), &gd, &rewards, &non_daa, &[], Some(&fs), (0, 0))
            .unwrap();

        let by_spk = |b: u8| tmpl.tx.outputs.iter().filter(|o| o.script_public_key == spk(b)).map(|o| o.value).sum::<u64>();
        // Hash-lane source: worker base 62% of 10_000 = 6200 to its own script.
        assert_eq!(by_spk(0x11), 6200, "hash-lane source worker base");
        // PALW source: base 77% = 7700 → A 3850 / B 3850; assembler gets nothing (0 fees).
        assert_eq!(by_spk(0xaa), 3850, "provider A 38.5%");
        assert_eq!(by_spk(0xbb), 3850, "provider B 38.5%");
        assert_eq!(by_spk(0x22), 0, "assembler gets no output with zero fees");
        // No PALW base leaks to the current miner's own script (no red reward, no bounty).
        assert_eq!(by_spk(0x33), 0, "current miner has no output");

        // Validator pool: hash-lane 30% (3000) + PALW-lane 15% (1500) = 4500.
        assert_eq!(cbm.coinbase_validator_pool(&gd, &rewards, &non_daa, &fs), 4500);
    }

    /// ADR-0039 §11.3 (K5) / §17.4 — the ZERO-PAY coinbase arms, exercised directly on the real coinbase
    /// manager (the exact production code both pipeline callers invoke). These arms are the anti-mis-pay /
    /// anti-double-pay teeth of the reward rail and are never reached by the two accept/pay E2E tests:
    ///   • a `ReplicaPalwHalted` **blue** source (algo-4 work minted under a Halted beacon) pays the
    ///     provider pair / assembler NOTHING and contributes 0 to the §E validator pool (coinbase.rs blue
    ///     arm + validator-pool blue arm), and none of its base leaks to the merging miner;
    ///   • a `ReplicaPalw` **red** source (§15.3 duplicate-ticket / §17.4) pays NOTHING, is NOT rerouted
    ///     into the merging miner's `red_reward`, and contributes 0 to the validator pool (red arms);
    ///   • a `ReplicaPalwHalted` **red** source is likewise zero on every axis.
    /// A regression that let any of these arms fall back to the `HashMiner` behavior would pay a
    /// halted/duplicate source's base to the includer — a mint-without-valid-work / double-pay.
    #[test]
    fn palw_replica_halted_and_red_sources_pay_nothing() {
        use kaspa_hashes::Hash64;
        let cbm = create_manager(&MAINNET_PARAMS);
        let spk = |b: u8| ScriptPublicKey::new(0, ScriptVec::from_slice(&[b]));
        let fs = FeeSplitParams {
            subsidy_worker_base_bps: 6200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 3000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        };
        let non_daa = BlockHashSet::default();
        let bid = Hash64::from_bytes([9u8; 64]);

        // --- Case A: a HALTED algo-4 BLUE source pays nothing; the hash-lane sibling is unaffected. ---
        let (sp, halted) = (1u64.into(), 2u64.into());
        let mut gd = GhostdagData::new_with_selected_parent(sp, 3);
        gd.add_blue(halted, 0, &Default::default());
        let mut rewards = BlockHashMap::default();
        rewards.insert(sp, BlockRewardData::new(10_000, 0, 0, spk(0x11), WorkRewardClass::HashMiner));
        rewards.insert(
            halted,
            BlockRewardData::new(10_000, 0, 0, spk(0x22), WorkRewardClass::ReplicaPalwHalted { batch_id: bid, leaf_index: 0 }),
        );
        let tmpl = cbm
            .expected_coinbase_transaction(0, MinerData::new(spk(0x33), vec![]), &gd, &rewards, &non_daa, &[], Some(&fs), (0, 0))
            .unwrap();
        let by = |b: u8| tmpl.tx.outputs.iter().filter(|o| o.script_public_key == spk(b)).map(|o| o.value).sum::<u64>();
        assert_eq!(by(0x11), 6200, "the hash-lane sibling still earns its 62% base");
        assert_eq!(by(0x22), 0, "a HALTED algo-4 source's script earns nothing (no provider/assembler pay)");
        assert_eq!(by(0x33), 0, "no HALTED base (would be 7700) leaks to the merging miner");
        assert_eq!(
            cbm.coinbase_validator_pool(&gd, &rewards, &non_daa, &fs),
            3000,
            "a HALTED source contributes 0 to the §E validator pool (hash-lane 30% only)"
        );

        // --- Case B: a DUPLICATE/RED ReplicaPalw source (§17.4) pays nothing and is NOT rerouted. ---
        let (sp2, red_palw) = (10u64.into(), 11u64.into());
        let mut gd2 = GhostdagData::new_with_selected_parent(sp2, 3);
        gd2.add_red(red_palw);
        let mut rewards2 = BlockHashMap::default();
        rewards2.insert(sp2, BlockRewardData::new(10_000, 0, 0, spk(0x11), WorkRewardClass::HashMiner));
        rewards2.insert(
            red_palw,
            BlockRewardData::new(
                10_000,
                0,
                0,
                spk(0x22),
                WorkRewardClass::ReplicaPalw {
                    batch_id: bid,
                    leaf_index: 0,
                    provider_a_script: spk(0xaa),
                    provider_b_script: spk(0xbb),
                    premium_pi_bps: kaspa_consensus_core::palw_premium::PALW_PREMIUM_BPS_ONE,
                },
            ),
        );
        let tmpl2 = cbm
            .expected_coinbase_transaction(0, MinerData::new(spk(0x33), vec![]), &gd2, &rewards2, &non_daa, &[], Some(&fs), (0, 0))
            .unwrap();
        let by2 = |b: u8| tmpl2.tx.outputs.iter().filter(|o| o.script_public_key == spk(b)).map(|o| o.value).sum::<u64>();
        assert_eq!(by2(0x11), 6200, "the blue hash-lane selected parent is still paid");
        assert_eq!(by2(0xaa), 0, "a red/duplicate PALW source pays provider A nothing (§17.4 anti-double-pay)");
        assert_eq!(by2(0xbb), 0, "a red/duplicate PALW source pays provider B nothing (§17.4 anti-double-pay)");
        assert_eq!(by2(0x33), 0, "the red PALW base is burned by don't-mint, NOT rerouted to the merging miner");
        assert_eq!(
            cbm.coinbase_validator_pool(&gd2, &rewards2, &non_daa, &fs),
            3000,
            "a red PALW source adds 0 to the validator pool (only the hash-lane SP's 30%)"
        );

        // --- Case C: a HALTED algo-4 source in the RED position is likewise zero on every axis. ---
        let (sp3, red_halted) = (20u64.into(), 21u64.into());
        let mut gd3 = GhostdagData::new_with_selected_parent(sp3, 3);
        gd3.add_red(red_halted);
        let mut rewards3 = BlockHashMap::default();
        rewards3.insert(sp3, BlockRewardData::new(10_000, 0, 0, spk(0x11), WorkRewardClass::HashMiner));
        rewards3.insert(
            red_halted,
            BlockRewardData::new(10_000, 0, 0, spk(0x22), WorkRewardClass::ReplicaPalwHalted { batch_id: bid, leaf_index: 0 }),
        );
        let tmpl3 = cbm
            .expected_coinbase_transaction(0, MinerData::new(spk(0x33), vec![]), &gd3, &rewards3, &non_daa, &[], Some(&fs), (0, 0))
            .unwrap();
        let by3 = |b: u8| tmpl3.tx.outputs.iter().filter(|o| o.script_public_key == spk(b)).map(|o| o.value).sum::<u64>();
        assert_eq!(by3(0x22), 0, "a red HALTED source pays nothing");
        assert_eq!(by3(0x33), 0, "a red HALTED source is not rerouted to the merging miner");
        assert_eq!(
            cbm.coinbase_validator_pool(&gd3, &rewards3, &non_daa, &fs),
            3000,
            "a red HALTED source adds 0 to the validator pool"
        );
    }

    /// kaspa-pq **ADR-0040 ECON-03 (THE WIRE) — the rule, not a comment: unresolved provider
    /// collateral is paid NOTHING on every axis.**
    ///
    /// The ECON-03 finding was that the 77 % provider base was paid against zero resolved collateral.
    /// `palw_work_reward_class` now classifies such a source `ReplicaPalwUnbackedCollateral`; this test
    /// exercises the resulting coinbase arms on the REAL coinbase manager — the exact production code
    /// both pipeline callers invoke — and pins that:
    ///   * neither provider reward script is paid (there are no scripts in the class to pay);
    ///   * the withheld base does NOT leak to the merging miner (burn by don't-mint, not reroute) —
    ///     a fallback to `HashMiner` here would pay 7700 to whoever merged the source, turning the
    ///     rule into a subsidy for merging unbacked work;
    ///   * the source contributes 0 to the §E validator pool;
    ///   * a hash-lane sibling in the same mergeset is entirely unaffected.
    ///
    /// Both the blue and the red position are covered, because the reward rail treats them through
    /// different arms and a regression could reach only one.
    #[test]
    fn palw_unbacked_collateral_sources_pay_nothing() {
        use kaspa_consensus_core::tx::TransactionOutpoint;
        use kaspa_hashes::Hash64;
        let cbm = create_manager(&MAINNET_PARAMS);
        let spk = |b: u8| ScriptPublicKey::new(0, ScriptVec::from_slice(&[b]));
        let fs = FeeSplitParams {
            subsidy_worker_base_bps: 6200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 3000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        };
        let non_daa = BlockHashSet::default();
        let unbacked = |leaf_index: u32| WorkRewardClass::ReplicaPalwUnbackedCollateral {
            batch_id: Hash64::from_bytes([0x5c; 64]),
            leaf_index,
            provider_a_bond: TransactionOutpoint::new(Hash64::from_bytes([0x5d; 64]), 0),
            provider_b_bond: TransactionOutpoint::new(Hash64::from_bytes([0x5e; 64]), 0),
        };

        // --- BLUE position: an unbacked algo-4 source earns nothing; the hash sibling is unaffected.
        let (sp, bad) = (1u64.into(), 2u64.into());
        let mut gd = GhostdagData::new_with_selected_parent(sp, 3);
        gd.add_blue(bad, 0, &Default::default());
        let mut rewards = BlockHashMap::default();
        rewards.insert(sp, BlockRewardData::new(10_000, 0, 0, spk(0x11), WorkRewardClass::HashMiner));
        rewards.insert(bad, BlockRewardData::new(10_000, 0, 0, spk(0x22), unbacked(0)));
        let tmpl = cbm
            .expected_coinbase_transaction(0, MinerData::new(spk(0x33), vec![]), &gd, &rewards, &non_daa, &[], Some(&fs), (0, 0))
            .unwrap();
        let by = |b: u8| tmpl.tx.outputs.iter().filter(|o| o.script_public_key == spk(b)).map(|o| o.value).sum::<u64>();
        assert_eq!(by(0x11), 6200, "the hash-lane sibling still earns its 62% base");
        assert_eq!(by(0x22), 0, "an unbacked algo-4 source's own script earns nothing");
        assert_eq!(by(0x33), 0, "the withheld 77% base is burned by don't-mint, NOT rerouted to the merging miner");
        assert_eq!(
            cbm.coinbase_validator_pool(&gd, &rewards, &non_daa, &fs),
            3000,
            "an unbacked source contributes 0 to the §E validator pool (hash-lane 30% only)"
        );

        // --- RED position: likewise zero on every axis.
        let (sp2, bad_red) = (10u64.into(), 11u64.into());
        let mut gd2 = GhostdagData::new_with_selected_parent(sp2, 3);
        gd2.add_red(bad_red);
        let mut rewards2 = BlockHashMap::default();
        rewards2.insert(sp2, BlockRewardData::new(10_000, 0, 0, spk(0x11), WorkRewardClass::HashMiner));
        rewards2.insert(bad_red, BlockRewardData::new(10_000, 0, 0, spk(0x22), unbacked(1)));
        let tmpl2 = cbm
            .expected_coinbase_transaction(0, MinerData::new(spk(0x33), vec![]), &gd2, &rewards2, &non_daa, &[], Some(&fs), (0, 0))
            .unwrap();
        let by2 = |b: u8| tmpl2.tx.outputs.iter().filter(|o| o.script_public_key == spk(b)).map(|o| o.value).sum::<u64>();
        assert_eq!(by2(0x22), 0, "a red unbacked source pays nothing");
        assert_eq!(by2(0x33), 0, "a red unbacked source is not rerouted to the merging miner");
        assert_eq!(
            cbm.coinbase_validator_pool(&gd2, &rewards2, &non_daa, &fs),
            3000,
            "a red unbacked source adds 0 to the validator pool"
        );
    }

    /// ADR-0039 §17/§22 — REWARD-RAIL E2E from real k=2 data, in-process (no network, no real value):
    /// two honest deterministic mock providers run k=2 → exact-match → the shared match key mints an
    /// on-chain leaf → BOTH (1) the leaf's ticket passes the full nine-clause `verify_palw_ticket`, AND
    /// (2) the REAL coinbase construction credits the leaf's two provider reward scripts (base 77% → A
    /// 38.5% / B 38.5%). This is the reward RAIL: a k=2 inference's providers get paid, proven end-to-end
    /// on real consensus reward code. The running-network parts (DNS beacon `R_E`, auditor certificate,
    /// block mining) are activation-tier and deliberately NOT exercised here.
    #[test]
    fn palw_reward_rail_e2e_from_k2_mock() {
        use kaspa_consensus_core::palw::{
            PalwPublicLeafV1, PalwTicketBinding, palw_select_template_ticket, palw_template_candidate, ticket_nullifier_commitment,
            verify_palw_ticket,
        };
        use kaspa_consensus_core::tx::TransactionOutpoint;
        use kaspa_hashes::Hash64;
        use misaka_palw::palw::{PalwRuntimeProfileV1, PalwSamplingParams, PalwTier};
        use misaka_palw::palw_replica::{MockDeterministicRuntime, ReplicaK2Outcome, dispatch_k2};
        let h = |b: u8| Hash64::from_bytes([b; 64]);
        let spk = |b: u8| ScriptPublicKey::new(0, ScriptVec::from_slice(&[b]));

        // 1) Deterministic k=2 inference: two honest same-class mock providers exact-match.
        let profile = |tier: PalwTier, arch: u32| PalwRuntimeProfileV1 {
            version: 1,
            tier,
            model_id: tier.model_id(),
            tokenizer_hash: h(1),
            quantization_manifest_hash: h(2),
            runtime_image_hash: h(3),
            kernel_graph_hash: h(4),
            operation_table_hash: h(5),
            shape_table_hash: h(6),
            gpu_arch_class: arch,
            tensor_parallel_degree: 1,
            pipeline_parallel_degree: 1,
            deterministic_reduction: true,
            batch_invariant: true,
            speculative_decode: false,
            sampling: PalwSamplingParams::greedy(),
        };
        let a = MockDeterministicRuntime::new(profile(PalwTier::Quality, 100), 3, 2);
        let b = MockDeterministicRuntime::new(profile(PalwTier::Quality, 100), 3, 2);
        let key = match dispatch_k2(&a, &b, b"job-set-descriptor", b"what is the capital of the moon?", &[0x11; 32]) {
            ReplicaK2Outcome::Matched(k) => k,
            ReplicaK2Outcome::Mismatch => panic!("two honest same-class providers must match"),
        };

        // 2) Mint the on-chain leaf from the shared match key; the two providers get one-time reward scripts.
        let (prov_a, prov_b) = (spk(0xaa), spk(0xbb));
        let raw_nf = h(0xC0);
        let (batch_id, leaf_index) = (h(0x10), 0u32);
        let leaf = PalwPublicLeafV1 {
            version: 1,
            batch_id,
            leaf_index,
            job_nullifier: h(0x20),
            ticket_nullifier_commitment: ticket_nullifier_commitment(&raw_nf),
            model_profile_id: key.model_profile_id,
            runtime_class_id: key.runtime_class_id,
            shape_id: key.shape_id,
            quantum_count: key.quantum_count,
            proof_type: 1,
            provider_a_bond: TransactionOutpoint::new(h(6), 0),
            provider_b_bond: TransactionOutpoint::new(h(7), 0),
            provider_a_reward_script: prov_a.clone(),
            provider_b_reward_script: prov_b.clone(),
            ticket_authority_pk_hash: h(8),
            private_match_commitment: key.canonical_gemm_trace_root, // binds the leaf to THIS exact k=2 GEMM
            receipt_da_object_version: 1,
            receipt_da_root: h(10),
            receipt_da_object_len: 1,
            receipt_da_chunk_count: 1,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: 3,
            activation_epoch: 4,
            expiry_epoch: 12,
            leaf_bond_sompi: 0,
            a_commit: Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root: Hash64::from_bytes([0x7c; 64]),
            assignment_proof_root: Hash64::from_bytes([0x7d; 64]),
            dispatch_kind: 0, // BeaconAssigned (external sentinels)
        };

        // 3) ACCEPTANCE rail — the leaf's ticket passes the full nine-clause verify_palw_ticket.
        let (net, elig_beacon, chain_commit, interval, epoch) = (0x9107u32, h(0x77), h(0x88), 600u64, 5u64);
        let lane_bits = 0x2100ffff_u32;
        let cand =
            palw_template_candidate(net, &elig_beacon, &chain_commit, interval, &batch_id, leaf_index, &leaf.leaf_hash(), &raw_nf);
        assert_eq!(
            palw_select_template_ticket(std::slice::from_ref(&cand), lane_bits),
            Some(0),
            "the k=2 leaf's ticket wins its draw"
        );
        let binding = PalwTicketBinding {
            ticket_nullifier_commitment: leaf.ticket_nullifier_commitment,
            proof_type: leaf.proof_type,
            leaf_activation_epoch: leaf.activation_epoch,
            leaf_expiry_epoch: leaf.expiry_epoch,
            target_daa_interval: interval,
        };
        assert_eq!(
            verify_palw_ticket(
                &raw_nf,
                leaf.proof_type,
                &chain_commit,
                lane_bits,
                cand.nonce,
                interval,
                &cand.eligibility_digest,
                &binding,
                true,
                epoch,
                &chain_commit,
                lane_bits,
                true,
            ),
            Ok(()),
            "the k=2 leaf's ticket is accepted by the validator"
        );

        // 4) REWARD rail — the REAL coinbase construction credits the leaf's two providers (base 77% split).
        let cbm = create_manager(&MAINNET_PARAMS);
        let fs = FeeSplitParams {
            subsidy_worker_base_bps: 6200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 3000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        };
        let (sp, palw_src) = (1u64.into(), 2u64.into());
        let mut gd = GhostdagData::new_with_selected_parent(sp, 3);
        gd.add_blue(palw_src, 0, &Default::default());
        let mut rewards = BlockHashMap::default();
        rewards.insert(sp, BlockRewardData::new(10_000, 0, 0, spk(0x11), WorkRewardClass::HashMiner));
        // The reward class is derived from THIS k=2 leaf — its provider reward scripts.
        rewards.insert(
            palw_src,
            BlockRewardData::new(
                10_000,
                0,
                0,
                spk(0x22),
                WorkRewardClass::ReplicaPalw {
                    batch_id: leaf.batch_id,
                    leaf_index: leaf.leaf_index,
                    provider_a_script: leaf.provider_a_reward_script.clone(),
                    provider_b_script: leaf.provider_b_reward_script.clone(),
                    premium_pi_bps: kaspa_consensus_core::palw_premium::PALW_PREMIUM_BPS_ONE,
                },
            ),
        );
        let tmpl = cbm
            .expected_coinbase_transaction(
                0,
                MinerData::new(spk(0x33), vec![]),
                &gd,
                &rewards,
                &BlockHashSet::default(),
                &[],
                Some(&fs),
                (0, 0),
            )
            .unwrap();
        let credited =
            |s: &ScriptPublicKey| tmpl.tx.outputs.iter().filter(|o| &o.script_public_key == s).map(|o| o.value).sum::<u64>();
        // The two providers from the k=2 match are each credited 38.5% of the 10_000 subsidy.
        assert_eq!(credited(&prov_a), 3850, "provider A (from the k=2 match) is credited 38.5%");
        assert_eq!(credited(&prov_b), 3850, "provider B (from the k=2 match) is credited 38.5%");
        assert_eq!(credited(&spk(0x33)), 0, "no base leaks to the assembler/miner");
    }

    /// kaspa-pq **ADR-0040 §16′** — the replica premium `π` actually reaches the coinbase, and the
    /// neutral point is a no-op.
    ///
    /// Two properties, and the first is what makes the change safe to land:
    ///
    /// 1. **At `π = 1` the split is byte-identical to the previous fixed 38.5 / 38.5.** A net that never
    ///    leaves the neutral point pays exactly what it paid before, so shipping the controller changes
    ///    no consensus outcome until it is deliberately moved.
    /// 2. **Off-neutral, the base redistributes by `σ_A = 1/(1+mπ)`, `σ_B = π/(1+mπ)`, conserving the
    ///    base exactly.** Nothing leaks to the assembler at any `π`.
    ///
    /// Safe to make dynamic because the split is invariant under collusion economics: in a
    /// self-collusion attack the attacker takes the leaf's whole value either way, so the ratio moves
    /// only the honest supply incentive.
    #[test]
    fn palw_replica_premium_reaches_the_coinbase_and_is_neutral_at_pi_one() {
        use kaspa_consensus_core::palw_premium::PALW_PREMIUM_BPS_ONE;
        let spk = |b: u8| ScriptPublicKey::new(0, ScriptVec::from_slice(&[b]));

        let cbm = create_manager(&MAINNET_PARAMS);
        // Same PALW-lane fee split the sibling k=2 test uses (§17: base 62 % + inclusion 8 % + validator 30 %).
        let fs = FeeSplitParams {
            subsidy_worker_base_bps: 6200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 3000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        };
        let (prov_a, prov_b) = (spk(0xa0), spk(0xb0));
        let (sp, palw_src) = (1u64.into(), 2u64.into());
        let mut gd = GhostdagData::new_with_selected_parent(sp, 3);
        gd.add_blue(palw_src, 0, &Default::default());

        // base = 77 % of the 10_000 subsidy = 7_700, which the premium then splits.
        let credited_at = |pi_bps: u32| {
            let mut rewards = BlockHashMap::default();
            rewards.insert(sp, BlockRewardData::new(10_000, 0, 0, spk(0x11), WorkRewardClass::HashMiner));
            rewards.insert(
                palw_src,
                BlockRewardData::new(
                    10_000,
                    0,
                    0,
                    spk(0x22),
                    WorkRewardClass::ReplicaPalw {
                        batch_id: 0x42u64.into(),
                        leaf_index: 0,
                        provider_a_script: prov_a.clone(),
                        provider_b_script: prov_b.clone(),
                        premium_pi_bps: pi_bps,
                    },
                ),
            );
            let tmpl = cbm
                .expected_coinbase_transaction(
                    0,
                    MinerData::new(spk(0x33), vec![]),
                    &gd,
                    &rewards,
                    &BlockHashSet::default(),
                    &[],
                    Some(&fs),
                    (0, 0),
                )
                .unwrap();
            let of = |s: &ScriptPublicKey| tmpl.tx.outputs.iter().filter(|o| &o.script_public_key == s).map(|o| o.value).sum::<u64>();
            (of(&prov_a), of(&prov_b), of(&spk(0x33)))
        };

        // (1) neutral ⇒ the pre-controller split, exactly.
        let (a, b, miner) = credited_at(PALW_PREMIUM_BPS_ONE);
        assert_eq!((a, b), (3850, 3850), "π = 1 must reproduce the fixed 38.5/38.5 split byte for byte");
        assert_eq!(miner, 0);

        // (2) off-neutral ⇒ redistribution, conservation, and no leak to the assembler.
        let base = a + b;
        for (pi, label) in [(5_000u32, "π=0.5 favours A"), (20_000, "π=2 favours B"), (30_000, "π=3 (ceiling)")] {
            let (a, b, miner) = credited_at(pi);
            assert_eq!(a + b, base, "{label}: the provider base must be conserved exactly");
            assert_eq!(miner, 0, "{label}: no base may leak to the assembler");
            if pi < PALW_PREMIUM_BPS_ONE {
                assert!(a > b, "{label}: A={a} B={b}");
            } else {
                assert!(b > a, "{label}: A={a} B={b}");
            }
        }
        // σ_A = 1/(1+π): at π = 2 that is 1/3 of the base, at π = 3 it is 1/4.
        assert_eq!(credited_at(20_000).0, base / 3, "π = 2 ⇒ σ_A = 1/3");
        assert_eq!(credited_at(30_000).0, base / 4, "π = 3 ⇒ σ_A = 1/4");
    }

    fn create_manager(params: &Params) -> CoinbaseManager {
        CoinbaseManager::new(
            params.coinbase_payload_script_public_key_max_len,
            params.max_coinbase_payload_len,
            params.deflationary_phase_daa_score,
            params.pre_deflationary_phase_base_subsidy,
            params.bps_history(),
        )
    }

    /// Return a CoinbaseManager with legacy golang 1 BPS properties
    fn create_legacy_manager() -> CoinbaseManager {
        CoinbaseManager::new(150, 204, 15778800 - 259200, 50000000000, ForkedParam::new_const(1))
    }
}
