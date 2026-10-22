//! **G14 r6 (user decision 2026-10-22): the rooted receipt-window pause row.** A certified, Gate C–governed free-prompt claim with an
//! accusation pending has its receipts suspended (`palw_fp_receipt_clock_v1` answers `accusation-pending`) AND its receipt use-window
//! clock paused; on dismissal the REMAINING window resumes ([`crate::palw_court_clock_v2::palw_fp_receipt_window_start_v2`]).
//!
//! The pause account is consensus state, so it is a rooted, journaled, carried row: kernel route aux table
//! [`PALW_RECEIPT_PAUSE_TABLE_V2`] (`145`, a fresh table — aux 144 is shared by the claim-local frames `F`/`R`/`C` and the native held
//! fee `0x01`, and its claim-local carriage check treats every non-`C`/`R` key as a liability frame, so a new prefix there would be
//! refused at load), key `b'P' ‖ claim` (65 bytes), row [`PalwReceiptPauseRowV2`]. It rides the kernel route's `KernelRouteRow`
//! journal (a reorg reverts it with its block), its aux root and its carriage tail `0xEC` — no new tail byte.
//!
//! Written by one writer (the fold's end-of-block reconcile, `palw_state_v2::palw_receipt_pause_fold_v2`) and only past Gate C's fence
//! for a claim Gate C governs, certified: opened at the DAA of the block in which an accusation is first seen pending, closed (the span
//! credited) at the DAA of the block in which none is pending any more; deleted when the claim is voided (a conviction — the claim is
//! `revoked`, no window to resume) or gone. Below the fence nothing is written or read, so the released receipt clock is unchanged.
use crate::palw_court_clock_v2::PalwReceiptPauseV2;
use crate::palw_state_v2::{PalwChainStateV2, PalwClaimStateV2, PalwStateParamsV2};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;

/// Kernel route aux table of the receipt pause rows (dormant; written only past Gate C's fence).
pub const PALW_RECEIPT_PAUSE_TABLE_V2: u8 = 145;
/// `b'P' ‖ claim` (65 bytes).
pub const PALW_RECEIPT_PAUSE_PREFIX_V2: u8 = b'P';
/// The row's version.
pub const PALW_RECEIPT_PAUSE_ROW_VERSION_V2: u8 = 1;

/// One claim's pause account, as stored. `open != 0` ⇔ a suspension is open since `open_since_daa`; `paused_total_daa` is the sum
/// of the suspensions already closed by dismissal. Fixed-width fields (no `Option`) so the native wire codec is a plain struct.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwReceiptPauseRowV2 {
    pub version: u8,
    pub claim: [u8; 64],
    pub open: u8,
    pub open_since_daa: u64,
    pub paused_total_daa: u64,
}

impl PalwReceiptPauseRowV2 {
    pub fn new(claim: &Hash64) -> Self {
        Self { version: PALW_RECEIPT_PAUSE_ROW_VERSION_V2, claim: claim.as_bytes(), ..Default::default() }
    }

    /// The in-memory account.
    pub fn account(&self) -> PalwReceiptPauseV2 {
        PalwReceiptPauseV2 { open_since_daa: (self.open != 0).then_some(self.open_since_daa), paused_total_daa: self.paused_total_daa }
    }

    /// Store `account` back into the row.
    pub fn with_account(mut self, account: &PalwReceiptPauseV2) -> Self {
        self.open = u8::from(account.open_since_daa.is_some());
        self.open_since_daa = account.open_since_daa.unwrap_or(0);
        self.paused_total_daa = account.paused_total_daa;
        self
    }

    /// Structural check (a carried row that fails it is refused by the reader, never zero-filled).
    pub fn well_formed(&self, claim: &Hash64) -> bool {
        self.version == PALW_RECEIPT_PAUSE_ROW_VERSION_V2 && self.claim == claim.as_bytes() && self.open <= 1
    }
}

/// The key of `claim`'s pause row (65 bytes).
pub fn palw_receipt_pause_key_v2(claim: &Hash64) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(PALW_RECEIPT_PAUSE_PREFIX_V2);
    key.extend_from_slice(&claim.as_bytes());
    key
}

/// Typed read failure: a present row that does not decode or does not name its key's claim. Never read as "no pause".
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("claim {0}'s receipt pause row is malformed")]
pub struct PalwReceiptPauseMalformedV2(pub Hash64);

/// **`claim`'s stored pause row** (`Ok(None)`: none — no accusation was ever seen pending on it past the fence, or no kernel route).
pub fn palw_receipt_pause_row_v2(
    chain: &PalwChainStateV2,
    claim: &Hash64,
) -> Result<Option<PalwReceiptPauseRowV2>, PalwReceiptPauseMalformedV2> {
    let Some(route) = chain.kernel_route() else { return Ok(None) };
    let Some(bytes) = route.aux.get(&(PALW_RECEIPT_PAUSE_TABLE_V2, palw_receipt_pause_key_v2(claim))) else { return Ok(None) };
    let row = borsh::from_slice::<PalwReceiptPauseRowV2>(bytes).map_err(|_| PalwReceiptPauseMalformedV2(*claim))?;
    if !row.well_formed(claim) {
        return Err(PalwReceiptPauseMalformedV2(*claim));
    }
    Ok(Some(row))
}

/// **`claim`'s pause account** (the default — nothing paused — when no row exists).
pub fn palw_receipt_pause_account_v2(
    chain: &PalwChainStateV2,
    claim: &Hash64,
) -> Result<PalwReceiptPauseV2, PalwReceiptPauseMalformedV2> {
    Ok(palw_receipt_pause_row_v2(chain, claim)?.map(|row| row.account()).unwrap_or_default())
}

/// Every stored pause row's claim, in key order (the reconcile's walk of the rows it owns).
pub fn palw_receipt_pause_claims_v2(chain: &PalwChainStateV2) -> Vec<Hash64> {
    let Some(route) = chain.kernel_route() else { return Vec::new() };
    route
        .aux
        .range((PALW_RECEIPT_PAUSE_TABLE_V2, Vec::new())..(PALW_RECEIPT_PAUSE_TABLE_V2 + 1, Vec::new()))
        .filter_map(|((_, key), _)| {
            (key.len() == 65 && key[0] == PALW_RECEIPT_PAUSE_PREFIX_V2)
                .then(|| Hash64::from_bytes(key[1..].try_into().expect("a 65-byte key holds a 64-byte claim")))
        })
        .collect()
}

/// **Does the pause apply to this claim?** Gate C governs it (accepted past the fence). Below the fence: never.
pub fn palw_receipt_pause_applies_v2(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    crate::palw_rfc0018_v1::palw_gate_c_governs_v1(params, claim)
}
