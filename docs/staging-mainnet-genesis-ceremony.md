# staging-mainnet (testnet-200) genesis ceremony — runbook

- **Status:** Runbook (procedure defined; execution is an operator action). Fulfils ADR-0048 DoD
  item "ceremony 手順書(分配・faucet・鍵)".
- **Date:** 2026-07-27
- **Scope:** the PALW staging-mainnet rehearsal network `testnet-200`
  (`STAGING_MAINNET_PALW_PARAMS` / `STAGING_PALW_GENESIS`, ADR-0048). This is a REHEARSAL for the
  final mainnet re-genesis, not mainnet itself — but it is run WITH the mainnet discipline, because
  its purpose is to exercise exactly that discipline before the identity is final.
- **Reuses:** `docs/release-signing-procedure.md` (key handling), `docs/operator-runbook-calibration-testnet.md`
  §7 (faucet), `docs/incident-runbook.md`. This doc specializes them to the staging genesis event.

## 0. What is already fixed by code (do NOT re-decide at ceremony time)

The genesis SHAPE is frozen in `consensus/core/src/config/`. The ceremony instantiates it; it does
not choose new parameters.

- `STAGING_PALW_GENESIS` (`genesis.rs`): `version = 4`, `bits = 0x207fffff` (real-PoW easy start),
  coinbase payload tag `"misaka-staging-palw"`, fresh timestamp. Its `hash`/`hash_merkle_root` are
  MINTED and pinned; `test_genesis_hashes` + `gen_kaspa_pq_genesis_hashes` recompute them. The
  Header-v4 conversion binds the canonical EMPTY spam accumulator at finalize
  (`header_v4_regenesis_commits_the_canonical_empty_spam_accumulator`).
- `STAGING_MAINNET_PALW_PARAMS` (`params.rs`): `palw_activation_daa_score = 0`,
  `palw_algo4_accept = false`, `palw_compute_work_scale = 0`, `palw_spam = PUBLIC_REGENESIS_CANDIDATE`,
  `skip_proof_of_work = false`, `palw_requires_archival = false`, `palw_requires_peer_allowlist = true`,
  full-scale `finality_depth = 432_000` / `pruning_depth = 1_080_000`. The ADR-0043 G6 bound is a
  node-local allocation policy (no preset knob).
- Premine: `misaka_premine_utxos(NetworkType::Testnet)` — the single testnet 10B main UTXO
  (`TESTNET_MAIN_ADDRESS`), committed by the genesis `utxo_commitment` MuHash. testnet-200 inherits
  this because it is `NetworkType::Testnet` suffix 200. **The staging premine is a testnet premine
  with no value** (see §3). Emission schedule is the global 30-year MSK table (BPS-scaled), unchanged.

If any of the above must change, that is a CODE change reviewed under ADR-0048, not a ceremony-time
decision — stop and amend the preset.

## 1. Pre-conditions (the launch gate)

Do not run the ceremony until ALL are green (ADR-0048 §2 起動条件):

1. ADR-0043 sibling bound implemented + unit/e2e green (threshold freeze is done ON staging, so
   implementation only). — **done** (allocation policy + 15/15 reachability tests + g6 re-measure Bounded).
2. ADR-0044 harness + prune-then-replay E2E + ADR-0042 §1c fixture green. — **done**
   (`palw_full_lifecycle_prune_then_replay_e2e`).
3. ADR-0045 D1 (SS-04 non-retroactive) landed. — **done** (`244deae`).
4. `STAGING_MAINNET_PALW_PARAMS` + `STAGING_PALW_GENESIS` + preset pin tests. — **done**
   (consensus-core 570/570).
5. This runbook reviewed.

## 2. Keys (three roles, all OFF the build/validator hosts)

Follow `release-signing-procedure.md` §Keys for generation/storage/handover discipline. For staging
there are three distinct ML-DSA-87 credentials; reusing one across roles is forbidden (cross-role
replay discipline — the LOCKED signature-domain table):

- **Genesis/premine key** — controls the testnet premine UTXO. Generated offline, sealed backup.
  Its credential id (`unkeyed BLAKE2b-512(vk)`) is published in the staging README out of band.
- **Faucet key** — a hot operator wallet funded FROM the premine (§3), used for the drip (§4).
  Rotatable; compromise is a bounded loss (no-value network).
- **Bootstrap provider/scheduler keys** — the initial bonded roles the vertical exercises. One
  credential per role, named in the ops log; never the genesis key.

## 3. Premine distribution plan (no value)

testnet-200's premine is the testnet 10B main UTXO — a REHEARSAL allocation, explicitly valueless
and voided on every reset (§6). The distribution below is a script the operator runs, not a
governance allocation:

1. From the premine UTXO (genesis key), split into role-sized funding UTXOs:
   - faucet wallet: enough for ~months of drip at the §4 caps;
   - N bootstrap provider bonds + M scheduler bonds at the `min_provider_bond_sompi` floor (ADR-0046
     暫定 10 MSK) + fees;
   - a reserve UTXO kept in the genesis wallet for a mid-run top-up (dated ops-log line if used).
2. Every split is one dated ops-log line (destination credential id, amount, purpose).
3. NO allocation implies mainnet allocation. The final mainnet re-genesis mints its OWN premine plan;
   staging distribution is thrown away on reset. State this in the ops channel with the split post.

## 4. Faucet policy (inherit calibration §7, invite-only)

- **No public faucet endpoint** for staging. Funds move via the operator ops channel; disbursement
  is the faucet key signing a transfer. Per-request caps = bonds+fees of ONE role, never top-ups.
- Every disbursement is a dated ops-log line; the faucet wallet is reconciled weekly (mismatch = S1).
- Disbursements VOID on reset; post that with every disbursement.
- A later PUBLIC staging phase (the 30-day permissionless soak, §5 演習) needs real anti-Sybil
  machinery (rate limits per IP/ASN, PoW or allowlist attestations, drained-wallet alarms). That is
  NOT built and is out of scope until the allowlist opens — recorded so the gap is visible.

## 5. Launch → rehearsal exercise sequence (ADR-0048 §3)

Run in order, each with a pass/fail ops-log entry:

1. **Boot** both nodes on testnet-200 with `--testnet --netsuffix=200`
   (datadir `misaka-testnet-200`, P2P 26511). `palw_requires_peer_allowlist = true` ⇒ closed net
   first: peers pinned by the operator.
2. **Genesis verify:** every node independently recomputes the genesis hash/merkle (the pinned
   constants) and refuses to peer on mismatch. Real PoW is on (`skip_proof_of_work = false`): confirm
   algo-3 blocks carry real work and algo-4 is hash-floor-exempt.
3. **Warm-up window:** measure the shortest path from genesis to a first mint attempt (the negative
   consequence ADR-0041 named — `min_beacon_burial_daa` + DA burial + activation lead). Record it.
4. **Full vertical:** premine → bond → manifest → beacon commit/reveal → DNS confirm → leaf-chunk →
   DA challenge/response → attested certificate → algo-4 mint → payment. This is the ADR-0044 harness
   path, now on a real multi-node net (the harness is the single-process rehearsal of exactly this).
5. **First pruning pass:** because samples are on chain from genesis, confirm the first pruning point
   advances WITHOUT the ~21,600-block delay ADR-0041 measured on a retrofit. The three snapshot-writer
   coherence defects the ADR-0044 harness fixed (view/paid-work/accum version+dup) must stay fixed —
   the pass must not refuse to advance.
6. **ADR-0046 L1/L2 re-measurement** (spam ramp + bond/slash) on staging; freeze thresholds here.
7. **Multi-node soak:** pruning / catch-up / reorg / DA-withholding over time (ADR-0042 §4 requirement).
8. **Open allowlist → 30-day permissionless public soak** (mainnet-readiness ledger C). External,
   wall-clock; needs the faucet anti-Sybil work in §4.

## 6. Reset procedure

Any reset voids all balances. Wipe `misaka-testnet-200` datadirs on every node, re-announce in the
ops channel (all disbursements void), and restart from §5.1. Because the identity is staging, resets
are cheap and expected during the rehearsal — the point is to shake out the ceremony, not to preserve
state. The FINAL mainnet re-genesis copies the frozen staging params/genesis shape verbatim (values
only, no new design — ADR-0048 §4) and is NOT reset.

## 7. What this ceremony does NOT authorize

- It does not flip `palw_algo4_accept` (stays false; acceptance is the activation ladder A/B gates).
- It does not set `palw_compute_work_scale > 0` (stays 0; gated on ADR-0045 D1 fraud+slash e2e).
- It does not mint mainnet or imply mainnet allocation.
- Its locally-generated keys/registry do not imply production authority
  (`docs/security-model.md` trust boundary).
