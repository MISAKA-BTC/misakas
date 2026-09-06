//! **ADR-0089 — the fold is the truth; the EVM is its window and its hand.**
//!
//! The contract between the PALW state fold (consensus-core) and the EVM executor (`kaspa-evm`),
//! spelled once so neither side can drift from the other:
//!
//! * the four system addresses and the facade family (Decision 1);
//! * the **view** a read precompile serves — rows of the fold at the EVM block's selected
//!   parent, flattened so the executor needs no PALW type but these (Decision 2);
//! * an **action** the writer queues (Decision 5) and the **settlement** the fold answers with
//!   (Decision 6), which the selected child carries as a system op;
//! * the holder id an EVM account has in the market (Decision 7) and the sink outpoint a filled
//!   buy materialises (Decision 6).
//!
//! Every value here is a pure function of its arguments or a plain row; nothing here reads a
//! store or a clock.

use super::{EVM_ADDRESS_SIZE, EvmAddress};
use crate::Hash64;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::blake2b_512_keyed;
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---- addresses (Decision 1) --------------------------------------------------------------------

const fn system_address(low: u16) -> EvmAddress {
    let mut bytes = [0u8; EVM_ADDRESS_SIZE];
    bytes[EVM_ADDRESS_SIZE - 2] = (low >> 8) as u8;
    bytes[EVM_ADDRESS_SIZE - 1] = (low & 0xff) as u8;
    EvmAddress::from_bytes(bytes)
}

/// `0x…F010`: `ModelRegistry`, a read precompile.
pub const MISAKA_MODEL_REGISTRY_PRECOMPILE: EvmAddress = system_address(0xF010);
/// `0x…F011`: `ModelAMM`, a read precompile whose quotes are the fold's own arithmetic.
pub const MISAKA_MODEL_AMM_PRECOMPILE: EvmAddress = system_address(0xF011);
/// `0x…F012`: `ModelPosition`, a read precompile.
pub const MISAKA_MODEL_POSITION_PRECOMPILE: EvmAddress = system_address(0xF012);
/// `0x…F013`: `ModelWriter`, the hand — a call-frame intercept that escrows a buy's value and
/// queues an action; F002's shape, because a stateless precompile cannot see `msg.sender` or
/// `msg.value`.
pub const MISAKA_MODEL_WRITER: EvmAddress = system_address(0xF013);
/// The two-byte prefix of every line's facade address (`MP`), followed by 18 bytes of the
/// line's keyed digest (Decision 1).
pub const MISAKA_MODEL_FACADE_PREFIX: [u8; 2] = [0x4d, 0x50];
pub const MISAKA_MODEL_FACADE_DOMAIN_V1: &[u8] = b"misaka-evm/model-position-facade/v1";

/// Decision 1: the facade address of a line.
pub fn facade_address_v1(line_id: &Hash64) -> EvmAddress {
    let digest = blake2b_512_keyed(MISAKA_MODEL_FACADE_DOMAIN_V1, line_id.as_byte_slice());
    let mut bytes = [0u8; EVM_ADDRESS_SIZE];
    bytes[..2].copy_from_slice(&MISAKA_MODEL_FACADE_PREFIX);
    bytes[2..].copy_from_slice(&digest.as_bytes()[..EVM_ADDRESS_SIZE - 2]);
    EvmAddress::from_bytes(bytes)
}

/// Does an address carry the facade prefix? (Whether it names a line is the view's to say.)
pub fn is_facade_shaped(address: &EvmAddress) -> bool {
    address.as_bytes()[..2] == MISAKA_MODEL_FACADE_PREFIX
}

// ---- bounds and gas (Decision 5, §4) -----------------------------------------------------------

/// Actions an EVM block may queue; the 129th call reverts (Decision 5).
pub const MAX_MARKET_ACTIONS_PER_EVM_BLOCK: usize = 128;
/// System gas one `MarketSettle` op charges the settling block (§4: the deposit claim's figure).
pub const MARKET_SETTLE_GAS: u64 = 25_000;
/// Gas the writer burns before emitting its log (Decision 5; Hyperliquid's ~25,000 as the
/// starting schedule, F002's 9,000 the in-tree reference).
pub const PALW_EVM_WRITER_GAS_V1: u64 = 25_000;
/// The writer's encoding version byte (Decision 5).
pub const PALW_EVM_MARKET_ACTION_ENCODING_V1: u8 = 1;
/// Action ids (Decision 5).
pub const PALW_EVM_ACTION_BUY: u32 = 1;
pub const PALW_EVM_ACTION_SELL: u32 = 2;
/// ADR-0090 Decision 3: the seed that opens a line's market — the call's value, at least
/// `PALW_MODEL_SEED_MIN_SOMPI_V1`, becomes the reserve.
pub const PALW_EVM_ACTION_SEED: u32 = 3;

/// The four bytes of `keccak256("sendAction(bytes)")` — the writer's one function (Decision 5).
/// Spelled here, in the crate every client already links, so a wallet or a CLI can build the
/// call without the executor's crate; the executor's intercept asserts the same selector.
pub fn send_action_selector() -> [u8; 4] {
    abi_selector("sendAction(bytes)")
}

/// The four bytes of `keccak256(signature)` — an ABI selector, so a client can build a call to
/// any of the four doors from the signature strings `contracts/misaka-model/` spells.
pub fn abi_selector(signature: &str) -> [u8; 4] {
    use sha3::{Digest, Keccak256};
    let h = Keccak256::digest(signature.as_bytes());
    [h[0], h[1], h[2], h[3]]
}

fn abi_word_u64(v: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&v.to_be_bytes());
    w
}

/// `sendAction(bytes)` calldata around raw action bytes (standard dynamic encoding).
pub fn send_action_calldata(action: &[u8]) -> Vec<u8> {
    let mut input = send_action_selector().to_vec();
    input.extend_from_slice(&abi_word_u64(32));
    input.extend_from_slice(&abi_word_u64(action.len() as u64));
    input.extend_from_slice(action);
    input.extend(std::iter::repeat_n(0u8, action.len().div_ceil(32) * 32 - action.len()));
    input
}

/// The action bytes of a buy: `version ‖ id(3) ‖ line(64) ‖ minUnitsOut(32)`. The MSK paid is the
/// call's `value` (whole sompi × `EVM_NATIVE_SCALE`), never a field.
pub fn send_action_buy_calldata(line: &Hash64, min_units_out: u64) -> Vec<u8> {
    let mut data = vec![PALW_EVM_MARKET_ACTION_ENCODING_V1];
    data.extend_from_slice(&PALW_EVM_ACTION_BUY.to_be_bytes()[1..]);
    data.extend_from_slice(line.as_byte_slice());
    data.extend_from_slice(&abi_word_u64(min_units_out));
    send_action_calldata(&data)
}

/// The action bytes of a seed (ADR-0090): `version ‖ id(3) ‖ line(64)`; the seed is the call's
/// value (whole sompi × `EVM_NATIVE_SCALE`), at least `PALW_MODEL_SEED_MIN_SOMPI_V1`.
pub fn send_action_seed_calldata(line: &Hash64) -> Vec<u8> {
    let mut data = vec![PALW_EVM_MARKET_ACTION_ENCODING_V1];
    data.extend_from_slice(&PALW_EVM_ACTION_SEED.to_be_bytes()[1..]);
    data.extend_from_slice(line.as_byte_slice());
    send_action_calldata(&data)
}

/// The action bytes of a sell: `version ‖ id(3) ‖ line(64) ‖ unitsIn(32) ‖ minMskOutSompi(32)`;
/// the call carries no value.
pub fn send_action_sell_calldata(line: &Hash64, units_in: u64, min_msk_out_sompi: u64) -> Vec<u8> {
    let mut data = vec![PALW_EVM_MARKET_ACTION_ENCODING_V1];
    data.extend_from_slice(&PALW_EVM_ACTION_SELL.to_be_bytes()[1..]);
    data.extend_from_slice(line.as_byte_slice());
    data.extend_from_slice(&abi_word_u64(units_in));
    data.extend_from_slice(&abi_word_u64(min_msk_out_sompi));
    send_action_calldata(&data)
}

// ---- identities (Decisions 6 and 7) ------------------------------------------------------------

pub const PALW_EVM_HOLDER_DOMAIN_V1: &[u8] = b"misaka-palw/model-market/holder/evm/v1";
pub const MISAKA_EVM_MARKET_SINK_CONTEXT: &[u8] = b"MISAKA_EVM_MARKET_SINK_V1";

/// Decision 7: an EVM account's holder id in the market — a `Hash64` in the same field ADR-0087
/// keys positions by, in a namespace no ML-DSA holder can reach.
pub fn evm_holder_v1(chain_id: u64, address: &EvmAddress) -> Hash64 {
    let mut preimage = Vec::with_capacity(8 + EVM_ADDRESS_SIZE);
    preimage.extend_from_slice(&chain_id.to_le_bytes());
    preimage.extend_from_slice(&address.as_bytes());
    blake2b_512_keyed(PALW_EVM_HOLDER_DOMAIN_V1, &preimage)
}

/// Decision 6: the outpoint of the sink output a filled buy materialises in the settling block
/// — pre-mining-stable, like `synthetic_withdrawal_txid`: bound to the block whose fold decided
/// it (the settling block's selected parent), the action's sequence, the account, the line and
/// the amount.
pub fn synthetic_market_sink_txid(decided_at: Hash64, seq: u32, account: &EvmAddress, line_id: &Hash64, amount_sompi: u64) -> Hash64 {
    let mut preimage = Vec::with_capacity(64 + 4 + EVM_ADDRESS_SIZE + 64 + 8);
    preimage.extend_from_slice(decided_at.as_byte_slice());
    preimage.extend_from_slice(&seq.to_le_bytes());
    preimage.extend_from_slice(&account.as_bytes());
    preimage.extend_from_slice(line_id.as_byte_slice());
    preimage.extend_from_slice(&amount_sompi.to_le_bytes());
    blake2b_512_keyed(MISAKA_EVM_MARKET_SINK_CONTEXT, &preimage)
}

// ---- actions and settlements (Decisions 5 and 6) ---------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub enum PalwEvmMarketActionKindV1 {
    Buy {
        min_units_out: u64,
    },
    Sell {
        units_in: u64,
        min_msk_out_sompi: u64,
    },
    /// ADR-0090: the whole `gross_sompi` is the seed.
    Seed,
}

impl PalwEvmMarketActionKindV1 {
    pub fn action_id(&self) -> u32 {
        match self {
            Self::Buy { .. } => PALW_EVM_ACTION_BUY,
            Self::Sell { .. } => PALW_EVM_ACTION_SELL,
            Self::Seed => PALW_EVM_ACTION_SEED,
        }
    }
}

/// What the writer queued in `EVM(B)`, scanned from its `ActionQueued` logs in log order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwEvmMarketActionV1 {
    pub seq: u32,
    pub account: EvmAddress,
    pub line_id: Hash64,
    pub kind: PalwEvmMarketActionKindV1,
    /// A buy's escrowed value in sompi (`msg.value / EVM_NATIVE_SCALE`); zero for a sell.
    pub gross_sompi: u64,
}

impl MemSizeEstimator for PalwEvmMarketActionV1 {}

/// Why the fold refused an action (Decision 6); the facade emits it in `Refused`'s `reason`.
pub mod refusal {
    pub const NOT_ARMED: u8 = 1;
    pub const LINE_MISSING: u8 = 2;
    pub const NOT_ACTIVE: u8 = 3;
    pub const RELEASES_NOTHING: u8 = 4;
    pub const BELOW_FLOOR: u8 = 5;
    pub const MARKET_MISSING: u8 = 6;
    pub const EXCEEDS_POSITION: u8 = 7;
    pub const PAYS_NOTHING: u8 = 8;
    pub const OTHER: u8 = 9;
    /// ADR-0090: the line's market is already seeded.
    pub const ALREADY_SEEDED: u8 = 10;
    /// ADR-0090: the seed is under `PALW_MODEL_SEED_MIN_SOMPI_V1`.
    pub const SEED_TOO_SMALL: u8 = 11;
    /// ADR-0090: the line's class is frozen.
    pub const CLASS_CLOSED: u8 = 12;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub enum PalwEvmSettlementOutcomeV1 {
    /// A filled buy: `units` credited, `gross_sompi` sunk, the price after. A filled sell:
    /// `units` debited, `gross_sompi` taken from the reserve, `net_sompi` credited.
    Filled {
        units: u64,
        gross_sompi: u64,
        net_sompi: u64,
        price_after_sompi: u64,
    },
    Refused {
        reason: u8,
    },
}

/// What `fold(B)` decided about one action; carried by `EVM(C)` as `EvmSystemOp::MarketSettle`
/// and validated equal to the list derived from `fold(B)` (Decision 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwEvmSettlementV1 {
    pub seq: u32,
    pub account: EvmAddress,
    pub line_id: Hash64,
    /// Which move this settles (`PALW_EVM_ACTION_BUY` / `SELL` / `SEED`, ADR-0090). A buy and a
    /// seed carry escrow; a sell carries none.
    pub action: u32,
    /// The escrow a buy holds in the writer's account (sompi); zero for a sell.
    pub escrow_sompi: u64,
    pub outcome: PalwEvmSettlementOutcomeV1,
}

impl MemSizeEstimator for PalwEvmSettlementV1 {}

impl PalwEvmSettlementV1 {
    /// A buy's and a seed's value sits in the writer's escrow until the settlement burns or
    /// refunds it; a sell moved nothing in.
    pub fn carries_escrow(&self) -> bool {
        self.action != PALW_EVM_ACTION_SELL
    }
    pub fn is_buy(&self) -> bool {
        self.action == PALW_EVM_ACTION_BUY
    }
    pub fn is_seed(&self) -> bool {
        self.action == PALW_EVM_ACTION_SEED
    }
}

// ---- fences ------------------------------------------------------------------------------------

/// The fences the executor reads, resolved by consensus at the block's DAA (Decision 9) — and
/// by the RPC at the simulated head, so `eth_call` registers the same handlers (parity).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwEvmMarketFencesV1 {
    /// `Params::palw_model_market` (ADR-0087 D6).
    pub market_active: bool,
    /// `Params::palw_model_lines` (ADR-0088 D11).
    pub lines_active: bool,
    /// `Params::palw_model_evm` (ADR-0089 D9): the four addresses and the facades exist.
    pub evm_active: bool,
}

// ---- the view (Decision 2) ---------------------------------------------------------------------

/// A class row as the registry precompile serves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEvmClassRowV1 {
    /// 0 Active, 1 Frozen, 2 Registered, 3 Dormant.
    pub status: u8,
    pub share_permille: u16,
    pub budget_blocks: u64,
    pub canonical_leaves: u64,
    pub is_base: bool,
    pub registrant_payload: Option<Hash64>,
    pub registered_daa: u64,
    pub certified_attempt: bool,
    pub certified_fp: bool,
    /// ADR-0088 Decision 3 at the view's DAA.
    pub roots_in_force: Vec<Hash64>,
}

/// A line row as served (ADR-0088 D1), with bond payloads resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEvmLineRowV1 {
    pub class_id: Hash64,
    pub owner_payload: Option<Hash64>,
    pub developer_payload: Option<Hash64>,
    pub maintainer_payload: Option<Hash64>,
    pub name: Vec<u8>,
    pub founded_daa: u64,
    pub current: u32,
    pub versions_published: u32,
    pub preview_count: u32,
    pub contributor_permille_of_leg: u16,
    /// 0 Active, 1 Retired.
    pub status: u8,
}

/// A version row as served (ADR-0088 D2/D4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEvmVersionRowV1 {
    pub root: Hash64,
    pub parent: Option<u32>,
    pub adopted_from: Option<Hash64>,
    pub runtime_hash: Option<Hash64>,
    pub dataset_commitment: Option<Hash64>,
    pub training_config_hash: Option<Hash64>,
    pub notes_hash: Option<Hash64>,
    pub published_daa: u64,
    pub published_by_payload: Option<Hash64>,
    /// 0 Current, 1 Preview, 2 Superseded, 3 Withdrawn.
    pub status: u8,
    pub until_daa: u64,
    pub attempt_claims: u64,
    pub fp_claims: u64,
    pub work_leaves: u128,
    pub first_used_daa: Option<u64>,
    pub last_used_daa: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEvmProposalRowV1 {
    pub proposal_id: Hash64,
    pub root: Hash64,
    pub note_hash: Hash64,
    pub by_payload: Option<Hash64>,
    pub posted_daa: u64,
    pub adopted_in: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEvmEvaluationRowV1 {
    pub evaluator_id: Hash64,
    pub score_permille: u32,
    pub report_hash: Hash64,
    pub by_payload: Option<Hash64>,
    pub posted_daa: u64,
    pub is_lines_own: bool,
}

/// **The window (Decision 2): the fold's rows at the EVM block's selected parent**, built by
/// consensus (`PalwChainStateV2::evm_view_v1`) and handed to the executor and the RPC's
/// simulator alike. Every read precompile answers from this and nothing else.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwEvmViewV1 {
    pub chain_daa: u64,
    pub chain_id: u64,
    /// In class-id order — `classAt(i)`.
    pub classes: Vec<(Hash64, PalwEvmClassRowV1)>,
    /// Every line, founding lines included (synthesised), in line-id order — `lineAt(i)`.
    pub lines: Vec<(Hash64, PalwEvmLineRowV1)>,
    pub versions: BTreeMap<(Hash64, u32), PalwEvmVersionRowV1>,
    pub proposals: BTreeMap<Hash64, Vec<PalwEvmProposalRowV1>>,
    pub evaluations: BTreeMap<(Hash64, u32), Vec<PalwEvmEvaluationRowV1>>,
    pub markets: BTreeMap<Hash64, crate::palw_model_market_v1::PalwModelMarketV1>,
    pub positions: BTreeMap<(Hash64, Hash64), u64>,
}

impl PalwEvmViewV1 {
    pub fn class(&self, class_id: &Hash64) -> Option<&PalwEvmClassRowV1> {
        self.classes.binary_search_by(|(id, _)| id.cmp(class_id)).ok().map(|i| &self.classes[i].1)
    }
    pub fn line(&self, line_id: &Hash64) -> Option<&PalwEvmLineRowV1> {
        self.lines.binary_search_by(|(id, _)| id.cmp(line_id)).ok().map(|i| &self.lines[i].1)
    }
    /// The line a facade address names, if any.
    pub fn line_of_facade(&self, address: &EvmAddress) -> Option<Hash64> {
        if !is_facade_shaped(address) {
            return None;
        }
        self.lines.iter().map(|(id, _)| *id).find(|id| facade_address_v1(id) == *address)
    }
    pub fn position(&self, line_id: &Hash64, holder: &Hash64) -> u64 {
        self.positions.get(&(*line_id, *holder)).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_addresses_are_where_the_adr_puts_them() {
        assert_eq!(&MISAKA_MODEL_REGISTRY_PRECOMPILE.as_bytes()[18..], &[0xF0, 0x10]);
        assert_eq!(&MISAKA_MODEL_WRITER.as_bytes()[18..], &[0xF0, 0x13]);
        assert!(MISAKA_MODEL_REGISTRY_PRECOMPILE.as_bytes()[..18].iter().all(|b| *b == 0));
        let line = Hash64::from_u64_word(7);
        let facade = facade_address_v1(&line);
        assert!(is_facade_shaped(&facade));
        assert_ne!(facade, facade_address_v1(&Hash64::from_u64_word(8)));
        assert!(!is_facade_shaped(&MISAKA_MODEL_WRITER));
    }

    #[test]
    fn the_holder_id_binds_the_chain_and_the_account() {
        let a = EvmAddress::from_bytes([1u8; 20]);
        assert_ne!(evm_holder_v1(1, &a), evm_holder_v1(2, &a));
        assert_ne!(evm_holder_v1(1, &a), evm_holder_v1(1, &EvmAddress::from_bytes([2u8; 20])));
        let sink = synthetic_market_sink_txid(Hash64::from_u64_word(1), 0, &a, &Hash64::from_u64_word(9), 100);
        assert_ne!(sink, synthetic_market_sink_txid(Hash64::from_u64_word(1), 1, &a, &Hash64::from_u64_word(9), 100));
        assert_ne!(sink, synthetic_market_sink_txid(Hash64::from_u64_word(2), 0, &a, &Hash64::from_u64_word(9), 100));
    }

    #[test]
    fn the_view_answers_by_id_and_by_facade() {
        let line = Hash64::from_u64_word(3);
        let row = PalwEvmLineRowV1 {
            class_id: line,
            owner_payload: None,
            developer_payload: None,
            maintainer_payload: None,
            name: b"x".to_vec(),
            founded_daa: 0,
            current: 1,
            versions_published: 1,
            preview_count: 0,
            contributor_permille_of_leg: 0,
            status: 0,
        };
        let view = PalwEvmViewV1 { lines: vec![(line, row)], ..Default::default() };
        assert!(view.line(&line).is_some());
        assert_eq!(view.line_of_facade(&facade_address_v1(&line)), Some(line));
        assert_eq!(view.line_of_facade(&facade_address_v1(&Hash64::from_u64_word(4))), None);
        assert_eq!(view.position(&line, &Hash64::from_u64_word(1)), 0);
    }
}
