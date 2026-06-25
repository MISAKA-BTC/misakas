# MISAKA EVM Flat State Backend — Operator Runbook v0.1 (C-01, slice S10)

Operational guide for migrating a node from the legacy per-block **206** EVM state snapshot
(storage O(state × blocks)) to the **C-01 flat latest-state backend** (storage O(latest state +
window diffs + checkpoints)). Companion to the design `docs/misaka-evm-state-backend-design-v0.1.md`.

**Everything here is node-local and consensus-neutral.** None of these flags changes a committed
byte, a commitment, or the chain a node follows; they change only what THIS node persists, seeds
from, and serves. A misconfiguration degrades availability of one node (at worst a HALT — see
§6), never the chain. All flags are off by default, so a node that sets none of them behaves
exactly as before.

Applies to an `--features evm` build on an EVM-active network. On a default (non-evm) build or an
EVM-inert network the flags are accepted but inert (no EVM state exists).

---

## 1. The four flags and their dependency chain

Each flag is a no-op (logged) unless the one before it is also set — the node demotes and warns
rather than doing something unsafe.

| Flag | Effect | Requires | Reversible? |
|---|---|---|---|
| `--evm-shadow-state-backend` | Maintain the flat store in parallel with 206 and HALT on any divergence (the live differential). 206 stays authoritative. | — | Yes (just stop setting it) |
| `--evm-flat-authoritative` | Seed the executor from the flat/reconstruct parent (validated byte-identical to 206 first). 206 still written. | shadow | Yes |
| `--evm-retire-206` | Stop writing the per-block 206 snapshot. Flat is the sole persisted post-state. | flat-authoritative (+shadow) | Yes, **if** flat-authoritative stays on across the revert |
| `--evm-prune-legacy-206` | One-shot startup bulk-delete + compaction of the legacy 206 rows. | retire-206 effective + recent/archive history + flat verified current | **NO — irreversible** |

Also relevant: `--evm-history-mode={head,recent,archive}` (§12 retention). Use **recent or
archive**, not `head`, for any node that retires/prunes 206 (head keeps no §12 history to
reconstruct a non-head parent, and `--evm-prune-legacy-206` refuses on head).

The flags are designed to be set **cumulatively and left on**: a steady-state retired node runs
`--evm-shadow-state-backend --evm-flat-authoritative --evm-retire-206` (+ optionally
`--evm-prune-legacy-206`, which self-disables once the legacy store is empty).

---

## 2. Migration procedure (per node, one at a time)

Do this on **one node at a time**, ideally a non-validator / non-producer first, and let each
phase soak before the next. Because every phase is consensus-neutral, a mixed fleet (some retired,
some not) is fine indefinitely.

### Phase 0 — baseline
- Confirm the build: `kaspad --version` and that it was built `--features evm`.
- Confirm history mode is `recent` or `archive` (not `head`) if this node will retire/prune.
- Note the data-directory size (for the eventual storage-win measurement).

### Phase 1 — shadow (warm up + validate)
Add `--evm-shadow-state-backend` and restart. This builds/advances the flat store alongside 206
and checks `flat_state_root == committed state_root` every block.

- **Watch for (good):** `[evm-shadow] flat state backend (re)seeded to block <h>` once at start,
  then steady operation. `[evm-shadow] flat state backend re-based across a reorg to block <h>` on
  reorgs is normal.
- **Soak** until the node has processed well past the pruning depth with **zero** divergence HALTs
  (see §6). This proves the flat backend is faithful before anything depends on it.
- Do not proceed until the flat store is **current at the head** — i.e. the node has run shadow
  through at least one virtual commit at the live tip after a clean restart.

### Phase 2 — flat-authoritative seed
Add `--evm-flat-authoritative` (keep shadow on) and restart. The executor now seeds from the flat
store, validated byte-identical to 206 before use; 206 is still written, so this is fully
reversible.

- This is the highest-confidence step: any flat error HALTs **before** the seed is used (it can
  never falsely disqualify a block).
- Soak again. A clean soak here is the green light for retirement.

### Phase 3 — retire 206 writes
Add `--evm-retire-206` (keep shadow + flat-authoritative) and restart. New blocks stop writing the
206 snapshot. The legacy 206 rows remain on disk and are reclaimed gradually by the normal
per-block pruner as the pruning point advances.

- **Confirm it took effect:** you should NOT see the demotion warning
  `[C-01] --evm-retire-206 ... it is a no-op` (that warning means a prerequisite is missing).
- The storage growth flattens immediately; existing 206 rows drain over ~one pruning window.

### Phase 4 — bulk-prune legacy 206 (optional, irreversible, immediate storage win)
Add `--evm-prune-legacy-206` and restart **once**. At startup the node verifies the safety gate and,
if it passes, `delete_range`s the entire legacy 206 store and compacts the range.

- **Success:** `[C-01 S9b-prune] flat backend verified current at EVM head <h>; IRREVERSIBLY
  bulk-deleting ...` then `[C-01 S9b-prune] legacy 206 snapshot store reclaimed; space returned to
  the OS after compaction.`
- The compaction is synchronous and can take a while on a large store — expect a longer-than-usual
  startup, once.
- After it runs the store is empty, so the flag is a no-op on every later boot
  (`[C-01 S9b-prune] no legacy 206 snapshot rows present; nothing to reclaim.`) — you may leave it
  on or remove it.
- **If it refuses** (any `... Refusing the ... delete ... 206 left in place` line), do NOT force it.
  Read the message and fix the cause (see §4) — the refusal means the flat backend is not yet a safe
  replacement and 206 is correctly being preserved.

---

## 3. Rollback

| Currently at | To roll back | How |
|---|---|---|
| Phase 4 (pruned) | — | **Not possible** — legacy 206 is deleted. Recover by re-IBD or restoring a pre-prune backup. This is why Phase 4 gates so hard. |
| Phase 3 (retired) | Phase 2 | Remove `--evm-retire-206`, **keep `--evm-flat-authoritative` ON**, restart. Blocks committed while retired have no 206 but are reconstructed + root-validated from the flat store. Do NOT drop flat-authoritative at the same time while retire-committed blocks are still unpruned (their parents would have neither a 206 nor a flat seed → the verifier HALTs rather than fork); wait until the chain advances past them. |
| Phase 2 (flat seed) | Phase 1 | Remove `--evm-flat-authoritative`, restart. Executor seeds from 206 again. |
| Phase 1 (shadow) | Phase 0 | Remove `--evm-shadow-state-backend`, restart. |

**Recommendation:** keep `--evm-prune-legacy-206` un-set until you are confident you will never want
to roll Phase 3 back on that node. Phases 1–3 are freely reversible; Phase 4 is the point of no
return for that data directory.

---

## 4. `--evm-prune-legacy-206` refusal reasons (Phase 4 pre-flight)

The startup pre-flight refuses (warn + skip, no data touched) on any of these. All are "fix and
retry," never "force."

- `--evm-prune-legacy-206 is set but --evm-retire-206 is not effective` — turn on the full chain
  (`--evm-retire-206 --evm-flat-authoritative --evm-shadow-state-backend`).
- `--evm-history-mode=head keeps no §12 state history ...` — switch to `recent`/`archive`.
- `the flat state pointer is absent — the flat backend was never initialized` — you skipped the
  shadow warm-up; do Phases 1–2 first.
- `the flat backend is stale — it materializes block <x> but the canonical EVM head is <y>` — let the
  node run (shadow + flat-authoritative) until the flat store converges to the head, then restart.
- `the flat backend is NOT faithful at the EVM head ... account rows hash to <a> but the committed
  state_root is <b>` — the on-disk flat state does not match the committed root (corruption / an
  incomplete warm-up). Do NOT prune; re-shadow / restore the flat backend first.
- `requires a kaspad built with --features evm` — wrong build.
- `could not probe/read ...` (store I/O) — transient; investigate disk, retry.

---

## 5. Metrics & observability (what to watch)

There is no separate metrics endpoint for these yet; watch the structured log lines.

**Healthy steady state (retired node):**
- Periodic `[evm-shadow] flat state backend (re)seeded/re-based to block <h>` — flat tracking the head.
- No `DIVERGENCE` / `CORRUPT` / `HALT` lines.
- No `[C-01] ... it is a no-op` demotion warnings (means a flag chain is incomplete).

**Counters worth scraping from logs (until first-class metrics land — Stage 2 follow-up):**
- **root-divergence count** — occurrences of `C-01 shadow seed DIVERGENCE` / `C-01 S9b retired-206
  seed DIVERGENCE` / `C-01 shadow seed CORRUPT`. Target: **0**. Any non-zero is a halt-worthy
  backend fault (§6).
- **seed-unavailable count** — `[evm-shadow-seed] seed unavailable for <h>` (and the
  `... read failed ...` siblings). On a non-retired node these are benign 206 fallbacks; on a
  **retired** node a non-head Unavailable becomes a HALT (no 206 to fall back to) — track it.
- **flat-hit vs reconstruct** — the seed path is FlatHead (fast, head) vs reconstruct (§12, non-head).
  A high reconstruct rate indicates frequent deep reorgs / non-head seeding; expected to be ~0 in
  steady state.
- **prune outcome** — the single `[C-01 S9b-prune] ...` line per boot (reclaimed / nothing to
  reclaim / refused-with-reason).

---

## 6. Failure modes & response

The backend follows design §7: **never serve a wrong state root — HALT instead.** A HALT stops this
node; it never forks or falsely disqualifies a block, so chain integrity is unaffected.

| Log (panic/HALT) | Meaning | Response |
|---|---|---|
| `C-01 shadow seed DIVERGENCE` | The flat/reconstruct seed ≠ the committed 206 snapshot (shadow on, 206 present). The backend is buggy/corrupt. | Node halted by design. 206 is still authoritative — disable `--evm-flat-authoritative`/`--evm-shadow-state-backend`, restart to recover, and file the divergence (it is a real backend bug). Do **not** retire/prune on this node. |
| `C-01 S9b retired-206 seed DIVERGENCE` | The flat head pointer root ≠ the committed parent root (retired, 206 absent). Stale/wrong pointer. | Halted by design. Restore the flat backend (re-shadow from a good snapshot / re-IBD); only re-enable retire once shadow runs clean. |
| `C-01 shadow seed CORRUPT` | A §12 reconstruction is internally inconsistent (bad diff/checkpoint/missing code). | Halted by design. The §12 archive is damaged; restore from a healthy node / re-IBD. |
| retire-206 HALT: `no flat/reconstruct seed could be obtained for EVM-active selected parent` | Retired node, flat seed Unavailable (e.g. non-head reorg with §12 GC'd, or head mode). | Halted by design. Use `--evm-history-mode=archive`, or temporarily roll back to Phase 2 (`--evm-flat-authoritative` on, `--evm-retire-206` off) so the node can seed again, then re-shadow. |

General rule: a HALT means "this node refused to risk a wrong root." The recovery is always to make
the flat backend trustworthy again (re-shadow / restore / re-IBD) **or** step back one phase — never
to bypass the check.

---

## 7. Fleet sequencing

- Migrate **observers/RPC nodes first**, then **validators/producers**, one at a time, each soaked.
- A mixed fleet (some at Phase 0, some at Phase 4) is consensus-safe — they commit identical bytes;
  only their local storage/serving differs.
- Producers: the template path seeds from the same validated flat parent as the verifier, so c==v
  holds; no special handling, but soak producers longest before retiring.
- Pruned-IBD: an importing node seeds the flat store from the verified pruning-point snapshot (S8),
  so a freshly-IBD'd node can run flat-authoritative once shadow confirms it; it has no legacy 206 to
  prune.

---

## 8. Stage 2 (not in this runbook)

The per-block root CPU is still O(state) in Stage 1 (the HashBuilder rebuild), and the executor
still seeds by **eagerly** materializing the parent (the lazy `FlatBackedCacheDB` seam exists —
`StoreFlatReader` — but is not on the live path; see design §6 S9c). The incremental-MPT root,
the live lazy-seed cutover, and `eth_getProof` are Stage 2, gated on measuring that per-block
root-CPU actually matters. This runbook covers only the Stage-1 storage migration.
