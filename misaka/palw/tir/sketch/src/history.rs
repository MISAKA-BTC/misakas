//! **Per-row history sketches** (RFC-0007 Part II, §II.3 — modelled in the first prototype, built here; open question 9 settled
//! 2026-10-03: build them before any long-context class relies on Part II).
//!
//! An attention node multiplies the query with a history: `Q·Kᵀ` (scores, one per history row) and `P·V` (one weighted sum of the
//! value rows). Both are activation × activation products, so a seat checks them with a vector of its own that no producer sees.
//! The first prototype built that vector's sketch of the whole window at every position — `O(window · d)` a position, which is the
//! work of the product it is checking. A **per-row** sketch moves the cost to the row that arrives:
//!
//! * **`P·V`** — `out = Σ_h p[h]·V_h`. With a secret `σ` over the row width, keep `S_V[h] = Σ_d σ[d]·V_h[d]` **once per appended
//!   row**. Then `Σ_d σ[d]·out[d]` must equal `Σ_h p[h]·S_V[h]`: a check that reads `d + H` numbers, never `H·d`.
//! * **`Q·Kᵀ`** — `scores[h] = Σ_d q[d]·K_h[d]`. With a secret `ρ[h]` per history row (indexed by absolute position, fixed per job, layer
//!   and head) keep the running `R[d] = Σ_h ρ[h]·K_h[d]`, updated `O(d)` per appended row. Then `Σ_h ρ[h]·scores[h]` must equal
//!   `Σ_d q[d]·R[d]`, again `d + H` numbers.
//!
//! **Sound for the reason a weight's sketch is**: the vectors are uniform and secret, and every row is verified before it is
//! sketched (a seat only sketches rows its own checks established). An error `e` in `out` (or in `scores`) survives a check only if
//! `Σ σ[d]·e[d] = 0` (or `Σ ρ[h]·e[h] = 0`) mod `p`, which for a nonzero `e` and a uniform vector happens with probability exactly
//! `1/p` (`tests/history.rs` observes it at a toy prime). **A sketch is as secret as its vector**: the structure has no encoding, its
//! `Debug` prints no value, and it is wiped on drop.
//!
//! A sliding window is a first-class case: [`TirHistorySketchV1::evict_oldest`] takes the leaving key row and subtracts its term.

use crate::field::TirSketchModulusV1;
use crate::secret::TirSketchKeysV1;

fn wipe(values: &mut [u64]) {
    values.fill(0);
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    std::hint::black_box(&*values);
}

/// One attention head's history sketches over one modulus: `σ`, `S_V[h]` and `ρ[h]` per retained row, and the running `R`.
pub struct TirHistorySketchV1 {
    m: TirSketchModulusV1,
    d: usize,
    job: [u8; 32],
    occurrence: u16,
    node: u16,
    sigma: Vec<u64>,
    v_rows: Vec<u64>,
    rho: Vec<u64>,
    r_keys: Vec<u64>,
    next_pos: u32,
}

impl TirHistorySketchV1 {
    /// Sketches for the head of `occurrence` / `node` (any stable pair that names the head) of job `job`, rows of width `d`, over
    /// modulus `m`. `σ` is drawn here, once; `ρ[h]` is drawn per appended row.
    pub fn new(keys: &TirSketchKeysV1, job: &[u8; 32], occurrence: u16, node: u16, m: TirSketchModulusV1, d: usize) -> Self {
        // σ at a position no history row can have (`u32::MAX`), so it is never one of the ρ's.
        let sigma = keys.fresh_vector(job, u32::MAX, occurrence, node, m, d);
        Self { m, d, job: *job, occurrence, node, sigma, v_rows: Vec::new(), rho: Vec::new(), r_keys: vec![0; d], next_pos: 0 }
    }

    /// The rows retained.
    pub fn retained(&self) -> usize {
        self.v_rows.len()
    }

    /// The absolute position the next appended row takes.
    pub fn next_pos(&self) -> u32 {
        self.next_pos
    }

    /// **Append one verified row**: `S_V` gains `Σ_d σ[d]·V[d]`, and `R` gains `ρ·K` with `ρ` drawn for this position. `O(d)`.
    pub fn append(&mut self, keys: &TirSketchKeysV1, k_row: &[i64], v_row: &[i64]) {
        assert_eq!(k_row.len(), self.d, "a key row has the head's width");
        assert_eq!(v_row.len(), self.d, "a value row has the head's width");
        let rho = keys.fresh_vector(&self.job, self.next_pos, self.occurrence, self.node, self.m, 1)[0];
        self.v_rows.push(self.m.dot_i64(v_row, &self.sigma));
        for (r, k) in self.r_keys.iter_mut().zip(k_row) {
            *r = self.m.add(*r, self.m.mul(rho, self.m.reduce_i128(i128::from(*k))));
        }
        self.rho.push(rho);
        self.next_pos += 1;
    }

    /// **Drop the oldest row** (a sliding window): its `S_V` leaves, and its `ρ·K` term is subtracted from `R` — so the caller hands
    /// the key row that is leaving. Returns `false` with nothing changed when no row is retained.
    pub fn evict_oldest(&mut self, k_row: &[i64]) -> bool {
        assert_eq!(k_row.len(), self.d, "a key row has the head's width");
        if self.v_rows.is_empty() {
            return false;
        }
        let rho = self.rho.remove(0);
        self.v_rows.remove(0);
        for (r, k) in self.r_keys.iter_mut().zip(k_row) {
            *r = self.m.sub(*r, self.m.mul(rho, self.m.reduce_i128(i128::from(*k))));
        }
        true
    }

    /// **Check a served `P·V`**: `p` holds the weight of each retained row (oldest first), `out` the served weighted sum of the
    /// retained value rows. `d + H` multiply-adds; `false` for a shape that is not the retained window's.
    pub fn check_pv(&self, p: &[i64], out: &[i64]) -> bool {
        if p.len() != self.v_rows.len() || out.len() != self.d {
            return false;
        }
        self.m.dot_i64(out, &self.sigma) == self.m.dot_i64(p, &self.v_rows)
    }

    /// **Check served `Q·Kᵀ` scores**: `q` is the query (width `d`), `scores` one per retained row (oldest first). `d + H`
    /// multiply-adds; `false` for a shape that is not the retained window's.
    pub fn check_qk(&self, q: &[i64], scores: &[i64]) -> bool {
        if q.len() != self.d || scores.len() != self.rho.len() {
            return false;
        }
        self.m.dot_i64(scores, &self.rho) == self.m.dot_i64(q, &self.r_keys)
    }
}

impl Drop for TirHistorySketchV1 {
    fn drop(&mut self) {
        wipe(&mut self.sigma);
        wipe(&mut self.v_rows);
        wipe(&mut self.rho);
        wipe(&mut self.r_keys);
    }
}

impl std::fmt::Debug for TirHistorySketchV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TirHistorySketchV1({} rows of {}, ..)", self.v_rows.len(), self.d)
    }
}
