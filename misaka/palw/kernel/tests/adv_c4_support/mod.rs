//! Shared support of the C4 adversarial suites: the independent settlement accountant (`Book`) and the strict-wire block fold.
//! Not a test crate (a subdirectory of `tests/`); included with `#[path = "adv_c4_support/mod.rs"] mod support;`.
#![allow(dead_code)]

use std::collections::BTreeMap;

use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{KernelLedgerV1, LedgerBlockV1, LedgerEventV1 as E, LedgerTxV1};
use misaka_palw_kernel::settle::{SettlementInstructionV1 as S, SettlementKindV1 as K};

// ── the independent accountant ───────────────────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct Book {
    pub coll: BTreeMap<Digest, u64>,
    pub res: BTreeMap<Digest, u64>,
    pub paid: BTreeMap<Digest, u64>,
    pub slashed: u64,
    pub routed: u64,
    pub burned: u64,
    pub issued: u64,
    pub withdrawn: u64,
    /// Net collateral the CONSUMER added or removed through `sync_bond` (outside the ledger's control).
    pub external: i128,
    pub initial: i128,
}

impl Book {
    pub fn from_ledger(l: &KernelLedgerV1) -> Self {
        let mut b = Book::default();
        for (k, r) in &l.bonds {
            b.coll.insert(*k, r.collateral);
            b.res.insert(*k, r.reserved);
        }
        b.burned = l.burned;
        b.initial = b.supply();
        b
    }

    pub fn supply(&self) -> i128 {
        self.coll.values().map(|v| *v as i128).sum::<i128>()
            + self.paid.values().map(|v| *v as i128).sum::<i128>()
            + self.burned as i128
            + self.withdrawn as i128
            - self.issued as i128
    }

    pub fn sync(&mut self, bond: Digest, collateral: u64) {
        let old = self.coll.insert(bond, collateral).unwrap_or(0);
        self.external += collateral as i128 - old as i128;
    }

    pub fn apply(&mut self, s: &S) -> Result<(), String> {
        let (b, a) = (s.bond, s.amount);
        let c = self.coll.get(&b).copied().unwrap_or(0);
        let r = self.res.get(&b).copied().unwrap_or(0);
        match s.kind {
            K::ReserveClaim | K::ReserveDemand => {
                if a > c.saturating_sub(r) {
                    return Err(format!("reserve {a} past free {}", c.saturating_sub(r)));
                }
                self.res.insert(b, r + a);
            }
            K::ReleaseClaim | K::ReleaseDemand => {
                if a > r {
                    return Err(format!("release {a} of {r}"));
                }
                self.res.insert(b, r - a);
            }
            K::SlashFraud | K::SlashDefault => {
                if a > r || a > c {
                    return Err(format!("slash {a} of reservation {r} / collateral {c}"));
                }
                self.res.insert(b, r - a);
                self.coll.insert(b, c - a);
                self.slashed += a;
            }
            K::SlashFiling => {
                if a > c.saturating_sub(r) {
                    return Err(format!("fee {a} past free {}", c.saturating_sub(r)));
                }
                self.coll.insert(b, c - a);
                self.slashed += a;
            }
            K::AccuserReward | K::DemanderShare => {
                *self.paid.entry(b).or_default() += a;
                self.routed += a;
            }
            K::Burn => {
                self.burned += a;
                self.routed += a;
            }
            K::FinalReward => {
                *self.paid.entry(b).or_default() += a;
                self.issued += a;
            }
            K::Withdraw => {
                if r != 0 || a != c {
                    return Err(format!("withdraw {a} of {c} with {r} reserved"));
                }
                self.coll.remove(&b);
                self.res.remove(&b);
                self.withdrawn += a;
            }
        }
        Ok(())
    }

    /// After every block: slashes were routed 1:1, value is conserved, and the ledger's view of every bond is the book's.
    pub fn check(&self, l: &KernelLedgerV1) -> Result<(), String> {
        if self.slashed != self.routed {
            return Err(format!("slashed {} but routed {}", self.slashed, self.routed));
        }
        if self.supply() - self.external != self.initial {
            return Err(format!("value not conserved: {} vs {}", self.supply() - self.external, self.initial));
        }
        for (k, row) in &l.bonds {
            let (c, r) = (self.coll.get(k).copied().unwrap_or(0), self.res.get(k).copied().unwrap_or(0));
            if (c, r) != (row.collateral, row.reserved) {
                return Err(format!("bond {:02x?}: book {c}/{r}, ledger {}/{}", &k[..2], row.collateral, row.reserved));
            }
        }
        if l.burned != self.burned {
            return Err(format!("burn checksum {} vs {}", l.burned, self.burned));
        }
        Ok(())
    }
}

/// Fold one block through the STRICT wire form; a refusal must leave the root byte-identical.
pub fn fold(l: &mut KernelLedgerV1, b: &LedgerBlockV1, mut book: Option<&mut Book>) -> Vec<E> {
    let mut out = Vec::new();
    if l.begin_block(b.daa).is_err() {
        return out;
    }
    let absorb = |ev: &[E], book: &mut Option<&mut Book>| {
        if let Some(bk) = book.as_deref_mut() {
            for e in ev {
                if let E::Settlement(s) = e {
                    bk.apply(s).unwrap_or_else(|why| panic!("a settlement a real bond refuses: {s:?}: {why}"));
                }
            }
        }
    };
    for tx in &b.txs {
        let before = l.root();
        let r = match tx {
            LedgerTxV1::SyncBond { bond, collateral } => {
                l.sync_bond(*bond, *collateral);
                if let Some(bk) = book.as_deref_mut() {
                    bk.sync(*bond, *collateral);
                }
                Ok(Vec::new())
            }
            LedgerTxV1::AttestArtifact { artifact_root } => {
                l.attest_artifact(*artifact_root);
                Ok(Vec::new())
            }
            LedgerTxV1::PanelCovered { claim } => l.apply_panel_tally(claim, true),
            LedgerTxV1::Object { auth, object } => l.apply_encoded(&object.encode(), auth),
        };
        match r {
            Ok(ev) => {
                absorb(&ev, &mut book);
                out.extend(ev);
            }
            Err(r) => {
                assert_eq!(l.root(), before, "a refusal changed the state: {r}");
                out.push(E::Refused { tx: r.object, why: r.why });
            }
        }
    }
    let ev = l.tick();
    absorb(&ev, &mut book);
    out.extend(ev);
    if let Some(bk) = book.as_deref() {
        bk.check(l).unwrap_or_else(|why| panic!("settlement accounting: {why}"));
    }
    out.retain(|e| !matches!(e, E::Settlement(_)));
    out
}


/// **Flip every byte of `obj`'s encoding** (three variants each) and apply the result to `base` through the strict decoder:
/// no panic, a refusal leaves the root byte-identical. Cheap enough for a minimal ledger (the base is cloned back only after an
/// applied mutant). Returns `(applied, refused, panics: Vec<(offset, message)>)`.
pub fn exhaustive_flips(
    base: &KernelLedgerV1,
    obj: &misaka_palw_kernel::ledger::KernelRouteObjectV1,
    signer: Digest,
) -> (u32, u32, Vec<(usize, String)>) {
    use misaka_palw_kernel::ledger::AuthV1;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let enc = obj.encode();
    let mut l = base.clone();
    l.begin_block(base.daa + 1).unwrap();
    let (mut applied, mut refused) = (0, 0);
    let mut panics = Vec::new();
    for i in 0..enc.len() {
        for f in [0x01u8, 0x80, 0xFF] {
            let mut b = enc.clone();
            b[i] ^= f;
            let before = l.root();
            match catch_unwind(AssertUnwindSafe(|| l.apply_encoded(&b, &AuthV1 { signer_bond: signer }))) {
                Err(_) => {
                    panics.push((i, format!("flip {f:#04x}")));
                    l = base.clone();
                    l.begin_block(base.daa + 1).unwrap();
                }
                Ok(Err(_)) => {
                    refused += 1;
                    assert_eq!(l.root(), before, "a refusal changed the state (byte {i} ^ {f:#04x})");
                }
                Ok(Ok(_)) => {
                    applied += 1;
                    l = base.clone();
                    l.begin_block(base.daa + 1).unwrap();
                }
            }
        }
    }
    (applied, refused, panics)
}
