//! **ADR-0087 — a position is bought from the curve and sold back to it.**
//!
//! The arithmetic of a class's model market, kept apart from the fold that applies it: the
//! constants, the constant-product curve over the reserve plus a virtual reserve, the fee split,
//! the sink script a buy's carrier pays into, and the message a sell is signed over. Everything
//! here is a pure function of its arguments so the fold, the RPC, the CLI's quote and the tests
//! read one arithmetic.
//!
//! **Amended by ADR-0090 (2026-09-05).** A position is a whole number — one unit, no fraction —
//! and a line's market opens only when a SEED of at least `PALW_MODEL_SEED_MIN_SOMPI_V1` is paid
//! into its sink: the seed becomes the reserve, fee-free, and nothing ever pays it out. The
//! curve is `msk_reserve × position_units = K` with `K` taken at each move from the row as it
//! stands (the product never falls); there is no virtual reserve. Every line opens with
//! `PALW_MODEL_POSITION_SUPPLY_V1` positions in the curve. The price at any moment is
//! `msk_reserve / position_units` and there is no other price.

use crate::Hash64;
use crate::tx::ScriptPublicKey;

/// ADR-0090 Decision 1: a position IS the unit — one, no fraction. The name survives so every
/// reader that multiplies by it keeps reading; the value is one.
pub const PALW_MODEL_POSITION_UNITS_V1: u64 = 1;
/// ADR-0090 Decision 1: five hundred thousand whole positions a line, fixed at the seed — a
/// network constant so no model is issued more room than another.
pub const PALW_MODEL_POSITION_SUPPLY_V1: u64 = 500_000;
/// The whole supply in units.
pub const PALW_MODEL_SUPPLY_UNITS_V1: u64 = PALW_MODEL_POSITION_SUPPLY_V1 * PALW_MODEL_POSITION_UNITS_V1;
/// ADR-0090 Decision 2 retired the virtual reserve; the constant is kept at zero so a reader
/// that still adds it adds nothing. The first price is `seed / supply` now.
pub const PALW_MODEL_MARKET_VIRTUAL_SOMPI_V1: u64 = 0;
/// ADR-0090 Decision 2: the least seed that opens a line's market — 100,000 MSK, every sompi of
/// which enters the curve, none of which any object ever pays out.
pub const PALW_MODEL_SEED_MIN_SOMPI_V1: u64 = 100_000 * 100_000_000;
/// Fee on the MSK leg of every move, in permille: burned.
pub const PALW_MODEL_BURN_PERMILLE_V1: u64 = 50;
/// Fee on the MSK leg of every move, in permille: to the class's registrant (burned when the
/// class has none).
pub const PALW_MODEL_REGISTRANT_PERMILLE_V1: u64 = 10;
/// The sink script's tag: `OP_RETURN <tag> <class id>`.
pub const PALW_MODEL_SINK_TAG_V1: &[u8; 8] = b"MSKMDL01";
const OP_RETURN: u8 = 0x6a;
const OP_DATA8: u8 = 0x08;
const OP_DATA64: u8 = 0x40;
/// Domain of the message a sell is signed over.
pub const PALW_MODEL_SELL_SIGN_DOMAIN_V1: &[u8] = b"misaka-palw/model-market/sell/v1";
/// ML-DSA-87 context of a sell's signature, distinct from every other context on the chain.
pub const PALW_MODEL_SELL_MLDSA87_CONTEXT: &[u8] = b"misaka-palw-model-sell-v1";

/// One class's market row, as the fold holds it (Decision 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwModelMarketV1 {
    pub opened_daa: u64,
    /// MSK the curve holds, in sompi — funded by sinks, drained by payouts, never a spendable output.
    pub msk_reserve: u64,
    /// Units still in the curve.
    pub position_units: u64,
    /// Units ever bought, cumulative (a sell does not reduce it).
    pub sold_units: u64,
    pub burned_sompi: u64,
    pub registrant_paid_sompi: u64,
    /// Set when the class left `Active`; sells continue, buys are refused (Decision 7).
    pub closed_to_buys: bool,
    /// ADR-0088 Decision 8: the part of the registrant leg paid to an adopted contributor.
    /// `registrant_paid_sompi` is the owner's total.
    pub contributor_paid_sompi: u64,
    /// ADR-0090 Decision 2: the MSK the market opened with — the floor the reserve never falls
    /// under (with every position back in the curve the product puts the reserve at the seed or
    /// above), and the number the site shows as "locked".
    pub seed_sompi: u64,
    /// ADR-0090 Decision 3: who paid the seed — a payout payload kept for the record only; the
    /// seeder holds nothing and can move nothing.
    pub seeded_by: Hash64,
}

impl PalwModelMarketV1 {
    /// ADR-0090 Decision 2: a market opens ONLY by a seed — the whole supply in the curve and the
    /// seed as the reserve, fee-free. There is no other opening.
    pub fn seed_v1(opened_daa: u64, seed_sompi: u64, seeded_by: Hash64) -> Self {
        Self {
            opened_daa,
            msk_reserve: seed_sompi,
            position_units: PALW_MODEL_SUPPLY_UNITS_V1,
            sold_units: 0,
            burned_sompi: 0,
            registrant_paid_sompi: 0,
            closed_to_buys: false,
            contributor_paid_sompi: 0,
            seed_sompi,
            seeded_by,
        }
    }

    /// The product the next move must not fall under: the row's own `reserve × units`.
    pub fn k(&self) -> u128 {
        self.msk_reserve as u128 * self.position_units as u128
    }

    pub fn price_sompi_per_position_v1(&self) -> u64 {
        if self.position_units == 0 {
            return u64::MAX;
        }
        let numerator = self.msk_reserve as u128 * PALW_MODEL_POSITION_UNITS_V1 as u128;
        (numerator / self.position_units as u128).min(u64::MAX as u128) as u64
    }
}

/// The three-way split of a gross MSK leg (Decision 4): `burn`, `registrant`, `net`, exact —
/// `burn + registrant + net == gross` — with the rounding remainder on the net leg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwModelFeeSplitV1 {
    pub gross: u64,
    pub burn: u64,
    pub registrant: u64,
    pub net: u64,
}

pub fn palw_model_fee_split_v1(gross: u64) -> PalwModelFeeSplitV1 {
    let burn = ((gross as u128 * PALW_MODEL_BURN_PERMILLE_V1 as u128) / 1000) as u64;
    let registrant = ((gross as u128 * PALW_MODEL_REGISTRANT_PERMILLE_V1 as u128) / 1000) as u64;
    PalwModelFeeSplitV1 { gross, burn, registrant, net: gross - burn - registrant }
}

/// What a buy of `msk_in` sompi does to a market: the split of the gross leg and the units the
/// curve releases for the NET leg, `units_out = units − ⌈K / (reserve + V + net)⌉` (rounded so the
/// curve's product never falls below `K`). `None` when the market releases nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwModelBuyQuoteV1 {
    pub fees: PalwModelFeeSplitV1,
    pub units_out: u64,
    pub after: PalwModelMarketV1,
}

pub fn palw_model_buy_quote_v1(market: &PalwModelMarketV1, msk_in: u64) -> Option<PalwModelBuyQuoteV1> {
    // ADR-0090: a row with no reserve is not a market (the fold never writes one; a reader may
    // synthesise one for a line that has no seed) — it quotes nothing rather than the whole curve.
    if market.closed_to_buys || msk_in == 0 || market.position_units == 0 || market.msk_reserve == 0 {
        return None;
    }
    let fees = palw_model_fee_split_v1(msk_in);
    let k = market.k();
    let x_after = market.msk_reserve as u128 + fees.net as u128;
    let units_after = k.div_ceil(x_after);
    let units_out = (market.position_units as u128).checked_sub(units_after)?;
    if units_out == 0 {
        return None;
    }
    let units_out = units_out as u64;
    let after = PalwModelMarketV1 {
        msk_reserve: market.msk_reserve.checked_add(fees.net)?,
        position_units: market.position_units - units_out,
        sold_units: market.sold_units.checked_add(units_out)?,
        burned_sompi: market.burned_sompi.checked_add(fees.burn)?,
        ..*market
    };
    Some(PalwModelBuyQuoteV1 { fees, units_out, after })
}

/// What a sell of `units_in` does: the gross MSK leg the curve pays, `gross = (reserve + V) −
/// ⌈K / (units + units_in)⌉` (rounded so the curve's product never falls below `K`), capped by
/// the reserve — the virtual reserve is never paid out — and its split. `None` when the curve
/// pays nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwModelSellQuoteV1 {
    pub fees: PalwModelFeeSplitV1,
    pub after: PalwModelMarketV1,
}

pub fn palw_model_sell_quote_v1(market: &PalwModelMarketV1, units_in: u64) -> Option<PalwModelSellQuoteV1> {
    if units_in == 0 {
        return None;
    }
    let k = market.k();
    let units_after = market.position_units as u128 + units_in as u128;
    if units_after > PALW_MODEL_SUPPLY_UNITS_V1 as u128 {
        return None;
    }
    let x_now = market.msk_reserve as u128;
    let x_after = k.div_ceil(units_after);
    let gross = x_now.checked_sub(x_after)?.min(market.msk_reserve as u128);
    if gross == 0 {
        return None;
    }
    let fees = palw_model_fee_split_v1(gross as u64);
    let after = PalwModelMarketV1 {
        msk_reserve: market.msk_reserve - fees.gross,
        position_units: units_after as u64,
        burned_sompi: market.burned_sompi.checked_add(fees.burn)?,
        ..*market
    };
    Some(PalwModelSellQuoteV1 { fees, after })
}

/// The sink a buy's carrier pays into: `OP_RETURN <8-byte tag> <64-byte class id>`. Unspendable
/// by construction (the script returns early); the fold credits its value to the class's reserve.
pub fn palw_model_sink_spk_v1(class_id: &Hash64) -> ScriptPublicKey {
    let mut script = Vec::with_capacity(1 + 1 + 8 + 1 + 64);
    script.push(OP_RETURN);
    script.push(OP_DATA8);
    script.extend_from_slice(PALW_MODEL_SINK_TAG_V1);
    script.push(OP_DATA64);
    script.extend_from_slice(class_id.as_byte_slice());
    ScriptPublicKey::new(0, crate::tx::ScriptVec::from_slice(&script))
}

/// The class a sink script names, if it is one.
pub fn palw_model_sink_class_v1(spk: &ScriptPublicKey) -> Option<Hash64> {
    if spk.version() != 0 {
        return None;
    }
    let script = spk.script();
    if script.len() != 75
        || script[0] != OP_RETURN
        || script[1] != OP_DATA8
        || &script[2..10] != PALW_MODEL_SINK_TAG_V1
        || script[10] != OP_DATA64
    {
        return None;
    }
    let mut id = [0u8; 64];
    id.copy_from_slice(&script[11..75]);
    Some(Hash64::from_bytes(id))
}

/// **The holder is its payout payload** (M8): the 64-byte BLAKE2b of the ML-DSA-87 public key,
/// the same identity a bond pays and `p2pkh_mldsa87_spk` locks to.
pub fn palw_model_holder_of_pubkey_v1(pubkey: &[u8]) -> Hash64 {
    crate::dns_finality::validator_id_from_pubkey(pubkey)
}

/// The message a sell is signed over: the domain, the class, the holder, the units, the floor.
pub fn palw_model_sell_message_v1(class_id: &Hash64, holder: &Hash64, units_in: u64, min_msk_out: u64) -> Vec<u8> {
    let mut m = Vec::with_capacity(32 + 64 + 64 + 16);
    m.extend_from_slice(PALW_MODEL_SELL_SIGN_DOMAIN_V1);
    m.extend_from_slice(class_id.as_byte_slice());
    m.extend_from_slice(holder.as_byte_slice());
    m.extend_from_slice(&units_in.to_le_bytes());
    m.extend_from_slice(&min_msk_out.to_le_bytes());
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    const MSK: u64 = 100_000_000;

    fn seeded() -> PalwModelMarketV1 {
        PalwModelMarketV1::seed_v1(7, PALW_MODEL_SEED_MIN_SOMPI_V1, Hash64::from_u64_word(9))
    }

    /// ADR-0090 §4's worked table, from a market seeded with the least seed (100,000 MSK): two
    /// buys of 1,000 MSK, then the sell of everything bought. Whole positions, no virtual reserve,
    /// the product taken from the row at every move.
    #[test]
    fn the_adr_table_is_the_curves_arithmetic() {
        let m0 = seeded();
        assert_eq!(m0.msk_reserve, 100_000 * MSK, "the seed is the reserve, fee-free");
        assert_eq!(m0.position_units, 500_000, "five hundred thousand whole positions in the curve");
        assert_eq!(m0.price_sompi_per_position_v1(), MSK / 5, "the first position costs seed / supply = 0.2 MSK");
        let b1 = palw_model_buy_quote_v1(&m0, 1_000 * MSK).expect("a buy");
        assert_eq!(b1.fees.burn, 50 * MSK);
        assert_eq!(b1.fees.registrant, 10 * MSK);
        assert_eq!(b1.fees.net, 940 * MSK);
        assert_eq!(b1.units_out, 4_656, "940 MSK into a 100,000 MSK reserve releases 4,656 whole positions");
        assert_eq!(b1.after.msk_reserve, 100_940 * MSK);
        assert_eq!(b1.after.price_sompi_per_position_v1(), 20_377_757, "0.20377757 MSK a position");
        let b2 = palw_model_buy_quote_v1(&b1.after, 1_000 * MSK).expect("a second buy");
        assert_eq!(b2.units_out, 4_570, "the same MSK releases fewer positions at the higher price");
        assert_eq!(b2.after.msk_reserve, 101_880 * MSK);
        assert!(b2.after.price_sompi_per_position_v1() > b1.after.price_sompi_per_position_v1(), "buying raises the price");
        let held = b1.units_out + b2.units_out;
        let s = palw_model_sell_quote_v1(&b2.after, held).expect("the sell");
        assert_eq!(s.fees.gross, 187_988_976_000, "returning every position bought pays back 1,879.88976 MSK gross");
        assert_eq!(s.fees.net, 176_709_637_440, "1,767.0963744 MSK net: 12 % of the legs, the curve's rounding kept");
        assert_eq!(s.fees.burn + s.fees.registrant + s.fees.net, s.fees.gross, "the split is exact");
        assert_eq!(s.after.position_units, PALW_MODEL_SUPPLY_UNITS_V1, "everything bought is back in the curve");
        assert!(s.after.msk_reserve >= m0.seed_sompi, "and the reserve is at or above the seed: {}", s.after.msk_reserve);
        assert!(s.after.price_sompi_per_position_v1() < b2.after.price_sompi_per_position_v1(), "selling lowers the price");
        // M2 at the arithmetic layer: what went in is where the ADR says it is.
        let paid_in = 2_000 * MSK;
        let burned = s.after.burned_sompi;
        let registrant = b1.fees.registrant + b2.fees.registrant + s.fees.registrant;
        assert_eq!(
            paid_in + m0.seed_sompi,
            s.after.msk_reserve + s.fees.net + burned + registrant,
            "nothing is minted, nothing vanishes"
        );
    }

    /// ADR-0090 P1: the reserve never falls under the seed — the product never falls, and with
    /// every position in the curve the product is `seed × supply`. Tried at every size, both moves.
    #[test]
    fn the_product_never_falls_and_the_seed_never_leaves() {
        let mut m = seeded();
        let mut k = m.k();
        for msk_in in [1u64, 999, MSK, 37 * MSK, 1_000 * MSK, 123_456_789_012] {
            let Some(q) = palw_model_buy_quote_v1(&m, msk_in) else { continue };
            assert!(q.after.k() >= k, "buy {msk_in}: product {} < {k}", q.after.k());
            k = q.after.k();
            m = q.after;
        }
        let bought = PALW_MODEL_SUPPLY_UNITS_V1 - m.position_units;
        for units in [1u64, 12_345, bought / 3, bought / 2] {
            let Some(q) = palw_model_sell_quote_v1(&m, units) else { continue };
            assert!(q.after.k() >= k, "sell {units}: product {} < {k}", q.after.k());
            assert!(q.after.msk_reserve >= m.seed_sompi, "sell {units}: the reserve {} fell under the seed", q.after.msk_reserve);
            k = q.after.k();
            m = q.after;
        }
        // and the rest, so every position is back in the curve
        let rest = PALW_MODEL_SUPPLY_UNITS_V1 - m.position_units;
        let q = palw_model_sell_quote_v1(&m, rest).expect("the rest sells");
        assert!(q.after.k() >= k);
        m = q.after;
        assert_eq!(m.position_units, PALW_MODEL_SUPPLY_UNITS_V1);
        assert!(m.msk_reserve >= m.seed_sompi, "with every position back, the reserve is the seed or more: {}", m.msk_reserve);
    }

    /// ADR-0090 Decision 1: a position is whole. A buy that would release less than one position
    /// releases nothing (refused at the fold), and the smallest buy that releases one releases
    /// exactly one.
    #[test]
    fn a_position_is_whole_and_a_dust_buy_releases_nothing() {
        let m0 = seeded();
        assert!(palw_model_buy_quote_v1(&m0, MSK / 10).is_none(), "0.1 MSK buys no whole position at 0.2 MSK each");
        let one = palw_model_buy_quote_v1(&m0, 22 * MSK / 100).expect("0.22 MSK buys one");
        assert_eq!(one.units_out, 1);
        assert_eq!(PALW_MODEL_POSITION_UNITS_V1, 1, "a position is the unit");
    }

    /// A buy and an immediate sell of what it bought returns 0.94² of the gross less slippage,
    /// and never more (M4).
    #[test]
    fn a_round_trip_pays_the_fees_twice_and_the_slippage_once() {
        let m0 = seeded();
        let b = palw_model_buy_quote_v1(&m0, 100 * MSK).expect("a buy");
        let s = palw_model_sell_quote_v1(&b.after, b.units_out).expect("the sell");
        assert!(s.fees.net <= 94 * 94 * MSK / 100, "at most 0.94² of the gross came back: {}", s.fees.net);
        assert!(s.fees.net > 80 * MSK, "and most of it did: {}", s.fees.net);
        assert_eq!(s.after.position_units, PALW_MODEL_SUPPLY_UNITS_V1);
    }

    /// A closed market refuses buys and honours sells (M6, at this layer).
    #[test]
    fn a_closed_market_refuses_buys_and_honours_sells() {
        let m0 = seeded();
        let b = palw_model_buy_quote_v1(&m0, 10 * MSK).expect("a buy");
        let closed = PalwModelMarketV1 { closed_to_buys: true, ..b.after };
        assert!(palw_model_buy_quote_v1(&closed, 10 * MSK).is_none());
        assert!(palw_model_sell_quote_v1(&closed, b.units_out).is_some());
    }

    /// The sink script names its class and nothing else does.
    #[test]
    fn the_sink_script_names_its_class_and_is_an_early_return() {
        let class = Hash64::from_bytes([7u8; 64]);
        let spk = palw_model_sink_spk_v1(&class);
        assert_eq!(spk.script()[0], OP_RETURN);
        assert_eq!(palw_model_sink_class_v1(&spk), Some(class));
        let other = crate::dns_finality::p2pkh_mldsa87_spk(&[9u8; 64]);
        assert_eq!(palw_model_sink_class_v1(&other), None);
        let mut forged = spk.script().to_vec();
        forged[3] ^= 1;
        assert_eq!(palw_model_sink_class_v1(&ScriptPublicKey::new(0, crate::tx::ScriptVec::from_slice(&forged))), None);
    }

    /// The sell message binds every field a holder commits to.
    #[test]
    fn the_sell_message_binds_its_fields() {
        let (c, h) = (Hash64::from_bytes([1u8; 64]), Hash64::from_bytes([2u8; 64]));
        let m = palw_model_sell_message_v1(&c, &h, 5, 6);
        assert_ne!(m, palw_model_sell_message_v1(&c, &h, 5, 7));
        assert_ne!(m, palw_model_sell_message_v1(&c, &h, 6, 6));
        assert_ne!(m, palw_model_sell_message_v1(&h, &c, 5, 6));
        assert!(m.starts_with(PALW_MODEL_SELL_SIGN_DOMAIN_V1));
    }
}
