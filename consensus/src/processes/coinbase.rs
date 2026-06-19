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

// kaspa-pq emission: 20 years of additional issuance (240 months) + a
// terminal 0 entry marking the end of issuance.
pub const SUBSIDY_BY_MONTH_TABLE_SIZE: usize = 241;
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

        // Collect all rewards from mergeset reds ∩ DAA window and create a
        // single output rewarding all to the current block (the "merging" block)
        let mut red_reward = 0u64;

        for red in ghostdag_data.mergeset_reds.iter() {
            let reward_data = mergeset_rewards.get(red).unwrap();
            // Reds ∩ DAA earn subsidy + fees; non-DAA reds earn fees only (both fee classes kept).
            let (eff_subsidy, eff_fees) =
                if mergeset_non_daa.contains(red) { (0, reward_data.total_fees) } else { (reward_data.subsidy, reward_data.total_fees) };
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
            let bounty = worker_inclusion_bounty(worker_inclusion_pool as u128, newly_included_stake, expected_stake, STAKE_SCORE_SCALE, false, 0)
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
            pool = pool.saturating_add(
                split_block_reward(reward_data.subsidy, reward_data.total_fees, reward_data.finality_fees, fee_split).validator_sompi,
            );
        }
        for red in ghostdag_data.mergeset_reds.iter() {
            let reward_data = mergeset_rewards.get(red).unwrap();
            let (eff_subsidy, eff_fees) =
                if mergeset_non_daa.contains(red) { (0, reward_data.total_fees) } else { (reward_data.subsidy, reward_data.total_fees) };
            pool = pool.saturating_add(split_block_reward(eff_subsidy, eff_fees, reward_data.finality_fees, fee_split).validator_sompi);
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
    kaspa-pq additional-issuance emission table.

    Tokenomics: 15B KAS of additional issuance over 20 years, decaying at a
    5%/year exponential rate (q = 0.95), on top of a 13B genesis premine for a
    28B final supply. The schedule steps once per year (12 identical months),
    so the table holds 20 yearly rates × 12 months = 240 entries followed by a
    terminal 0 (issuance ends after year 20).

    Values are the reward per second (= reward per block at 1 BPS); the manager
    divides each by the actual BPS via `div_ceil` at construction. Each yearly
    rate is `round(E_y / SECONDS_PER_YEAR)` with `E_y = E_1 · 0.95^(y-1)` and
    `E_1 = 15e9 · (1 - 0.95) / (1 - 0.95^20) ≈ 1.169109184e9 KAS`. This yields
    ≈ 3.70468 KAS/block in year 1 at 10 BPS and a 20-year total of ≈ 15B KAS.

    To regenerate, recompute the 20 yearly rates with the formula above
    (SECONDS_PER_YEAR = 12 · SECONDS_PER_MONTH = 31_557_600) and repeat each 12×.
*/
#[rustfmt::skip]
const SUBSIDY_BY_MONTH_TABLE: [u64; SUBSIDY_BY_MONTH_TABLE_SIZE] = [
    3704683450, 3704683450, 3704683450, 3704683450, 3704683450, 3704683450, 3704683450, 3704683450, 3704683450, 3704683450, 3704683450, 3704683450, 3519449277, 3519449277, 3519449277, 3519449277, 3519449277, 3519449277, 3519449277, 3519449277, 3519449277, 3519449277, 3519449277, 3519449277, 3343476813,
    3343476813, 3343476813, 3343476813, 3343476813, 3343476813, 3343476813, 3343476813, 3343476813, 3343476813, 3343476813, 3343476813, 3176302973, 3176302973, 3176302973, 3176302973, 3176302973, 3176302973, 3176302973, 3176302973, 3176302973, 3176302973, 3176302973, 3176302973, 3017487824, 3017487824,
    3017487824, 3017487824, 3017487824, 3017487824, 3017487824, 3017487824, 3017487824, 3017487824, 3017487824, 3017487824, 2866613433, 2866613433, 2866613433, 2866613433, 2866613433, 2866613433, 2866613433, 2866613433, 2866613433, 2866613433, 2866613433, 2866613433, 2723282761, 2723282761, 2723282761,
    2723282761, 2723282761, 2723282761, 2723282761, 2723282761, 2723282761, 2723282761, 2723282761, 2723282761, 2587118623, 2587118623, 2587118623, 2587118623, 2587118623, 2587118623, 2587118623, 2587118623, 2587118623, 2587118623, 2587118623, 2587118623, 2457762692, 2457762692, 2457762692, 2457762692,
    2457762692, 2457762692, 2457762692, 2457762692, 2457762692, 2457762692, 2457762692, 2457762692, 2334874557, 2334874557, 2334874557, 2334874557, 2334874557, 2334874557, 2334874557, 2334874557, 2334874557, 2334874557, 2334874557, 2334874557, 2218130830, 2218130830, 2218130830, 2218130830, 2218130830,
    2218130830, 2218130830, 2218130830, 2218130830, 2218130830, 2218130830, 2218130830, 2107224288, 2107224288, 2107224288, 2107224288, 2107224288, 2107224288, 2107224288, 2107224288, 2107224288, 2107224288, 2107224288, 2107224288, 2001863074, 2001863074, 2001863074, 2001863074, 2001863074, 2001863074,
    2001863074, 2001863074, 2001863074, 2001863074, 2001863074, 2001863074, 1901769920, 1901769920, 1901769920, 1901769920, 1901769920, 1901769920, 1901769920, 1901769920, 1901769920, 1901769920, 1901769920, 1901769920, 1806681424, 1806681424, 1806681424, 1806681424, 1806681424, 1806681424, 1806681424,
    1806681424, 1806681424, 1806681424, 1806681424, 1806681424, 1716347353, 1716347353, 1716347353, 1716347353, 1716347353, 1716347353, 1716347353, 1716347353, 1716347353, 1716347353, 1716347353, 1716347353, 1630529985, 1630529985, 1630529985, 1630529985, 1630529985, 1630529985, 1630529985, 1630529985,
    1630529985, 1630529985, 1630529985, 1630529985, 1549003486, 1549003486, 1549003486, 1549003486, 1549003486, 1549003486, 1549003486, 1549003486, 1549003486, 1549003486, 1549003486, 1549003486, 1471553312, 1471553312, 1471553312, 1471553312, 1471553312, 1471553312, 1471553312, 1471553312, 1471553312,
    1471553312, 1471553312, 1471553312, 1397975646, 1397975646, 1397975646, 1397975646, 1397975646, 1397975646, 1397975646, 1397975646, 1397975646, 1397975646, 1397975646, 1397975646, 0,
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

    /// Verifies the kaspa-pq additional-issuance schedule sums to ~15B KAS over
    /// 20 years. The per-month table holds reward-per-second values, so the total
    /// issuance is `Σ table[m] * SECONDS_PER_MONTH` (BPS-invariant: higher BPS
    /// divides the per-block reward but produces proportionally more blocks, up to
    /// a small `div_ceil` rounding surplus).
    #[test]
    fn verify_total_emission() {
        // 1 BPS reference total (the clean figure the table is derived from).
        let total_sompi: u128 = SUBSIDY_BY_MONTH_TABLE.iter().map(|&x| x as u128 * SECONDS_PER_MONTH as u128).sum();
        let total_kas = total_sompi / SOMPI_PER_KASPA as u128;
        println!("kaspa-pq additional issuance: {total_sompi} sompi => {total_kas} KAS");

        const TARGET_KAS: u128 = 15_000_000_000;
        let delta_kas = TARGET_KAS as i128 - total_kas as i128;
        assert!(delta_kas.abs() <= 1, "additional issuance {total_kas} KAS deviates from 15B by {delta_kas} KAS");
        // The clean 1 BPS figure stays within the 15B budget; the live network adds
        // only the small div_ceil rounding surplus checked below.
        assert!(total_kas <= TARGET_KAS, "additional issuance {total_kas} KAS exceeds the 15B budget");

        // Per-network totals differ from the 1 BPS reference only by the per-month
        // div_ceil rounding surplus: at most (bps-1) sompi/month * SECONDS_PER_MONTH *
        // 240 months ≈ 57 KAS at 10 BPS (cf. the upstream "+51 KAS" note). Negligible
        // against the 28B supply (1 part in ~5e8) and far below the MAX_SOMPI cap.
        for network_id in NetworkId::iter() {
            let cbm = create_manager(&network_id.into());
            let bps = Params::from(network_id).bps();
            let net_total: u128 =
                cbm.subsidy_by_month_table_after.iter().map(|&x| x as u128 * SECONDS_PER_MONTH as u128 * bps as u128).sum();
            let surplus_kas = net_total as i128 / SOMPI_PER_KASPA as i128 - total_kas as i128;
            assert!((0..=64).contains(&surplus_kas), "{network_id}: bps rounding surplus {surplus_kas} KAS out of range");
        }
    }

    #[test]
    fn subsidy_test() {
        // Year-1 per-block subsidy at 10 BPS = table[0].div_ceil(10) ≈ 3.70468 KAS.
        const YEAR1_PER_BLOCK_10BPS: u64 = 370468345;

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
            for m in 0..SUBSIDY_BY_MONTH_TABLE_SIZE - 1 {
                let daa = m as u64 * blocks_per_month;
                let expected = SUBSIDY_BY_MONTH_TABLE[m].div_ceil(bps);
                assert_eq!(cbm.calc_block_subsidy(daa), expected, "{network_id}: month {m} start");
                assert_eq!(cbm.calc_block_subsidy(daa + blocks_per_month - 1), expected, "{network_id}: month {m} end");
            }

            // 5%/year exponential decay: each year's rate is ~0.95x the previous year's.
            for y in 1..20usize {
                let prev = SUBSIDY_BY_MONTH_TABLE[(y - 1) * 12] as f64;
                let curr = SUBSIDY_BY_MONTH_TABLE[y * 12] as f64;
                let ratio = curr / prev;
                assert!((ratio - 0.95).abs() < 1e-4, "{network_id}: year {y}->{} decay ratio {ratio}", y + 1);
            }

            // Issuance ends after 20 years: month index >= 240 yields zero subsidy.
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
