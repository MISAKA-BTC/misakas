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
    /// Target time per block throughout history, in **milliseconds**. The emission schedule used
    /// to consume integer blocks-per-second, which truncates to 0 on the 0.1-bps PALW network;
    /// every rate conversion below scales by `ttpb / 1000` instead (bit-identical on the
    /// integer-bps networks: `(v * 100).div_ceil(1000) == v.div_ceil(10)` and
    /// `daa * 100 / 1000 == daa / 10`).
    ttpb_history: ForkedParam<u64>,

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
        ttpb_history: ForkedParam<u64>,
    ) -> Self {
        // Precomputed subsidy by month table for the actual block rate. The const table holds
        // reward-per-second values; the per-block reward is `value * ttpb / 1000`, rounded UP so
        // we keep the same number of rewarding months as in the original 1 BPS table (on a 10 BPS
        // network the induced increase in total rewards is ~51 KAS — see
        // tests::calc_high_bps_total_rewards_delta; on a sub-1-bps network the product is exact
        // and there is no rounding surplus at all).
        let subsidy_by_month_table_before: SubsidyByMonthTable =
            core::array::from_fn(|i| (SUBSIDY_BY_MONTH_TABLE[i] * ttpb_history.before()).div_ceil(1000));
        let subsidy_by_month_table_after: SubsidyByMonthTable =
            core::array::from_fn(|i| (SUBSIDY_BY_MONTH_TABLE[i] * ttpb_history.after()).div_ceil(1000));
        Self {
            coinbase_payload_script_public_key_max_len,
            max_coinbase_payload_len,
            deflationary_phase_daa_score,
            pre_deflationary_phase_base_subsidy,
            ttpb_history,
            subsidy_by_month_table_before,
            subsidy_by_month_table_after,
            crescendo_activation_daa_score: ttpb_history.activation().daa_score(),
        }
    }

    #[cfg(test)]
    #[inline]
    pub fn ttpb(&self) -> ForkedParam<u64> {
        self.ttpb_history
    }

    // Eleven arguments against a ceiling of ten. Every one is a distinct consensus input and
    // bundling them into a struct would move the coupling rather than remove it -- the call
    // sites would still have to get all eleven right, with one more indirection to read through.
    #[allow(clippy::too_many_arguments)]
    pub fn expected_coinbase_transaction<T: AsRef<[u8]>>(
        &self,
        _daa_score: u64,
        // **The subsidy this block's own payload declares** (ADR-0060 Decision 1.4). Almost
        // always `calc_block_subsidy(daa_score)` — but a heartbeat (algo-3 on a ConsensusV2
        // network) declares ZERO, and the validation path must expect the same payload the body
        // rule enforced, or every heartbeat chain block dies here as "not built as expected".
        own_subsidy: u64,
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
        // **MISAKA ADR-0042 Decision 10: the PALW escrow WITHHELD from the selected parent's
        // worker reward.**
        //
        // Decision 10 states the rule this argument enforces: "PALW reward is a carve of the fixed
        // subsidy ... never an addition to it — the schedule is never exceeded (I6/I15)." The
        // escrow releases were already appended to `validator_reward_outputs`, and nothing was
        // ever taken out to fund them, so every finalized claim minted its whole carve ON TOP of
        // the emission schedule. Both halves have to exist or the sentence is false in one
        // direction or the other.
        //
        // It is the SELECTED PARENT's escrow specifically: a claim is created by the PALW
        // transition, which runs only for chain blocks, and exactly one block of any mergeset is a
        // chain block — the selected parent. A merge-blue attempt-lane block that never joined the
        // chain created no claim and is paid in full, correctly.
        //
        // `0` on every network without a V2 bundle, which is every shipped preset.
        palw_escrow_withheld: u64,
        // **ADR-0038: the mergeset blues whose producer this chain cannot show is bonded** —
        // launch blockers §8, first bullet.
        //
        // The subsidy pays for PALW work. On a `ConsensusV2` network the attempt's stateless half
        // (shape, the challenge against the header's own position, the executor signature) is
        // checked for EVERY block before GHOSTDAG, but the stateful half — the named bond exists,
        // is not retiring, holds the carried key and operator, and the class is registered and
        // unfrozen — runs only where there is chain state to run it against, which is the selected
        // chain. Every other merged blue was paid its full worker share anyway, so a block that
        // never joined the chain collected the subsidy on a solved hash and an unbonded key.
        //
        // These are the blues that failed that check against the accepting block's own state.
        // They are not rejected — DAG membership must not depend on state their miner could not
        // have known — they are simply not paid, and the value is not minted elsewhere either.
        //
        // Empty on every network without a V2 bundle, which is every shipped preset.
        palw_unentitled_blues: &BlockHashSet,
        // **ADR-0058: on a `ConsensusV2` network an entitled, in-DAA-window red is paid its
        // worker share to ITS OWN miner script, exactly as a blue is** — not lumped into the
        // merging miner's red reward. At the frozen 120 s cadence `ghostdag_k = 1`, so any block
        // whose anticone holds two or more blocks is a red BY CONSTRUCTION — which is every
        // block of every class slower than the floor. Its claim (created by the accepting
        // block's transition, ADR-0058) carries the slash exposure; paying the includer instead
        // would put the stake on one key and the reward on another. `false` outside V2 keeps the
        // legacy lump byte-identical.
        palw_pay_entitled_reds_to_their_miner: bool,
    ) -> CoinbaseResult<CoinbaseTransactionTemplate> {
        // §D base inclusion bounty: the worker-inclusion sub-pool summed over the SAME
        // mergeset blue(∩DAA)+red iteration the Worker carve uses (paid to the includer below).
        // The withholding must LAND, or an escrow would be released that was never funded. The only
        // mergeset member that can be both the selected parent and non-DAA is genesis — the window
        // manager inserts it explicitly, because its timestamp is fixed — and genesis registers
        // rather than works: it creates no claim and escrows nothing. Asserted rather than assumed,
        // because the assumption is what the whole "never an addition to the schedule" rule rests on.
        debug_assert!(
            palw_escrow_withheld == 0 || !mergeset_non_daa.contains(&ghostdag_data.selected_parent),
            "a selected parent that escrowed a reward must be inside the DAA window, or its escrow is unfunded"
        );
        let mut worker_inclusion_pool = 0u64;
        let mut outputs = Vec::with_capacity(ghostdag_data.mergeset_blues.len() + 1); // + 1 for possible red reward
        let mut miner_script_output_indices = Vec::with_capacity(2); // red reward + optional inclusion bounty

        // Add an output for each mergeset blue block (∩ DAA window), paying to the script reported by the block.
        // Note that combinatorically it is nearly impossible for a blue block to be non-DAA
        for blue in ghostdag_data.mergeset_blues.iter().filter(|h| !mergeset_non_daa.contains(h)) {
            // Not paid, and nothing is redistributed: the share is simply not minted. Placed
            // before the reward lookup so an unentitled blue costs nothing to skip.
            if palw_unentitled_blues.contains(blue) {
                continue;
            }
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
            // ADR-0042 Decision 10: an attempt-lane chain block's worker reward is ESCROWED, not
            // paid — it becomes spendable when its claim reaches `Final`, and is burned by
            // don't-mint if the claim voids. Withheld here, from the one block of the mergeset
            // whose transition could have created a claim.
            //
            // `saturating_sub` rather than an assert: the escrow is bounded by the worker base
            // share at bundle construction (`Params::validate_palw_v2`), so this cannot bite on a
            // network a node will start on — and if a future split made it bite, under-paying the
            // miner is the direction that does not mint.
            let value = if *blue == ghostdag_data.selected_parent { value.saturating_sub(palw_escrow_withheld) } else { value };
            if value > 0 {
                outputs.push(TransactionOutput::new(value, reward_data.script_public_key.clone()));
            }
        }

        // Collect all rewards from mergeset reds ∩ DAA window and create a
        // single output rewarding all to the current block (the "merging" block)
        let mut red_reward = 0u64;

        for red in ghostdag_data.mergeset_reds.iter() {
            // **The same skip the blues loop has, for the same reason.** It was missing here
            // while the set was built from blues alone, so the two halves agreed only by never
            // meeting: an unentitled red was paid its full worker share to this block's miner.
            // At the frozen cadence `ghostdag_k = 1` against a mergeset limit of 180, so the
            // blues this filtered were at most ONE block and the reds it did not were
            // everything else — which is where the value actually was.
            if palw_unentitled_blues.contains(red) {
                continue;
            }
            let reward_data = mergeset_rewards.get(red).unwrap();
            // Reds ∩ DAA earn subsidy + fees; non-DAA reds earn fees only (both fee classes kept).
            let (eff_subsidy, eff_fees) = if mergeset_non_daa.contains(red) {
                (0, reward_data.total_fees)
            } else {
                (reward_data.subsidy, reward_data.total_fees)
            };
            // ADR-0058: see the parameter — an entitled in-window red's share goes to the red's
            // own script, through the same carve arithmetic as the lump below, so moving a block
            // between the two pay paths never changes the amount, only the payee.
            if palw_pay_entitled_reds_to_their_miner && !mergeset_non_daa.contains(red) {
                let value = match carve {
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
                if value > 0 {
                    outputs.push(TransactionOutput::new(value, reward_data.script_public_key.clone()));
                }
                continue;
            }
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

        // Build the current block's payload. `own_subsidy` is the caller's per-lane answer —
        // see the parameter; `daa_score` still prices every MERGED block's reward above.
        let payload =
            self.serialize_coinbase_payload(&CoinbaseData { blue_score: ghostdag_data.blue_score, subsidy: own_subsidy, miner_data })?;

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
    /// `palw_unentitled` is the same set the coinbase itself refuses to pay. This function's doc
    /// used to claim it iterates identically to the coinbase "so the pool and the carve never
    /// drift", and it took no such argument — so at `subsidy_validator_bps = 3000`, 30% of an
    /// unentitled block's subsidy was still minted into the attester pool the coinbase had just
    /// declined to fund. The pool is a payout SCALE, not a burn ceiling, so that value was paid.
    pub fn coinbase_validator_pool(
        &self,
        ghostdag_data: &GhostdagData,
        mergeset_rewards: &BlockHashMap<BlockRewardData>,
        mergeset_non_daa: &BlockHashSet,
        fee_split: &FeeSplitParams,
        palw_unentitled: &BlockHashSet,
    ) -> u64 {
        let mut pool = 0u64;
        for blue in ghostdag_data.mergeset_blues.iter().filter(|h| !mergeset_non_daa.contains(h)) {
            if palw_unentitled.contains(blue) {
                continue;
            }
            let reward_data = mergeset_rewards.get(blue).unwrap();
            pool = pool.saturating_add(
                split_block_reward(reward_data.subsidy, reward_data.total_fees, reward_data.finality_fees, fee_split).validator_sompi,
            );
        }
        for red in ghostdag_data.mergeset_reds.iter() {
            if palw_unentitled.contains(red) {
                continue;
            }
            let reward_data = mergeset_rewards.get(red).unwrap();
            let (eff_subsidy, eff_fees) = if mergeset_non_daa.contains(red) {
                (0, reward_data.total_fees)
            } else {
                (reward_data.subsidy, reward_data.total_fees)
            };
            pool =
                pool.saturating_add(split_block_reward(eff_subsidy, eff_fees, reward_data.finality_fees, fee_split).validator_sompi);
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
        let subsidy_table = if self.ttpb_history.activation().is_active(daa_score) {
            &self.subsidy_by_month_table_after
        } else {
            &self.subsidy_by_month_table_before
        };
        subsidy_table[subsidy_month.min(subsidy_table.len() - 1)]
    }

    /// Get the subsidy month as function of the current DAA score.
    ///
    /// Note that this function is called only if daa_score >= self.deflationary_phase_daa_score.
    /// Elapsed seconds are `blocks * ttpb / 1000` (floor) — identical to the old `blocks / bps`
    /// wherever bps was an integer dividing 1000, and exact (×10) on the 10 s-per-block network.
    fn subsidy_month(&self, daa_score: u64) -> u64 {
        let seconds_since_deflationary_phase_started = if self.crescendo_activation_daa_score < self.deflationary_phase_daa_score {
            // crescendo_activation < deflationary_phase <= daa_score (activated before deflation)
            (daa_score - self.deflationary_phase_daa_score) * self.ttpb_history.after() / 1000
        } else if daa_score < self.crescendo_activation_daa_score {
            // deflationary_phase <= daa_score < crescendo_activation (pre activation)
            (daa_score - self.deflationary_phase_daa_score) * self.ttpb_history.before() / 1000
        } else {
            // Else - deflationary_phase <= crescendo_activation <= daa_score.
            // Count seconds differently before and after Crescendo activation
            (self.crescendo_activation_daa_score - self.deflationary_phase_daa_score) * self.ttpb_history.before() / 1000
                + (daa_score - self.crescendo_activation_daa_score) * self.ttpb_history.after() / 1000
        };

        seconds_since_deflationary_phase_started / SECONDS_PER_MONTH
    }
}

/*
    kaspa-pq additional-issuance emission table.

    Tokenomics: 15B KAS of additional issuance over 20 years, decaying at a
    5%/year exponential rate (q = 0.95), on top of a 10B genesis premine for a
    25B final supply. The schedule steps once per year (12 identical months),
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
        // Simnet runs 10 bps (100 ms blocks); derive the integer bps from ttpb for the legacy
        // round-trip identity below.
        let testnet_11_bps = 1000 / SIMNET_PARAMS.target_time_per_block_history().before();
        let total_high_bps_rewards_rounded_up: u64 = pre_deflationary_rewards
            + SUBSIDY_BY_MONTH_TABLE.iter().map(|x| (x.div_ceil(testnet_11_bps) * testnet_11_bps) * SECONDS_PER_MONTH).sum::<u64>();

        let cbm = create_manager(&SIMNET_PARAMS);
        let blocks_per_second = 1000 / cbm.ttpb().before();
        let total_high_bps_rewards: u64 = pre_deflationary_rewards
            + cbm.subsidy_by_month_table_before.iter().map(|x| x * SECONDS_PER_MONTH * blocks_per_second).sum::<u64>();
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
                    (SUBSIDY_BY_MONTH_TABLE[i] * cbm.ttpb().before()).div_ceil(1000),
                    *x,
                    "{}: locally computed and precomputed values must match",
                    network_id
                );
            });
            cbm.subsidy_by_month_table_after.iter().enumerate().for_each(|(i, x)| {
                assert_eq!(
                    (SUBSIDY_BY_MONTH_TABLE[i] * cbm.ttpb().after()).div_ceil(1000),
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
        // against the 25B supply (1 part in ~4e8) and far below the MAX_SOMPI cap.
        // Sub-1-bps networks (devnet: 10_000 ms/block) have an EXACT per-block product
        // (×10) and therefore a surplus of exactly 0.
        for network_id in NetworkId::iter() {
            let cbm = create_manager(&network_id.into());
            let ttpb = Params::from(network_id).target_time_per_block_history().after();
            // Blocks per month = seconds-per-month * 1000 / ttpb (exact for every shipped ttpb).
            let blocks_per_month = SECONDS_PER_MONTH as u128 * 1000 / ttpb as u128;
            let net_total: u128 = cbm.subsidy_by_month_table_after.iter().map(|&x| x as u128 * blocks_per_month).sum();
            let surplus_kas = net_total as i128 / SOMPI_PER_KASPA as i128 - total_kas as i128;
            assert!((0..=64).contains(&surplus_kas), "{network_id}: rate rounding surplus {surplus_kas} KAS out of range");
        }
    }

    #[test]
    fn subsidy_test() {
        // Year-1 per-block subsidy at 10 BPS (100 ms) = table[0].div_ceil(10) ≈ 3.70468 KAS.
        const YEAR1_PER_BLOCK_10BPS: u64 = 370468345;
        // Year-1 per-block subsidy at 0.1 BPS (10 s) = table[0] * 10 ≈ 370.468 KAS — the same
        // 3.70468.. KAS/s emission RATE, paid in 100×-larger, 100×-rarer blocks.
        const YEAR1_PER_BLOCK_DECI_BPS: u64 = 37046834500;
        // Year-1 per-block subsidy at 120 s/block (the PALW public testnet) = table[0] * 120 ≈
        // 4445.62 KAS. Same rate again, in 1200×-larger, 1200×-rarer blocks.
        const YEAR1_PER_BLOCK_TWO_MINUTE: u64 = 444562014000;

        for network_id in NetworkId::iter() {
            let params: Params = network_id.into();
            let cbm = create_manager(&params);
            let ttpb = params.target_time_per_block_history().after();
            let blocks_per_month = SECONDS_PER_MONTH * 1000 / ttpb;

            // kaspa-pq has no flat pre-deflationary phase: the decay table applies from genesis.
            assert_eq!(params.deflationary_phase_daa_score, 0, "{network_id}: expected no pre-deflationary phase");

            // Genesis / year-1 subsidy.
            let expected_year1 = (SUBSIDY_BY_MONTH_TABLE[0] * ttpb).div_ceil(1000);
            assert_eq!(cbm.calc_block_subsidy(0), expected_year1, "{network_id}: genesis subsidy");
            // The invariant is the per-SECOND emission rate, not any per-block figure: a block
            // interval change must move the per-block subsidy in exact proportion, which is what
            // makes the 10 s → 120 s decision emission-neutral.
            assert_eq!(
                expected_year1 * 1000 / ttpb,
                SUBSIDY_BY_MONTH_TABLE[0],
                "{network_id}: year-1 emission rate must stay {} sompi/s",
                SUBSIDY_BY_MONTH_TABLE[0]
            );
            match ttpb {
                100 => assert_eq!(expected_year1, YEAR1_PER_BLOCK_10BPS, "{network_id}: year-1 per-block subsidy"),
                10_000 => assert_eq!(expected_year1, YEAR1_PER_BLOCK_DECI_BPS, "{network_id}: year-1 per-block subsidy"),
                120_000 => assert_eq!(expected_year1, YEAR1_PER_BLOCK_TWO_MINUTE, "{network_id}: year-1 per-block subsidy"),
                other => panic!("{network_id}: unexpected target time per block {other}"),
            }

            // Every emission month pays table[m] * ttpb / 1000 (rounded up), flat within the month
            // (stepped schedule: the same rate holds from the first to the last block of the month).
            // Index-based: `m` is both a table index and a DAA-score multiplier below.
            #[allow(clippy::needless_range_loop)]
            for m in 0..SUBSIDY_BY_MONTH_TABLE_SIZE - 1 {
                let daa = m as u64 * blocks_per_month;
                let expected = (SUBSIDY_BY_MONTH_TABLE[m] * ttpb).div_ceil(1000);
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
            params.target_time_per_block_history(),
        )
    }

    /// Return a CoinbaseManager with legacy golang 1 BPS (1000 ms/block) properties
    fn create_legacy_manager() -> CoinbaseManager {
        CoinbaseManager::new(150, 204, 15778800 - 259200, 50000000000, ForkedParam::new_const(1000))
    }
}
