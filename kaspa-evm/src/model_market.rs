//! **ADR-0089 — the fold is the truth; the EVM is its window and its hand.**
//!
//! The executor's half of the model market: three read precompiles (Decision 2), the MRC-20
//! facade family (Decision 3), the writer (Decision 5) and the settlement receipt (Decision 6).
//! Every value served here comes from [`PalwEvmViewV1`] — the fold's rows at the EVM block's
//! selected parent, built by consensus — and nothing here reads a store or a clock.
//!
//! Two mechanisms, one seam. Reads and writes are both **call-frame intercepts** (F002's shape:
//! `handler.execution.call` is wrapped, so the handler sees `msg.sender` and `msg.value`, which
//! the stateless precompile ABI cannot). They are installed by
//! [`crate::precompiles::register_all_misaka_precompiles`] — the one place consensus, `eth_call`
//! and the tracer register MISAKA's handlers — and ONLY when `fences.evm_active` (Decision 9):
//! below the fence nothing is registered, so the four addresses and every facade are empty
//! accounts, byte-identical to before (the F003 idiom).
//!
//! **Encoding rule** (overrides the ADR's prose): every 64-byte id (line, class, holder, payload,
//! root, hash) crosses the ABI as two `bytes32` words, high half first (`xA`, `xB`). No dynamic
//! ABI types except `string` for the facade's `name()`/`symbol()`, `bytes` for `sendAction(bytes)`
//! and the `ActionQueued` data, and `bytes4` for `supportsInterface`. Selectors are the first four
//! bytes of keccak256 over the canonical signature strings spelled in this file.
//!
//! **Gas.** A read costs `2,000 + 65 × (input_len + output_len)` (the reference's schedule, to be
//! re-measured before the fence is armed — ADR §10); malformed input reverts and consumes the
//! frame's gas; an unknown id answers zeros, because the zero is itself a fact. The writer burns
//! [`PALW_EVM_WRITER_GAS_V1`] before anything else; its refusals are ordinary reverts carrying a
//! four-byte error selector (`NotAnAccount()`, `BadValue()`, …) so a wallet can name the fault.
//!
//! **The writer's log.** `ActionQueued(address indexed account, uint8 actionId, bytes data)` from
//! `0x…F013`. `data` is the writer's *normalised record* of the action — not the caller's raw bytes:
//! `[1][id u24 BE][lineA][lineB][w3][w4]` (132 bytes) where a buy's `w3`/`w4` are
//! `minUnitsOut`/`grossSompi` and a sell's are `unitsIn`/`minMskOutSompi`. The record carries the
//! escrowed gross for the same reason F002's log carries its amount: the executor rebuilds the
//! block's action list from committed logs alone ([`decode_action_log`]), and a facade `buy` has no
//! caller bytes at all. A reverted tx leaves no log, and the transfer into escrow and the log sit in
//! the same journal, so they unwind together.

use kaspa_consensus_core::evm::model_market::{
    MAX_MARKET_ACTIONS_PER_EVM_BLOCK, MISAKA_MODEL_AMM_PRECOMPILE, MISAKA_MODEL_POSITION_PRECOMPILE, MISAKA_MODEL_REGISTRY_PRECOMPILE,
    MISAKA_MODEL_WRITER, PALW_EVM_ACTION_BUY, PALW_EVM_ACTION_SEED, PALW_EVM_ACTION_SELL, PALW_EVM_MARKET_ACTION_ENCODING_V1,
    PALW_EVM_WRITER_GAS_V1, PalwEvmMarketActionKindV1, PalwEvmMarketActionV1, PalwEvmMarketFencesV1, PalwEvmSettlementOutcomeV1,
    PalwEvmSettlementV1, PalwEvmViewV1, evm_holder_v1, facade_address_v1,
};
use kaspa_consensus_core::evm::{EVM_NATIVE_SCALE, EvmAddress, EvmLog};
use kaspa_consensus_core::palw_model_market_v1::{
    PALW_MODEL_BURN_PERMILLE_V1, PALW_MODEL_POSITION_UNITS_V1, PALW_MODEL_REGISTRANT_PERMILLE_V1, PALW_MODEL_SEED_MIN_SOMPI_V1,
    PALW_MODEL_SUPPLY_UNITS_V1, PalwModelMarketV1, palw_model_buy_quote_v1, palw_model_sell_quote_v1,
};
use kaspa_hashes::{EvmH256, Hash64};
use revm::handler::register::EvmHandler;
use revm::interpreter::{CallInputs, CallOutcome, Gas, InstructionResult, InterpreterResult};
use revm::primitives::{Address, B256, Bytes, Log, LogData, U256, keccak256};
use revm::{Database, FrameOrResult, FrameResult};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

// ---- gas (Decision 2; ADR §5, §10) ----------------------------------------------------------

/// The fixed part of a read's gas: `2,000 + 65 × (input_len + output_len)`.
pub const PALW_EVM_READ_BASE_GAS_V1: u64 = 2_000;
/// The per-byte part of a read's gas, over input and output alike.
pub const PALW_EVM_READ_GAS_PER_BYTE_V1: u64 = 65;

/// The writer's normalised action record: `[1][id u24][lineA][lineB][w3][w4]`.
const ACTION_RECORD_LEN: usize = 4 + 64 + 32 + 32;
/// `ACTION_RECORD_LEN` rounded up to a word.
const ACTION_RECORD_PADDED_LEN: usize = 160;
/// `ActionQueued` data: `word(actionId) ‖ word(0x40) ‖ word(len) ‖ record ‖ padding`.
const ACTION_LOG_DATA_LEN: usize = 32 * 3 + ACTION_RECORD_PADDED_LEN;

// ---- addresses -------------------------------------------------------------------------------

fn to_address(a: &EvmAddress) -> Address {
    Address::from(a.as_bytes())
}

/// `0x…F010` as a revm address.
pub fn registry_address() -> Address {
    to_address(&MISAKA_MODEL_REGISTRY_PRECOMPILE)
}
/// `0x…F011` as a revm address.
pub fn amm_address() -> Address {
    to_address(&MISAKA_MODEL_AMM_PRECOMPILE)
}
/// `0x…F012` as a revm address.
pub fn position_address() -> Address {
    to_address(&MISAKA_MODEL_POSITION_PRECOMPILE)
}
/// `0x…F013` — the writer and its escrow — as a revm address.
pub fn writer_address() -> Address {
    to_address(&MISAKA_MODEL_WRITER)
}

// ---- selectors, topics, error selectors --------------------------------------------------------

fn selector(signature: &str) -> [u8; 4] {
    let h = keccak256(signature.as_bytes());
    [h[0], h[1], h[2], h[3]]
}

/// Every selector the four doors answer, computed once. The signature strings ARE the ABI;
/// change one and the selector moves with it.
struct Selectors {
    // ModelRegistry (0x…F010)
    class_count: [u8; 4],
    class_at: [u8; 4],
    class_row: [u8; 4],
    certified: [u8; 4],
    roots_in_force_count: [u8; 4],
    root_in_force_at: [u8; 4],
    line_count: [u8; 4],
    line_at: [u8; 4],
    lines_of_count: [u8; 4],
    line_of_class_at: [u8; 4],
    line: [u8; 4],
    version: [u8; 4],
    usage: [u8; 4],
    evaluation_count: [u8; 4],
    evaluation: [u8; 4],
    proposal_count: [u8; 4],
    proposal: [u8; 4],
    facade_of: [u8; 4],
    line_of: [u8; 4],
    chain_daa: [u8; 4],
    // ModelAMM (0x…F011)
    market: [u8; 4],
    price_of: [u8; 4],
    quote_buy_of: [u8; 4],
    quote_sell_of: [u8; 4],
    constants: [u8; 4],
    // ModelPosition (0x…F012)
    balance_of_holder: [u8; 4],
    balance_of_address: [u8; 4],
    total_supply_of: [u8; 4],
    sold: [u8; 4],
    holder_id_of: [u8; 4],
    // ModelWriter (0x…F013)
    send_action: [u8; 4],
    // ADR-0090: the facade's seed door
    seed: [u8; 4],
    // IMRC20 (the facade family)
    name: [u8; 4],
    symbol: [u8; 4],
    decimals: [u8; 4],
    total_supply: [u8; 4],
    balance_of: [u8; 4],
    line_id: [u8; 4],
    circulating: [u8; 4],
    price: [u8; 4],
    quote_buy: [u8; 4],
    quote_sell: [u8; 4],
    buy: [u8; 4],
    sell: [u8; 4],
    supports_interface: [u8; 4],
    transfer: [u8; 4],
    transfer_from: [u8; 4],
    approve: [u8; 4],
    allowance: [u8; 4],
    // events
    action_queued: B256,
    bought: B256,
    sold_event: B256,
    refused: B256,
    seeded: B256,
    // errors
    not_an_account: [u8; 4],
    bad_value: [u8; 4],
    closed_to_buys: [u8; 4],
    non_transferable: [u8; 4],
    market_not_active: [u8; 4],
    unknown_line: [u8; 4],
    too_many_actions: [u8; 4],
    bad_input: [u8; 4],
    seed_too_small: [u8; 4],
    // ERC-165
    imrc20_interface_id: [u8; 4],
}

fn sel() -> &'static Selectors {
    static SEL: OnceLock<Selectors> = OnceLock::new();
    SEL.get_or_init(|| {
        let name = selector("name()");
        let symbol = selector("symbol()");
        let decimals = selector("decimals()");
        let total_supply = selector("totalSupply()");
        let balance_of = selector("balanceOf(address)");
        let line_id = selector("lineId()");
        let circulating = selector("circulating()");
        let price = selector("price()");
        let quote_buy = selector("quoteBuy(uint256)");
        let quote_sell = selector("quoteSell(uint256)");
        let buy = selector("buy(uint256)");
        let sell = selector("sell(uint256,uint256)");
        let seed = selector("seed()");
        // ERC-165: the XOR of every IMRC20 function selector except supportsInterface itself.
        let mut imrc20 = [0u8; 4];
        for s in [name, symbol, decimals, total_supply, balance_of, line_id, circulating, price, quote_buy, quote_sell, buy, sell] {
            for (i, b) in s.iter().enumerate() {
                imrc20[i] ^= b;
            }
        }
        Selectors {
            class_count: selector("classCount()"),
            class_at: selector("classAt(uint256)"),
            class_row: selector("classRow(bytes32,bytes32)"),
            certified: selector("certified(bytes32,bytes32,uint8)"),
            roots_in_force_count: selector("rootsInForceCount(bytes32,bytes32)"),
            root_in_force_at: selector("rootInForceAt(bytes32,bytes32,uint32)"),
            line_count: selector("lineCount()"),
            line_at: selector("lineAt(uint256)"),
            lines_of_count: selector("linesOfCount(bytes32,bytes32)"),
            line_of_class_at: selector("lineOfClassAt(bytes32,bytes32,uint32)"),
            line: selector("line(bytes32,bytes32)"),
            version: selector("version(bytes32,bytes32,uint32)"),
            usage: selector("usage(bytes32,bytes32,uint32)"),
            evaluation_count: selector("evaluationCount(bytes32,bytes32,uint32)"),
            evaluation: selector("evaluation(bytes32,bytes32,uint32,uint32)"),
            proposal_count: selector("proposalCount(bytes32,bytes32)"),
            proposal: selector("proposal(bytes32,bytes32,uint32)"),
            facade_of: selector("facadeOf(bytes32,bytes32)"),
            line_of: selector("lineOf(address)"),
            chain_daa: selector("chainDaa()"),
            market: selector("market(bytes32,bytes32)"),
            price_of: selector("price(bytes32,bytes32)"),
            quote_buy_of: selector("quoteBuy(bytes32,bytes32,uint64)"),
            quote_sell_of: selector("quoteSell(bytes32,bytes32,uint64)"),
            constants: selector("constants()"),
            balance_of_holder: selector("balanceOf(bytes32,bytes32,bytes32,bytes32)"),
            balance_of_address: selector("balanceOfAddress(bytes32,bytes32,address)"),
            total_supply_of: selector("totalSupply(bytes32,bytes32)"),
            sold: selector("sold(bytes32,bytes32)"),
            holder_id_of: selector("holderIdOf(address)"),
            send_action: selector("sendAction(bytes)"),
            seed,
            name,
            symbol,
            decimals,
            total_supply,
            balance_of,
            line_id,
            circulating,
            price,
            quote_buy,
            quote_sell,
            buy,
            sell,
            supports_interface: selector("supportsInterface(bytes4)"),
            transfer: selector("transfer(address,uint256)"),
            transfer_from: selector("transferFrom(address,address,uint256)"),
            approve: selector("approve(address,uint256)"),
            allowance: selector("allowance(address,address)"),
            action_queued: keccak256(b"ActionQueued(address,uint8,bytes)"),
            bought: keccak256(b"Bought(address,uint256,uint256,uint256)"),
            sold_event: keccak256(b"Sold(address,uint256,uint256,uint256)"),
            refused: keccak256(b"Refused(address,uint8,uint256,bytes32)"),
            seeded: keccak256(b"Seeded(address,uint256,uint256)"),
            not_an_account: selector("NotAnAccount()"),
            bad_value: selector("BadValue()"),
            closed_to_buys: selector("ClosedToBuys()"),
            non_transferable: selector("NonTransferable()"),
            market_not_active: selector("MarketNotActive()"),
            unknown_line: selector("UnknownLine()"),
            too_many_actions: selector("TooManyActions()"),
            bad_input: selector("BadInput()"),
            seed_too_small: selector("SeedTooSmall()"),
            imrc20_interface_id: imrc20,
        }
    })
}

/// The writer's event topic: `keccak256("ActionQueued(address,uint8,bytes)")`. Frozen at
/// activation (it is part of the committed receipts).
pub fn action_queued_topic() -> B256 {
    sel().action_queued
}
/// `keccak256("Bought(address,uint256,uint256,uint256)")`.
pub fn bought_topic() -> B256 {
    sel().bought
}
/// `keccak256("Sold(address,uint256,uint256,uint256)")`.
pub fn sold_topic() -> B256 {
    sel().sold_event
}
/// `keccak256("Refused(address,uint8,uint256,bytes32)")`.
pub fn refused_topic() -> B256 {
    sel().refused
}
pub fn seeded_topic() -> B256 {
    sel().seeded
}
/// ERC-165 id of `IMRC20`: the XOR of its twelve function selectors (`supportsInterface` excluded).
pub fn imrc20_interface_id() -> [u8; 4] {
    sel().imrc20_interface_id
}
/// The ERC-165 interface id of ERC-165 itself.
pub const ERC165_INTERFACE_ID: [u8; 4] = [0x01, 0xff, 0xc9, 0xa7];
/// The ERC-165 interface id of ERC-20 — which the facade answers `false` to, on purpose (ADR §3).
pub const ERC20_INTERFACE_ID: [u8; 4] = [0x36, 0x37, 0x2b, 0x07];

/// Four-byte error selectors a refused call reverts with.
pub mod errors {
    /// `NotAnAccount()`: the caller is not the signing account, or has code (Decision 4).
    pub fn not_an_account() -> [u8; 4] {
        super::sel().not_an_account
    }
    /// `BadValue()`: a buy without a positive sompi-exact value, a sell with value, a sell of zero,
    /// a field beyond `u64`, or a balance short of the value.
    pub fn bad_value() -> [u8; 4] {
        super::sel().bad_value
    }
    /// `ClosedToBuys()`: a buy on a line whose market is closed (ADR-0087 D7).
    pub fn closed_to_buys() -> [u8; 4] {
        super::sel().closed_to_buys
    }
    /// `NonTransferable()`: ERC-20's transfer half does not exist (Decision 4).
    pub fn non_transferable() -> [u8; 4] {
        super::sel().non_transferable
    }
    /// `MarketNotActive()`: `palw_model_market` is not armed at this block.
    pub fn market_not_active() -> [u8; 4] {
        super::sel().market_not_active
    }
    /// `UnknownLine()`: the line is not in the view.
    pub fn unknown_line() -> [u8; 4] {
        super::sel().unknown_line
    }
    /// `TooManyActions()`: the block already queued `MAX_MARKET_ACTIONS_PER_EVM_BLOCK` actions.
    pub fn too_many_actions() -> [u8; 4] {
        super::sel().too_many_actions
    }
    /// `BadInput()`: malformed calldata (a read consumes the frame's gas with it).
    pub fn bad_input() -> [u8; 4] {
        super::sel().bad_input
    }
    pub fn seed_too_small() -> [u8; 4] {
        super::sel().seed_too_small
    }
}

// ---- ABI words ---------------------------------------------------------------------------------

type Word = [u8; 32];

fn word_u128(v: u128) -> Word {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&v.to_be_bytes());
    w
}
fn word_u64(v: u64) -> Word {
    word_u128(v as u128)
}
fn word_bool(b: bool) -> Word {
    word_u64(b as u64)
}
fn word_address(a: &Address) -> Word {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a.as_slice());
    w
}
fn word_bytes4(b: [u8; 4]) -> Word {
    let mut w = [0u8; 32];
    w[..4].copy_from_slice(&b);
    w
}
/// A 64-byte id as its two words, high half first.
fn hash_words(h: &Hash64) -> (Word, Word) {
    let b = h.as_byte_slice();
    let mut a = [0u8; 32];
    let mut c = [0u8; 32];
    a.copy_from_slice(&b[..32]);
    c.copy_from_slice(&b[32..]);
    (a, c)
}
fn hash_from_words(a: &[u8], b: &[u8]) -> Hash64 {
    let mut bytes = [0u8; 64];
    bytes[..32].copy_from_slice(a);
    bytes[32..].copy_from_slice(b);
    Hash64::from_bytes(bytes)
}

/// A growing ABI return buffer.
#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn word(mut self, w: Word) -> Self {
        self.0.extend_from_slice(&w);
        self
    }
    fn u64(self, v: u64) -> Self {
        self.word(word_u64(v))
    }
    fn u128(self, v: u128) -> Self {
        self.word(word_u128(v))
    }
    fn bool(self, b: bool) -> Self {
        self.word(word_bool(b))
    }
    fn address(self, a: &Address) -> Self {
        self.word(word_address(a))
    }
    /// Two words; zeros when there is no id.
    fn hash(self, h: Option<&Hash64>) -> Self {
        match h {
            Some(h) => {
                let (a, b) = hash_words(h);
                self.word(a).word(b)
            }
            None => self.word([0u8; 32]).word([0u8; 32]),
        }
    }
    /// A single `string` return: offset, length, padded bytes.
    fn string(mut self, s: &str) -> Self {
        self.0.extend_from_slice(&word_u64(32));
        self.0.extend_from_slice(&word_u64(s.len() as u64));
        self.0.extend_from_slice(s.as_bytes());
        let pad = s.len().div_ceil(32) * 32 - s.len();
        self.0.extend(std::iter::repeat_n(0u8, pad));
        self
    }
    fn zeros(self, words: usize) -> Self {
        let mut out = self;
        for _ in 0..words {
            out = out.word([0u8; 32]);
        }
        out
    }
    fn finish(self) -> Vec<u8> {
        self.0
    }
}

/// Malformed calldata: a read reverts consuming the frame's gas, the writer reverts `BadInput()`.
struct Malformed;

/// Strictly-sized static arguments: `input.len() == 4 + 32 × n`.
struct Args<'a>(&'a [u8]);

impl<'a> Args<'a> {
    fn parse(input: &'a [u8], n: usize) -> Result<Self, Malformed> {
        if input.len() != 4 + 32 * n {
            return Err(Malformed);
        }
        Ok(Args(&input[4..]))
    }
    fn word(&self, i: usize) -> &'a [u8] {
        &self.0[32 * i..32 * (i + 1)]
    }
    fn hash64(&self, i: usize) -> Hash64 {
        hash_from_words(self.word(i), self.word(i + 1))
    }
    /// An unsigned integer of `bytes` bytes: the leading `32 − bytes` bytes must be zero.
    fn uint(&self, i: usize, bytes: usize) -> Result<u128, Malformed> {
        let w = self.word(i);
        if w[..32 - bytes].iter().any(|b| *b != 0) {
            return Err(Malformed);
        }
        let mut v = 0u128;
        for b in &w[32 - bytes..] {
            v = (v << 8) | *b as u128;
        }
        Ok(v)
    }
    fn u64(&self, i: usize) -> Result<u64, Malformed> {
        self.uint(i, 8).map(|v| v as u64)
    }
    fn u32(&self, i: usize) -> Result<u32, Malformed> {
        self.uint(i, 4).map(|v| v as u32)
    }
    fn u8(&self, i: usize) -> Result<u8, Malformed> {
        self.uint(i, 1).map(|v| v as u8)
    }
    /// A `uint256`; `None` when it does not fit `u64` (a valid word, an impossible amount).
    fn u256_as_u64(&self, i: usize) -> Option<u64> {
        self.uint(i, 8).ok().map(|v| v as u64)
    }
    fn u256(&self, i: usize) -> U256 {
        U256::from_be_slice(self.word(i))
    }
    fn address(&self, i: usize) -> Result<Address, Malformed> {
        let w = self.word(i);
        if w[..12].iter().any(|b| *b != 0) {
            return Err(Malformed);
        }
        Ok(Address::from_slice(&w[12..]))
    }
    fn bytes4(&self, i: usize) -> Result<[u8; 4], Malformed> {
        let w = self.word(i);
        if w[4..].iter().any(|b| *b != 0) {
            return Err(Malformed);
        }
        Ok([w[0], w[1], w[2], w[3]])
    }
}

// ---- the handlers ------------------------------------------------------------------------------

/// Which door a call knocked on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Door {
    Registry,
    Amm,
    Position,
    Writer,
    Facade(Hash64),
}

/// A decoded action, before the writer's checks.
#[derive(Clone, Copy, Debug)]
enum Action {
    Buy {
        min_units_out: U256,
    },
    Sell {
        units_in: U256,
        min_msk_out_sompi: U256,
    },
    /// ADR-0090: the seed — the call's value is the whole of it.
    Seed,
}

/// What a facade call turned out to be.
enum FacadeCall {
    View(Vec<u8>),
    NonTransferable,
    Write(Action),
}

/// **The window and the hand, bound to one block's view.** Built once per EVM block (or per
/// `eth_call`) from the selected parent's fold rows; cloned into every handler closure. `queued`
/// is the block's running action count: the intercept refuses the 129th action, and the executor
/// re-syncs it to the committed count after every tx so a reverted or skipped tx never counts.
#[derive(Clone)]
pub struct MarketHandlers {
    view: Arc<PalwEvmViewV1>,
    fences: PalwEvmMarketFencesV1,
    chain_id: u64,
    /// `facade_address_v1(line) → line`, for every line in the view.
    facades: Arc<HashMap<Address, Hash64>>,
    queued: Arc<AtomicUsize>,
}

impl MarketHandlers {
    /// Bind the view. `chain_id` is the holder-id domain (Decision 7); consensus passes
    /// `EVM_CHAIN_ID`.
    pub fn new(view: Arc<PalwEvmViewV1>, fences: PalwEvmMarketFencesV1, chain_id: u64) -> Self {
        let facades = view.lines.iter().map(|(id, _)| (to_address(&facade_address_v1(id)), *id)).collect();
        Self { view, fences, chain_id, facades: Arc::new(facades), queued: Arc::new(AtomicUsize::new(0)) }
    }

    pub fn fences(&self) -> PalwEvmMarketFencesV1 {
        self.fences
    }

    /// The block's action count as the intercept sees it.
    pub fn queued(&self) -> usize {
        self.queued.load(Ordering::SeqCst)
    }

    /// Re-sync the running count to the committed action list (the executor, after every tx).
    pub fn set_queued(&self, n: usize) {
        self.queued.store(n, Ordering::SeqCst)
    }

    /// The holder id of an EVM account in this chain's namespace (Decision 7).
    pub fn holder_of(&self, account: &Address) -> Hash64 {
        evm_holder_v1(self.chain_id, &EvmAddress::from_bytes(account.into_array()))
    }

    fn door(&self, target: &Address) -> Option<Door> {
        if *target == registry_address() {
            Some(Door::Registry)
        } else if *target == amm_address() {
            Some(Door::Amm)
        } else if *target == position_address() {
            Some(Door::Position)
        } else if *target == writer_address() {
            Some(Door::Writer)
        } else {
            self.facades.get(target).map(|line| Door::Facade(*line))
        }
    }

    /// The market row a quote is taken against: the fold's row, or — for a line the fold knows
    /// but nobody has seeded — a zero row that quotes nothing (ADR-0090: a market opens by a seed).
    /// `None` for a line the fold does not know. The bool is `exists`.
    fn market_row(&self, line: &Hash64) -> Option<(PalwModelMarketV1, bool)> {
        if let Some(m) = self.view.markets.get(line) {
            Some((*m, true))
        } else if self.view.line(line).is_some() {
            Some((PalwModelMarketV1::seed_v1(self.view.chain_daa, 0, Hash64::default()), false))
        } else {
            None
        }
    }

    fn lines_of_class(&self, class: &Hash64) -> impl Iterator<Item = &Hash64> {
        self.view.lines.iter().filter(move |(_, row)| row.class_id == *class).map(|(id, _)| id)
    }

    // ---- ModelRegistry (0x…F010) ----------------------------------------------------------

    fn registry(&self, input: &[u8]) -> Result<Vec<u8>, Malformed> {
        let s = sel();
        let selector: [u8; 4] = input.get(..4).ok_or(Malformed)?.try_into().map_err(|_| Malformed)?;
        let view = &*self.view;
        Ok(match selector {
            x if x == s.class_count => {
                Args::parse(input, 0)?;
                Out::default().u64(view.classes.len() as u64).finish()
            }
            x if x == s.class_at => {
                let a = Args::parse(input, 1)?;
                let id = a.u256_as_u64(0).and_then(|i| view.classes.get(i as usize)).map(|(id, _)| id);
                Out::default().hash(id).finish()
            }
            x if x == s.class_row => {
                let a = Args::parse(input, 2)?;
                match view.class(&a.hash64(0)) {
                    Some(c) => Out::default()
                        .u64(c.status as u64)
                        .u64(c.share_permille as u64)
                        .u64(c.budget_blocks)
                        .u64(c.canonical_leaves)
                        .bool(c.is_base)
                        .hash(c.registrant_payload.as_ref())
                        .u64(c.registered_daa)
                        .finish(),
                    None => Out::default().zeros(8).finish(),
                }
            }
            x if x == s.certified => {
                let a = Args::parse(input, 3)?;
                let lane = a.u8(2)?;
                let c = view.class(&a.hash64(0));
                let yes = match (c, lane) {
                    (Some(c), 0) => c.certified_attempt,
                    (Some(c), 1) => c.certified_fp,
                    _ => false,
                };
                Out::default().bool(yes).finish()
            }
            x if x == s.roots_in_force_count => {
                let a = Args::parse(input, 2)?;
                let n = view.class(&a.hash64(0)).map(|c| c.roots_in_force.len()).unwrap_or(0);
                Out::default().u64(n as u64).finish()
            }
            x if x == s.root_in_force_at => {
                let a = Args::parse(input, 3)?;
                let i = a.u32(2)? as usize;
                let root = view.class(&a.hash64(0)).and_then(|c| c.roots_in_force.get(i));
                Out::default().hash(root).finish()
            }
            x if x == s.line_count => {
                Args::parse(input, 0)?;
                Out::default().u64(view.lines.len() as u64).finish()
            }
            x if x == s.line_at => {
                let a = Args::parse(input, 1)?;
                let id = a.u256_as_u64(0).and_then(|i| view.lines.get(i as usize)).map(|(id, _)| id);
                Out::default().hash(id).finish()
            }
            x if x == s.lines_of_count => {
                let a = Args::parse(input, 2)?;
                let n = self.lines_of_class(&a.hash64(0)).count();
                Out::default().u64(n as u64).finish()
            }
            x if x == s.line_of_class_at => {
                let a = Args::parse(input, 3)?;
                let i = a.u32(2)? as usize;
                let id = self.lines_of_class(&a.hash64(0)).nth(i);
                Out::default().hash(id).finish()
            }
            x if x == s.line => {
                let a = Args::parse(input, 2)?;
                match view.line(&a.hash64(0)) {
                    Some(l) => Out::default()
                        .hash(Some(&l.class_id))
                        .hash(l.owner_payload.as_ref())
                        .hash(l.developer_payload.as_ref())
                        .hash(l.maintainer_payload.as_ref())
                        .u64(l.current as u64)
                        .u64(l.versions_published as u64)
                        .u64(l.preview_count as u64)
                        .u64(l.contributor_permille_of_leg as u64)
                        .u64(l.status as u64)
                        .word(keccak256(&l.name).0)
                        .finish(),
                    None => Out::default().zeros(14).finish(),
                }
            }
            x if x == s.version => {
                let a = Args::parse(input, 3)?;
                let n = a.u32(2)?;
                match view.versions.get(&(a.hash64(0), n)) {
                    Some(v) => Out::default()
                        .hash(Some(&v.root))
                        .u64(v.parent.unwrap_or(0) as u64)
                        .hash(v.adopted_from.as_ref())
                        .hash(v.runtime_hash.as_ref())
                        .hash(v.dataset_commitment.as_ref())
                        .hash(v.training_config_hash.as_ref())
                        .hash(v.notes_hash.as_ref())
                        .u64(v.published_daa)
                        .hash(v.published_by_payload.as_ref())
                        .u64(v.status as u64)
                        .u64(v.until_daa)
                        .finish(),
                    None => Out::default().zeros(18).finish(),
                }
            }
            x if x == s.usage => {
                let a = Args::parse(input, 3)?;
                let n = a.u32(2)?;
                match view.versions.get(&(a.hash64(0), n)) {
                    Some(v) => Out::default()
                        .u64(v.attempt_claims)
                        .u64(v.fp_claims)
                        .u128(v.work_leaves)
                        .u64(v.first_used_daa.unwrap_or(0))
                        .u64(v.last_used_daa.unwrap_or(0))
                        .finish(),
                    None => Out::default().zeros(5).finish(),
                }
            }
            x if x == s.evaluation_count => {
                let a = Args::parse(input, 3)?;
                let n = a.u32(2)?;
                let count = view.evaluations.get(&(a.hash64(0), n)).map(|e| e.len()).unwrap_or(0);
                Out::default().u64(count as u64).finish()
            }
            x if x == s.evaluation => {
                let a = Args::parse(input, 4)?;
                let n = a.u32(2)?;
                let i = a.u32(3)? as usize;
                match view.evaluations.get(&(a.hash64(0), n)).and_then(|e| e.get(i)) {
                    Some(e) => Out::default()
                        .hash(Some(&e.evaluator_id))
                        .u64(e.score_permille as u64)
                        .hash(Some(&e.report_hash))
                        .hash(e.by_payload.as_ref())
                        .u64(e.posted_daa)
                        .bool(e.is_lines_own)
                        .finish(),
                    None => Out::default().zeros(9).finish(),
                }
            }
            x if x == s.proposal_count => {
                let a = Args::parse(input, 2)?;
                let n = view.proposals.get(&a.hash64(0)).map(|p| p.len()).unwrap_or(0);
                Out::default().u64(n as u64).finish()
            }
            x if x == s.proposal => {
                let a = Args::parse(input, 3)?;
                let i = a.u32(2)? as usize;
                match view.proposals.get(&a.hash64(0)).and_then(|p| p.get(i)) {
                    Some(p) => Out::default()
                        .hash(Some(&p.proposal_id))
                        .hash(Some(&p.root))
                        .hash(Some(&p.note_hash))
                        .hash(p.by_payload.as_ref())
                        .u64(p.posted_daa)
                        .u64(p.adopted_in.unwrap_or(0) as u64)
                        .finish(),
                    None => Out::default().zeros(10).finish(),
                }
            }
            x if x == s.facade_of => {
                let a = Args::parse(input, 2)?;
                let line = a.hash64(0);
                let addr = if view.line(&line).is_some() { to_address(&facade_address_v1(&line)) } else { Address::ZERO };
                Out::default().address(&addr).finish()
            }
            x if x == s.line_of => {
                let a = Args::parse(input, 1)?;
                let addr = a.address(0)?;
                Out::default().hash(self.facades.get(&addr)).finish()
            }
            x if x == s.chain_daa => {
                Args::parse(input, 0)?;
                Out::default().u64(view.chain_daa).finish()
            }
            _ => return Err(Malformed),
        })
    }

    // ---- ModelAMM (0x…F011) ---------------------------------------------------------------

    fn amm(&self, input: &[u8]) -> Result<Vec<u8>, Malformed> {
        let s = sel();
        let selector: [u8; 4] = input.get(..4).ok_or(Malformed)?.try_into().map_err(|_| Malformed)?;
        Ok(match selector {
            x if x == s.market => {
                let a = Args::parse(input, 2)?;
                match self.market_row(&a.hash64(0)) {
                    Some((m, exists)) => Out::default()
                        .u64(m.opened_daa)
                        .u64(m.msk_reserve)
                        .u64(m.position_units)
                        .u64(m.sold_units)
                        .u64(m.burned_sompi)
                        .u64(m.registrant_paid_sompi)
                        .u64(m.contributor_paid_sompi)
                        .bool(m.closed_to_buys)
                        .bool(exists)
                        .finish(),
                    None => Out::default().zeros(9).finish(),
                }
            }
            x if x == s.price_of => {
                let a = Args::parse(input, 2)?;
                let price = self.market_row(&a.hash64(0)).map(|(m, _)| m.price_sompi_per_position_v1()).unwrap_or(0);
                Out::default().u64(price).finish()
            }
            x if x == s.quote_buy_of => {
                let a = Args::parse(input, 3)?;
                let msk_in = a.u64(2)?;
                match self.market_row(&a.hash64(0)).and_then(|(m, _)| palw_model_buy_quote_v1(&m, msk_in)) {
                    Some(q) => Out::default()
                        .u64(q.units_out)
                        .u64(q.fees.burn)
                        .u64(q.fees.registrant)
                        .u64(q.fees.net)
                        .u64(q.after.price_sompi_per_position_v1())
                        .finish(),
                    None => Out::default().zeros(5).finish(),
                }
            }
            x if x == s.quote_sell_of => {
                let a = Args::parse(input, 3)?;
                let units_in = a.u64(2)?;
                match self.market_row(&a.hash64(0)).and_then(|(m, _)| palw_model_sell_quote_v1(&m, units_in)) {
                    Some(q) => Out::default()
                        .u64(q.fees.net)
                        .u64(q.fees.burn)
                        .u64(q.fees.registrant)
                        .u64(q.fees.net)
                        .u64(q.after.price_sompi_per_position_v1())
                        .finish(),
                    None => Out::default().zeros(5).finish(),
                }
            }
            x if x == s.constants => {
                Args::parse(input, 0)?;
                Out::default()
                    .u64(PALW_MODEL_SUPPLY_UNITS_V1)
                    .u64(PALW_MODEL_POSITION_UNITS_V1)
                    .u64(PALW_MODEL_SEED_MIN_SOMPI_V1)
                    .u64(PALW_MODEL_BURN_PERMILLE_V1)
                    .u64(PALW_MODEL_REGISTRANT_PERMILLE_V1)
                    .finish()
            }
            _ => return Err(Malformed),
        })
    }

    // ---- ModelPosition (0x…F012) ----------------------------------------------------------

    fn position(&self, input: &[u8]) -> Result<Vec<u8>, Malformed> {
        let s = sel();
        let selector: [u8; 4] = input.get(..4).ok_or(Malformed)?.try_into().map_err(|_| Malformed)?;
        let view = &*self.view;
        Ok(match selector {
            x if x == s.balance_of_holder => {
                let a = Args::parse(input, 4)?;
                Out::default().u64(view.position(&a.hash64(0), &a.hash64(2))).finish()
            }
            x if x == s.balance_of_address => {
                let a = Args::parse(input, 3)?;
                let holder = self.holder_of(&a.address(2)?);
                Out::default().u64(view.position(&a.hash64(0), &holder)).finish()
            }
            x if x == s.total_supply_of => {
                let a = Args::parse(input, 2)?;
                let supply = if view.line(&a.hash64(0)).is_some() { PALW_MODEL_SUPPLY_UNITS_V1 } else { 0 };
                Out::default().u64(supply).finish()
            }
            x if x == s.sold => {
                let a = Args::parse(input, 2)?;
                let sold = view.markets.get(&a.hash64(0)).map(|m| m.sold_units).unwrap_or(0);
                Out::default().u64(sold).finish()
            }
            x if x == s.holder_id_of => {
                let a = Args::parse(input, 1)?;
                let holder = self.holder_of(&a.address(0)?);
                Out::default().hash(Some(&holder)).finish()
            }
            _ => return Err(Malformed),
        })
    }

    // ---- IMRC20 (a line's facade) ---------------------------------------------------------

    fn facade(&self, line: &Hash64, input: &[u8]) -> Result<FacadeCall, Malformed> {
        let s = sel();
        let selector: [u8; 4] = input.get(..4).ok_or(Malformed)?.try_into().map_err(|_| Malformed)?;
        let short = short_hex(line);
        let row = self.market_row(line).map(|(m, _)| m);
        Ok(FacadeCall::View(match selector {
            x if x == s.name => {
                Args::parse(input, 0)?;
                Out::default().string(&format!("MISAKA Model Position {short}")).finish()
            }
            x if x == s.symbol => {
                Args::parse(input, 0)?;
                Out::default().string(&format!("MP-{short}")).finish()
            }
            x if x == s.decimals => {
                Args::parse(input, 0)?;
                // ADR-0090 Decision 1: a position is whole.
                Out::default().u64(0).finish()
            }
            x if x == s.total_supply => {
                Args::parse(input, 0)?;
                Out::default().u64(PALW_MODEL_SUPPLY_UNITS_V1).finish()
            }
            x if x == s.balance_of => {
                let a = Args::parse(input, 1)?;
                let holder = self.holder_of(&a.address(0)?);
                Out::default().u64(self.view.position(line, &holder)).finish()
            }
            x if x == s.line_id => {
                Args::parse(input, 0)?;
                Out::default().hash(Some(line)).finish()
            }
            x if x == s.circulating => {
                Args::parse(input, 0)?;
                Out::default().u64(row.map(|m| m.sold_units).unwrap_or(0)).finish()
            }
            x if x == s.price => {
                Args::parse(input, 0)?;
                Out::default().u64(row.map(|m| m.price_sompi_per_position_v1()).unwrap_or(0)).finish()
            }
            x if x == s.quote_buy => {
                let a = Args::parse(input, 1)?;
                match a.u256_as_u64(0).and_then(|msk_in| palw_model_buy_quote_v1(&row?, msk_in)) {
                    Some(q) => Out::default().u64(q.units_out).u64(q.after.price_sompi_per_position_v1()).finish(),
                    None => Out::default().zeros(2).finish(),
                }
            }
            x if x == s.quote_sell => {
                let a = Args::parse(input, 1)?;
                match a.u256_as_u64(0).and_then(|units| palw_model_sell_quote_v1(&row?, units)) {
                    Some(q) => Out::default().u64(q.fees.net).u64(q.after.price_sompi_per_position_v1()).finish(),
                    None => Out::default().zeros(2).finish(),
                }
            }
            x if x == s.supports_interface => {
                let a = Args::parse(input, 1)?;
                let id = a.bytes4(0)?;
                Out::default().bool(id == s.imrc20_interface_id || id == ERC165_INTERFACE_ID).finish()
            }
            x if x == s.transfer || x == s.transfer_from || x == s.approve || x == s.allowance => {
                return Ok(FacadeCall::NonTransferable);
            }
            x if x == s.buy => {
                let a = Args::parse(input, 1)?;
                return Ok(FacadeCall::Write(Action::Buy { min_units_out: a.u256(0) }));
            }
            x if x == s.sell => {
                let a = Args::parse(input, 2)?;
                return Ok(FacadeCall::Write(Action::Sell { units_in: a.u256(0), min_msk_out_sompi: a.u256(1) }));
            }
            x if x == s.seed => {
                Args::parse(input, 0)?;
                return Ok(FacadeCall::Write(Action::Seed));
            }
            _ => return Err(Malformed),
        }))
    }
}

/// The first eight hex characters of a line id — the facade's `name()`/`symbol()` suffix.
fn short_hex(line: &Hash64) -> String {
    line.as_byte_slice()[..4].iter().map(|b| format!("{b:02x}")).collect()
}

// ---- the writer's wire format ------------------------------------------------------------------

/// `sendAction(bytes)`: a strictly-encoded single `bytes` argument.
fn decode_send_action(input: &[u8]) -> Result<(Hash64, Action), Malformed> {
    if input.len() < 4 + 64 || input[..4] != sel().send_action {
        return Err(Malformed);
    }
    let offset = U256::from_be_slice(&input[4..36]);
    if offset != U256::from(32u64) {
        return Err(Malformed);
    }
    let len = U256::from_be_slice(&input[36..68]);
    let len = usize::try_from(len).map_err(|_| Malformed)?;
    let padded = len.checked_add(31).ok_or(Malformed)? / 32 * 32;
    if input.len() != 68 + padded {
        return Err(Malformed);
    }
    if input[68 + len..].iter().any(|b| *b != 0) {
        return Err(Malformed);
    }
    decode_action_bytes(&input[68..68 + len])
}

/// The action bytes (Decision 5): `[version 1][action id u24 BE][ABI]`.
fn decode_action_bytes(data: &[u8]) -> Result<(Hash64, Action), Malformed> {
    if data.len() < 4 || data[0] != PALW_EVM_MARKET_ACTION_ENCODING_V1 {
        return Err(Malformed);
    }
    let id = u32::from_be_bytes([0, data[1], data[2], data[3]]);
    let abi = &data[4..];
    match id {
        PALW_EVM_ACTION_BUY => {
            if abi.len() != 96 {
                return Err(Malformed);
            }
            let line = hash_from_words(&abi[..32], &abi[32..64]);
            Ok((line, Action::Buy { min_units_out: U256::from_be_slice(&abi[64..96]) }))
        }
        PALW_EVM_ACTION_SELL => {
            if abi.len() != 128 {
                return Err(Malformed);
            }
            let line = hash_from_words(&abi[..32], &abi[32..64]);
            Ok((
                line,
                Action::Sell { units_in: U256::from_be_slice(&abi[64..96]), min_msk_out_sompi: U256::from_be_slice(&abi[96..128]) },
            ))
        }
        PALW_EVM_ACTION_SEED => {
            if abi.len() != 64 {
                return Err(Malformed);
            }
            Ok((hash_from_words(&abi[..32], &abi[32..64]), Action::Seed))
        }
        _ => Err(Malformed),
    }
}

/// The normalised action record the writer logs (see the module header).
fn action_record(id: u32, line: &Hash64, w3: u64, w4: u64) -> Vec<u8> {
    let mut r = Vec::with_capacity(ACTION_RECORD_LEN);
    r.push(PALW_EVM_MARKET_ACTION_ENCODING_V1);
    r.extend_from_slice(&id.to_be_bytes()[1..]);
    r.extend_from_slice(line.as_byte_slice());
    r.extend_from_slice(&word_u64(w3));
    r.extend_from_slice(&word_u64(w4));
    debug_assert_eq!(r.len(), ACTION_RECORD_LEN);
    r
}

/// `ActionQueued` data: `ABI(uint8 actionId, bytes record)`.
fn action_log_data(id: u32, record: &[u8]) -> Bytes {
    let mut data = Vec::with_capacity(ACTION_LOG_DATA_LEN);
    data.extend_from_slice(&word_u64(id as u64));
    data.extend_from_slice(&word_u64(64));
    data.extend_from_slice(&word_u64(record.len() as u64));
    data.extend_from_slice(record);
    data.extend(std::iter::repeat_n(0u8, ACTION_RECORD_PADDED_LEN - record.len()));
    debug_assert_eq!(data.len(), ACTION_LOG_DATA_LEN);
    Bytes::from(data)
}

/// Decode one committed `ActionQueued` log into the block's action list entry `seq`
/// (`None` = not the writer's log, or malformed — impossible for logs our own intercept emitted).
/// Like `decode_withdraw_log`, it re-asserts the record's invariants: version 1, a known action
/// id agreeing with the event's `actionId`, a positive gross for a buy, positive units for a sell,
/// every field within `u64`. A log from any address but `0x…F013` is not the writer's.
pub fn decode_action_log(log: &Log, seq: u32) -> Option<PalwEvmMarketActionV1> {
    if log.address != writer_address() {
        return None;
    }
    let topics = log.data.topics();
    if topics.len() != 2 || topics[0] != action_queued_topic() || topics[1][..12].iter().any(|b| *b != 0) {
        return None;
    }
    let account = EvmAddress::from_bytes(topics[1][12..].try_into().ok()?);
    let data = log.data.data.as_ref();
    if data.len() != ACTION_LOG_DATA_LEN {
        return None;
    }
    let word = |i: usize| -> Option<u64> {
        let w = &data[32 * i..32 * (i + 1)];
        if w[..24].iter().any(|b| *b != 0) {
            return None;
        }
        Some(u64::from_be_bytes(w[24..].try_into().ok()?))
    };
    let id = word(0)?;
    if word(1)? != 64 || word(2)? != ACTION_RECORD_LEN as u64 {
        return None;
    }
    let record = &data[96..96 + ACTION_RECORD_LEN];
    if data[96 + ACTION_RECORD_LEN..].iter().any(|b| *b != 0) {
        return None;
    }
    if record[0] != PALW_EVM_MARKET_ACTION_ENCODING_V1 {
        return None;
    }
    let record_id = u32::from_be_bytes([0, record[1], record[2], record[3]]) as u64;
    if record_id != id {
        return None;
    }
    let line_id = hash_from_words(&record[4..36], &record[36..68]);
    let field = |at: usize| -> Option<u64> {
        let w = &record[at..at + 32];
        if w[..24].iter().any(|b| *b != 0) {
            return None;
        }
        Some(u64::from_be_bytes(w[24..].try_into().ok()?))
    };
    let w3 = field(68)?;
    let w4 = field(100)?;
    let (kind, gross_sompi) = match id as u32 {
        PALW_EVM_ACTION_BUY => {
            if w4 == 0 {
                return None;
            }
            (PalwEvmMarketActionKindV1::Buy { min_units_out: w3 }, w4)
        }
        PALW_EVM_ACTION_SELL => {
            if w3 == 0 {
                return None;
            }
            (PalwEvmMarketActionKindV1::Sell { units_in: w3, min_msk_out_sompi: w4 }, 0)
        }
        PALW_EVM_ACTION_SEED => {
            if w3 != 0 || w4 == 0 {
                return None;
            }
            (PalwEvmMarketActionKindV1::Seed, w4)
        }
        _ => return None,
    };
    Some(PalwEvmMarketActionV1 { seq, account, line_id, kind, gross_sompi })
}

// ---- the settlement receipt (Decision 6) ------------------------------------------------------

/// The log a settlement emits from its line's facade, in the settling block's system receipt:
/// `Bought(holder, mskIn, unitsOut, priceAfter)`, `Sold(holder, unitsIn, mskOut, priceAfter)` or
/// `Refused(holder, actionId, amount, reason)` — `amount` is the escrow a refused buy returns
/// (zero for a sell), `reason` the fold's code as `bytes32(uint256(code))`.
pub fn settlement_log(s: &PalwEvmSettlementV1) -> EvmLog {
    let holder = EvmH256::from_bytes(word_address(&to_address(&s.account)));
    let (topic, data) = match s.outcome {
        // ADR-0090: `Seeded(holder, mskIn, priceAfter)` — the whole escrow became the reserve.
        PalwEvmSettlementOutcomeV1::Filled { price_after_sompi, .. } if s.is_seed() => {
            (seeded_topic(), Out::default().u64(s.escrow_sompi).u64(price_after_sompi).finish())
        }
        PalwEvmSettlementOutcomeV1::Filled { units, price_after_sompi, .. } if s.is_buy() => {
            (bought_topic(), Out::default().u64(s.escrow_sompi).u64(units).u64(price_after_sompi).finish())
        }
        PalwEvmSettlementOutcomeV1::Filled { units, net_sompi, price_after_sompi, .. } => {
            (sold_topic(), Out::default().u64(units).u64(net_sompi).u64(price_after_sompi).finish())
        }
        PalwEvmSettlementOutcomeV1::Refused { reason } => {
            (refused_topic(), Out::default().u64(s.action as u64).u64(s.escrow_sompi).u64(reason as u64).finish())
        }
    };
    EvmLog { address: facade_address_v1(&s.line_id), topics: vec![EvmH256::from_bytes(topic.0), holder], data }
}

// ---- the intercept -----------------------------------------------------------------------------

fn outcome(result: InstructionResult, output: Vec<u8>, gas: Gas, memory: std::ops::Range<usize>) -> FrameOrResult {
    FrameOrResult::Result(FrameResult::Call(CallOutcome::new(InterpreterResult { result, output: Bytes::from(output), gas }, memory)))
}

fn revert(selector: [u8; 4], gas: Gas, memory: std::ops::Range<usize>) -> FrameOrResult {
    outcome(InstructionResult::Revert, selector.to_vec(), gas, memory)
}

fn oog(gas: Gas, memory: std::ops::Range<usize>) -> FrameOrResult {
    outcome(InstructionResult::PrecompileOOG, Vec::new(), gas, memory)
}

/// A read's frame: the gas schedule, the value guard, the malformed-input rule.
fn read_frame(result: Result<Vec<u8>, Malformed>, inputs: &CallInputs, mut gas: Gas) -> FrameOrResult {
    let memory = inputs.return_memory_offset.clone();
    if !gas.record_cost(PALW_EVM_READ_BASE_GAS_V1 + PALW_EVM_READ_GAS_PER_BYTE_V1 * inputs.input.len() as u64) {
        return oog(gas, memory);
    }
    // A read is non-payable: a value-bearing call reverts so the value is never stranded in the
    // precompile (F003's rule). STATICCALL and zero-value CALL are fine.
    if inputs.value.transfer().is_some_and(|v| !v.is_zero()) {
        return revert(errors::bad_value(), gas, memory);
    }
    match result {
        Ok(output) => {
            if !gas.record_cost(PALW_EVM_READ_GAS_PER_BYTE_V1 * output.len() as u64) {
                return oog(gas, memory);
            }
            outcome(InstructionResult::Return, output, gas, memory)
        }
        Err(Malformed) => {
            // Hyperliquid's rule: invalid input "returns an error and consumes all gas passed
            // into the precompile call frame".
            gas.spend_all();
            revert(errors::bad_input(), gas, memory)
        }
    }
}

/// Register the market's doors on `handler` (only ever called when `fences.evm_active`).
pub fn register_market_handlers<EXT, DB: Database>(handler: &mut EvmHandler<'_, EXT, DB>, market: MarketHandlers) {
    let prev = handler.execution.call.clone();
    handler.execution.call = Arc::new(move |ctx, inputs| {
        let target = inputs.target_address;
        // delegate/callcode never match (they run the door's — empty — code at the caller).
        if inputs.bytecode_address != target {
            return prev(ctx, inputs);
        }
        let Some(door) = market.door(&target) else {
            return prev(ctx, inputs);
        };
        let gas = Gas::new(inputs.gas_limit);
        match door {
            Door::Registry => Ok(read_frame(market.registry(&inputs.input), &inputs, gas)),
            Door::Amm => Ok(read_frame(market.amm(&inputs.input), &inputs, gas)),
            Door::Position => Ok(read_frame(market.position(&inputs.input), &inputs, gas)),
            Door::Writer => {
                let memory = inputs.return_memory_offset.clone();
                let mut gas = gas;
                if !gas.record_cost(PALW_EVM_WRITER_GAS_V1) {
                    return Ok(oog(gas, memory));
                }
                let decoded = match decode_send_action(&inputs.input) {
                    Ok(d) => d,
                    Err(Malformed) => return Ok(revert(errors::bad_input(), gas, memory)),
                };
                write_frame(&market, ctx, &inputs, gas, decoded.0, decoded.1)
            }
            Door::Facade(line) => {
                match market.facade(&line, &inputs.input) {
                    Ok(FacadeCall::View(out)) => Ok(read_frame(Ok(out), &inputs, gas)),
                    Err(Malformed) => Ok(read_frame(Err(Malformed), &inputs, gas)),
                    Ok(FacadeCall::NonTransferable) => {
                        // ERC-20's transfer half does not exist: charged as a read, refused loudly.
                        let memory = inputs.return_memory_offset.clone();
                        let mut gas = gas;
                        if !gas.record_cost(PALW_EVM_READ_BASE_GAS_V1 + PALW_EVM_READ_GAS_PER_BYTE_V1 * inputs.input.len() as u64) {
                            return Ok(oog(gas, memory));
                        }
                        Ok(revert(errors::non_transferable(), gas, memory))
                    }
                    Ok(FacadeCall::Write(action)) => {
                        // `facade.buy` / `facade.sell` ARE the writer's two actions with the line
                        // filled in by the address: one write path, two doors (Decision 5).
                        let memory = inputs.return_memory_offset.clone();
                        let mut gas = gas;
                        if !gas.record_cost(PALW_EVM_WRITER_GAS_V1) {
                            return Ok(oog(gas, memory));
                        }
                        write_frame(&market, ctx, &inputs, gas, line, action)
                    }
                }
            }
        }
    });
}

/// The hand (Decision 5), after the gas charge and the decode: the checks in order, then the
/// journaled escrow transfer and the journaled log — which unwind together on a revert.
fn write_frame<EXT, DB: Database>(
    market: &MarketHandlers,
    ctx: &mut revm::Context<EXT, DB>,
    inputs: &CallInputs,
    gas: Gas,
    line: Hash64,
    action: Action,
) -> Result<FrameOrResult, revm::primitives::EVMError<DB::Error>> {
    let memory = inputs.return_memory_offset.clone();
    // A static frame cannot queue (an escrow move and a log are state changes).
    if inputs.is_static {
        return Ok(revert(errors::bad_input(), gas, memory));
    }
    let Some(value) = inputs.value.transfer() else {
        return Ok(revert(errors::bad_value(), gas, memory));
    };
    // Decision 4: a holder is the signing account and only the signing account —
    // `msg.sender == tx.origin` and no code at the sender.
    let inner = &mut ctx.evm.inner;
    if inputs.caller != inner.env.tx.caller {
        return Ok(revert(errors::not_an_account(), gas, memory));
    }
    let caller_has_code = !inner.journaled_state.load_account(inputs.caller, &mut inner.db)?.info.is_empty_code_hash();
    if caller_has_code {
        return Ok(revert(errors::not_an_account(), gas, memory));
    }
    if !market.fences.market_active {
        return Ok(revert(errors::market_not_active(), gas, memory));
    }
    let scale = U256::from(EVM_NATIVE_SCALE);
    let fits = |v: U256| -> Option<u64> { u64::try_from(v).ok() };
    let (id, w3, w4, gross_sompi) = match action {
        Action::Buy { min_units_out } => {
            if value.is_zero() || !(value % scale).is_zero() {
                return Ok(revert(errors::bad_value(), gas, memory));
            }
            let (Some(gross), Some(min)) = (fits(value / scale), fits(min_units_out)) else {
                return Ok(revert(errors::bad_value(), gas, memory));
            };
            (PALW_EVM_ACTION_BUY, min, gross, gross)
        }
        Action::Sell { units_in, min_msk_out_sompi } => {
            if !value.is_zero() {
                return Ok(revert(errors::bad_value(), gas, memory));
            }
            let (Some(units), Some(min)) = (fits(units_in), fits(min_msk_out_sompi)) else {
                return Ok(revert(errors::bad_value(), gas, memory));
            };
            if units == 0 {
                return Ok(revert(errors::bad_value(), gas, memory));
            }
            (PALW_EVM_ACTION_SELL, units, min, 0)
        }
        // ADR-0090: the seed is the value, whole sompi, at least the floor; refused here already
        // so a too-small seed never even queues.
        Action::Seed => {
            if value.is_zero() || !(value % scale).is_zero() {
                return Ok(revert(errors::bad_value(), gas, memory));
            }
            let Some(gross) = fits(value / scale) else {
                return Ok(revert(errors::bad_value(), gas, memory));
            };
            if gross < PALW_MODEL_SEED_MIN_SOMPI_V1 {
                return Ok(revert(errors::seed_too_small(), gas, memory));
            }
            (PALW_EVM_ACTION_SEED, 0, gross, gross)
        }
    };
    if market.view.line(&line).is_none() {
        return Ok(revert(errors::unknown_line(), gas, memory));
    }
    if id == PALW_EVM_ACTION_BUY && market.view.markets.get(&line).is_some_and(|m| m.closed_to_buys) {
        return Ok(revert(errors::closed_to_buys(), gas, memory));
    }
    if market.queued() >= MAX_MARKET_ACTIONS_PER_EVM_BLOCK {
        return Ok(revert(errors::too_many_actions(), gas, memory));
    }
    // Escrow (a buy): the value moves caller → writer in the CURRENT tx journal.
    let writer = writer_address();
    if gross_sompi > 0 {
        match inner.journaled_state.transfer(&inputs.caller, &writer, value, &mut inner.db) {
            Ok(None) => {}
            Ok(Some(_)) => return Ok(revert(errors::bad_value(), gas, memory)),
            Err(e) => return Err(e),
        }
    }
    let record = action_record(id, &line, w3, w4);
    inner.journaled_state.log(Log {
        address: writer,
        data: LogData::new_unchecked(
            vec![action_queued_topic(), B256::from(word_address(&inputs.caller))],
            action_log_data(id, &record),
        ),
    });
    market.queued.fetch_add(1, Ordering::SeqCst);
    Ok(outcome(InstructionResult::Return, Vec::new(), gas, memory))
}

// ---- calldata builders (for callers, tests and the CLI) ------------------------------------------

/// Calldata for `sendAction(bytes)` carrying a buy — the core crate's spelling, re-exported so a
/// caller that links this crate builds the same bytes a wallet linking only consensus-core does.
pub fn send_action_buy_calldata(line: &Hash64, min_units_out: u64) -> Vec<u8> {
    kaspa_consensus_core::evm::model_market::send_action_buy_calldata(line, min_units_out)
}

/// Calldata for `sendAction(bytes)` carrying a sell (the core crate's spelling).
pub fn send_action_sell_calldata(line: &Hash64, units_in: u64, min_msk_out_sompi: u64) -> Vec<u8> {
    kaspa_consensus_core::evm::model_market::send_action_sell_calldata(line, units_in, min_msk_out_sompi)
}

/// Calldata for `sendAction(bytes)` carrying a seed (ADR-0090; the core crate's spelling).
pub fn send_action_seed_calldata(line: &Hash64) -> Vec<u8> {
    kaspa_consensus_core::evm::model_market::send_action_seed_calldata(line)
}

/// `sendAction(bytes)` around raw action bytes (the core crate's spelling).
pub fn send_action_calldata(action: &[u8]) -> Vec<u8> {
    kaspa_consensus_core::evm::model_market::send_action_calldata(action)
}

/// Calldata for a static call: the selector of `signature` followed by `words`.
pub fn calldata(signature: &str, words: &[[u8; 32]]) -> Vec<u8> {
    let mut input = selector(signature).to_vec();
    for w in words {
        input.extend_from_slice(w);
    }
    input
}

/// ABI word helpers for callers building calldata.
pub mod abi {
    use super::Hash64;
    pub fn u64(v: u64) -> [u8; 32] {
        super::word_u64(v)
    }
    pub fn address(a: &[u8; 20]) -> [u8; 32] {
        super::word_address(&super::Address::from(*a))
    }
    pub fn bytes4(b: [u8; 4]) -> [u8; 32] {
        super::word_bytes4(b)
    }
    /// The two words of a 64-byte id, high half first.
    pub fn hash64(h: &Hash64) -> [[u8; 32]; 2] {
        let (a, b) = super::hash_words(h);
        [a, b]
    }
    /// Read word `i` of an ABI return as `u64` (`None` if it does not fit).
    pub fn read_u64(output: &[u8], i: usize) -> Option<u64> {
        let w = output.get(32 * i..32 * (i + 1))?;
        if w[..24].iter().any(|b| *b != 0) {
            return None;
        }
        Some(u64::from_be_bytes(w[24..].try_into().ok()?))
    }
    /// Read words `i` and `i + 1` of an ABI return as a 64-byte id.
    pub fn read_hash64(output: &[u8], i: usize) -> Option<Hash64> {
        let a = output.get(32 * i..32 * (i + 1))?;
        let b = output.get(32 * (i + 1)..32 * (i + 2))?;
        Some(super::hash_from_words(a, b))
    }
    /// Read a single `string` return.
    pub fn read_string(output: &[u8]) -> Option<String> {
        let len = read_u64(output, 1)? as usize;
        String::from_utf8(output.get(64..64 + len)?.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_crate_spells_the_writer_selector_the_intercept_answers() {
        assert_eq!(sel().send_action, kaspa_consensus_core::evm::model_market::send_action_selector());
    }

    #[test]
    fn selectors_are_keccak_prefixes_and_the_interface_id_excludes_erc20() {
        // A known Ethereum selector as the external anchor for the hashing.
        assert_eq!(sel().balance_of, [0x70, 0xa0, 0x82, 0x31], "balanceOf(address) is 0x70a08231 on every EVM");
        assert_eq!(sel().transfer, [0xa9, 0x05, 0x9c, 0xbb], "transfer(address,uint256) is 0xa9059cbb");
        assert_eq!(sel().total_supply, [0x18, 0x16, 0x0d, 0xdd]);
        assert_eq!(sel().supports_interface, [0x01, 0xff, 0xc9, 0xa7], "supportsInterface(bytes4) is the ERC-165 id itself");
        assert_ne!(imrc20_interface_id(), ERC20_INTERFACE_ID);
        assert_ne!(imrc20_interface_id(), ERC165_INTERFACE_ID);
    }

    #[test]
    fn the_action_record_round_trips_through_the_log() {
        let line = Hash64::from_u64_word(9);
        let account = Address::from([0xAB; 20]);
        let record = action_record(PALW_EVM_ACTION_BUY, &line, 7, 5);
        let log = Log {
            address: writer_address(),
            data: LogData::new_unchecked(
                vec![action_queued_topic(), B256::from(word_address(&account))],
                action_log_data(PALW_EVM_ACTION_BUY, &record),
            ),
        };
        let action = decode_action_log(&log, 3).expect("the writer's own log decodes");
        assert_eq!(action.seq, 3);
        assert_eq!(action.account, EvmAddress::from_bytes([0xAB; 20]));
        assert_eq!(action.line_id, line);
        assert_eq!(action.kind, PalwEvmMarketActionKindV1::Buy { min_units_out: 7 });
        assert_eq!(action.gross_sompi, 5);
        // A forged log from another address is not the writer's.
        let forged = Log { address: registry_address(), data: log.data.clone() };
        assert!(decode_action_log(&forged, 0).is_none());
        // A buy record with a zero gross is not an action.
        let zero = action_record(PALW_EVM_ACTION_BUY, &line, 7, 0);
        let bad = Log {
            address: writer_address(),
            data: LogData::new_unchecked(
                vec![action_queued_topic(), B256::from(word_address(&account))],
                action_log_data(PALW_EVM_ACTION_BUY, &zero),
            ),
        };
        assert!(decode_action_log(&bad, 0).is_none());
        // A sell.
        let sell = action_record(PALW_EVM_ACTION_SELL, &line, 11, 2);
        let log = Log {
            address: writer_address(),
            data: LogData::new_unchecked(
                vec![action_queued_topic(), B256::from(word_address(&account))],
                action_log_data(PALW_EVM_ACTION_SELL, &sell),
            ),
        };
        let action = decode_action_log(&log, 0).unwrap();
        assert_eq!(action.kind, PalwEvmMarketActionKindV1::Sell { units_in: 11, min_msk_out_sompi: 2 });
        assert_eq!(action.gross_sompi, 0);
    }

    #[test]
    fn send_action_calldata_decodes_and_strictness_holds() {
        let line = Hash64::from_u64_word(4);
        let (l, a) = decode_send_action(&send_action_buy_calldata(&line, 3)).ok().expect("a buy decodes");
        assert_eq!(l, line);
        assert!(matches!(a, Action::Buy { min_units_out } if min_units_out == U256::from(3u64)));
        let (l, a) = decode_send_action(&send_action_sell_calldata(&line, 8, 9)).ok().expect("a sell decodes");
        assert_eq!(l, line);
        assert!(
            matches!(a, Action::Sell { units_in, min_msk_out_sompi } if units_in == U256::from(8u64) && min_msk_out_sompi == U256::from(9u64))
        );
        // Reserved action ids, a wrong version and a trailing byte are all malformed.
        let mut reserved = vec![PALW_EVM_MARKET_ACTION_ENCODING_V1, 0, 0, 3];
        reserved.extend_from_slice(&[0u8; 96]);
        assert!(decode_send_action(&send_action_calldata(&reserved)).is_err());
        let mut v2 = vec![2u8, 0, 0, 1];
        v2.extend_from_slice(&[0u8; 96]);
        assert!(decode_send_action(&send_action_calldata(&v2)).is_err());
        let mut long = send_action_buy_calldata(&line, 3);
        long.push(0);
        assert!(decode_send_action(&long).is_err());
    }

    #[test]
    fn the_settlement_log_names_the_facade_and_the_outcome() {
        let line = Hash64::from_u64_word(2);
        let account = EvmAddress::from_bytes([0x11; 20]);
        let filled = PalwEvmSettlementV1 {
            seq: 0,
            account,
            line_id: line,
            action: PALW_EVM_ACTION_BUY,
            escrow_sompi: 100,
            outcome: PalwEvmSettlementOutcomeV1::Filled { units: 40, gross_sompi: 100, net_sompi: 94, price_after_sompi: 7 },
        };
        let log = settlement_log(&filled);
        assert_eq!(log.address, facade_address_v1(&line));
        assert_eq!(log.topics[0].as_bytes(), bought_topic().0);
        assert_eq!(abi::read_u64(&log.data, 0), Some(100));
        assert_eq!(abi::read_u64(&log.data, 1), Some(40));
        assert_eq!(abi::read_u64(&log.data, 2), Some(7));
        let refused = PalwEvmSettlementV1 {
            action: PALW_EVM_ACTION_SELL,
            escrow_sompi: 0,
            outcome: PalwEvmSettlementOutcomeV1::Refused { reason: 5 },
            ..filled
        };
        let log = settlement_log(&refused);
        assert_eq!(log.topics[0].as_bytes(), refused_topic().0);
        assert_eq!(abi::read_u64(&log.data, 0), Some(PALW_EVM_ACTION_SELL as u64));
        assert_eq!(abi::read_u64(&log.data, 1), Some(0));
        assert_eq!(abi::read_u64(&log.data, 2), Some(5));
    }
}
