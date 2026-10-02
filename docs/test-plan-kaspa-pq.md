# kaspa-pq Test Plan — Pure PQ-PoW · ML-DSA · LtHash · Hash64 · DNS Overlay

Purpose: verify that **every kaspa-pq change relative to upstream rusty-kaspa**
behaves to spec ([`kaspa-pq-spec.md`](kaspa-pq-spec.md)), and that each item is
**reproducible on simnet / devnet / testnet / staging-mainnet**. This plan is the
gate before any network promotion. PALW (the LLM proof-of-work that produces
testnet-12's blocks) has its own test and certification docs and is out of scope here.

> Status legend per item: ☐ not started · ◐ partial (unit only) · ☑ done.
> "C==V" = the construction path (block template) and the validation path
> produce byte-identical results.

---

## 0. Method

**Test levels**
- **L1 unit / pure-fn** — `cargo test` on the pure functions (deterministic, DAG-free).
- **L2 crate integration** — consensus/txscript/pow/rpc-core suites.
- **L3 single node** — one `kaspad` per network flavor; RPC + mining + submit.
- **L4 multi-node DAG harness** — ≥2 miners over real GHOSTDAG; reorgs, propagation, overlay-active reward & finality (see §G).
- **L5 live network** — the running simnet/devnet/testnet/staging meshes.

**Network matrix** (every functional item must pass on each, unless gated):
| net | role | overlay (`dns_params`) | gate (`dns_activation_daa_score`) |
|---|---|---|---|
| simnet | fast in-proc / CI | `Some(GENESIS_ACTIVE_DNS_PARAMS)` | 0 |
| devnet | integration mesh | `Some(GENESIS_ACTIVE_DNS_PARAMS)` | 0 |
| testnet-12 | public test (current) | `Some(PALW_T12_DNS_PARAMS)` (production set, testnet `required_work_depth`) | 0 |
| staging-mainnet | release candidate | `Some(PRODUCTION_DNS_PARAMS)` | 0 |

(Values from `consensus/core/src/config/params.rs`; every shipped preset is genesis-active.)

**Pass bar baseline (must stay green every PR):**
`cargo check --workspace --exclude muhash-fuzz --all-targets` clean +
`kaspa-consensus-core` (dns_finality) + `kaspa-consensus` + `kaspa-pow` +
`kaspa-txscript` + `rpc-core` suites pass, and `bash scripts/pq-ci-guard.sh` is green.

---

## A. Hash64 — 64-byte BlockHash / TransactionId (BLAKE2b-512)

Spec: all consensus hashes widened 32→64 bytes; `Hash64` type.

| id | item | level | pass criteria |
|---|---|---|---|
| H64-1 | BLAKE2b-512 hashing vectors (block, tx, merkle, coinbase payload) | L1 | match pinned vectors |
| H64-2 | Borsh/serde round-trip of Hash64 in headers, tx, outpoints, stores | L1/L2 | byte-stable |
| H64-3 | Genesis hashes per net (`*_GENESIS.hash`) match running chain | L2/L3 | const == runtime |
| H64-4 | Merkle roots (hash_merkle_root, accepted_id_merkle_root) | L2 | recomputation equality |
| H64-5 | DB key widths (StakeBonds 68-byte key, outpoint keys) | L2 | no truncation/collision |
| H64-6 | RPC hex round-trip (64-byte → 128 hex) gRPC + wRPC | L3 | symmetric ser/de |

## B. Pure PQ-PoW — Layer-0 BLAKE2b-512 heavy hash

Spec: PoW = BLAKE2b-512 Layer-0 (`POW_ALGO_ID_KHEAVYHASH`), µs verify, NOT Argon2id.

| id | item | level | pass criteria |
|---|---|---|---|
| POW-1 | PoW verify accepts valid / rejects invalid nonce | L1/L2 | deterministic |
| POW-2 | Difficulty / DAA window retargeting | L2 | target tracks hashrate |
| POW-3 | Block template carries `pow_layer0` algo id | L2/L3 | header field correct |
| POW-4 | Miner (`pq-miner`) finds blocks at target BPS | L3 | blocks accepted |
| POW-5 | `--min-block-interval-ms` throttle (split-brain mitigation) | L4 | vDaa monotonic, tips 1–3 |

## C. ML-DSA-87 — post-quantum signatures, P2PKH, multisig

Spec: ML-DSA-87 (libcrux); `OpCheckSigMlDsa87`=0xa6, `OpCheckMultiSigMlDsa87`=0xa7, `OpBlake2b512`=0xc4; P2PKH-ML-DSA-87 (`misaka*` prefix); raised script/element/sig-script size limits (spec §2, §6). P2SH is consensus-disabled in PQ-only mode, so multisig is exercised at the opcode level only.

| id | item | level | pass criteria |
|---|---|---|---|
| MLDSA-1 | keygen/sign/verify + context binding (`MLDSA87_TX_CONTEXT`); NIST ACVP vectors (`cargo test -p kaspa-txscript --lib mldsa87`) | L1 | round-trip ok |
| MLDSA-2 | `OpCheckSigMlDsa87` P2PKH spend round-trip | L2 | `test_mldsa87_p2pkh_spend_roundtrip` |
| MLDSA-3 | `OpCheckMultiSigMlDsa87` 2-of-3 (opcode level) | L2 | `test_multisig_mldsa87_2_of_3` |
| MLDSA-4 | script/element/sig-script size limits (16384/8192/16384) | L2 | large spends accepted |
| MLDSA-5 | mempool standardness + mass limits for an ML-DSA-87 P2PKH tx | L3 | ML-DSA tx admitted |
| MLDSA-6 | address codec (`misaka`/`misakatest`/`misakasim`/`misakadev`) | L1/L2 | encode/decode, prefix isolation |
| MLDSA-7 | **e2e ML-DSA send on-chain** (WASM `signTransactionMlDsa87` → submit → mined) | L5 | recipient UTXO confirmed |
| MLDSA-8 | 16-input ML-DSA send (transient-mass bound) | L5 | accepted + mined |

## D. UTXO commitment — homomorphic accumulator (LtHash32_1024)

Spec: homomorphic UTXO-set commitment. The backend is LtHash32_1024 (`crypto/muhash/src/lib.rs`; the `MuHash` struct name is kept for API compatibility), finalized to a 64-byte keyed BLAKE2b-512 `Hash64`.

| id | item | level | pass criteria |
|---|---|---|---|
| UTXO-1 | add/remove homomorphism (order-independent) | L1 | commitment invariant |
| UTXO-2 | genesis `utxo_commitment` == hash(premine set) all nets | L2 | const == computed |
| UTXO-3 | commitment matches after a chain of blocks (C==V) | L2/L4 | header commitment verifies |
| UTXO-4 | reorg: commitment rolls back + replays correctly | L4 | post-reorg == recomputed |

## E. Tokenomics — 25B cap (10B premine + 15B/20yr 5%-decay)

Spec: `MAX_SOMPI` (`consensus/core/src/constants.rs`), `MISAKA_PREMINE_CAP_SOMPI` (`consensus/core/src/config/premine.rs`).

| id | item | level | pass criteria |
|---|---|---|---|
| TOK-1 | emission table sums to ~15B over 20yr (`verify_total_emission`) | L1 | within budget |
| TOK-2 | per-block subsidy schedule (`subsidy_test`, decay 0.95/yr) | L1 | matches table |
| TOK-3 | premine: genesis totals exactly 10B; one main-wallet ML-DSA-87 P2PKH UTXO, with every carve-out (bond collateral, floats, community) paid out of it | L2/L3 | UTXOs present, spendable, sum == cap |
| TOK-4 | circulating-supply RPC == `MISAKA_PREMINE_CAP_SOMPI` + emitted | L3 | matches |
| TOK-5 | `MAX_SOMPI` = 25B never exceeded (incl. bps rounding surplus) | L1/L4 | ≤ cap |

## F. DNS Overlay (ADR-0009 / 0017 / 0018) — the BFT-free PoS finality layer

The overlay is gated by `dns_activation_daa_score`, which is `0` on every shipped
preset (testnet-12 included), so the items below are live behavior, exercised via
pure-fn tests, the §G harness and the running network.

### F1 — StakeBonds (lifecycle, reorg, spend-gate)
| id | item | level | pass |
|---|---|---|---|
| DNS-B1 | bond population from accepted txs (`bond_mutations_from_accepted_txs`) | L1/L2 | insert/slash recorded |
| DNS-B2 | `effective_bond_status` precedence (slashed>unbonding>activation), DAA-derived | L1 | status correct |
| DNS-B3 | per-block `ActiveBondView` apply/revert mirrors store; reorg-reversible | L2/L4 | view == store @ pov |
| DNS-B4 | bond-UTXO spend-gate: locked while Pending/Active/unbonding/Slashed | L2 | non-releasable spend rejected |
| DNS-B5 | ADR-0016 stake-lock: output-0 value bound to `amount` | L2 | mismatch rejected |

### F2 — Attestations (all-active, ADR-0017)
| id | item | level | pass |
|---|---|---|---|
| DNS-A1 | every active bond attests; NO committee/sortition/ticket/commit-reveal | L1/L4 | self-declared VSC, no sampling |
| DNS-A2 | attestation message binds (network_id, epoch, target, target_daa, vsc, bond_outpoint) | L1 | sig bound, no replay |
| DNS-A3 | §B.4 eligibility = active bond + valid ML-DSA sig → else block INVALID | L2 | bad attestation rejects block |
| DNS-A4 | recency window + (bond,epoch) within/cross-block dedup | L1/L2 | one reward per pair |
| DNS-A5 | shard tx stays STATELESS at tx-level (A.2); strictness is block-validity | L2 | mempool admits, block gates |

### F3 — StakeScore + DnsState / DnsConfirmation
| id | item | level | pass |
|---|---|---|---|
| DNS-S1 | φS quality floor (`epoch_stake_credit`); φS=0 == old linear EXACTLY | L1 | byte-identical at floor 0 |
| DNS-S2 | StakeScore aggregation over selected chain (dedup, normalize by expected stake) | L1/L4 | deterministic |
| DNS-S3 | `update_dns_state` once/epoch (throttle), C==V | L2/L4 | DnsState written per epoch |
| DNS-S4 | `getDnsConfirmation` RPC (gRPC+wRPC), `available:false` until active | L3 | fields correct |

### F4 — Two-dimensional reorg dominance (§H)
| id | item | level | pass |
|---|---|---|---|
| DNS-H1 | `check_dns_reorg_rule` TwoDimensionalDominance arm (pure) | L1 | out-Work AND out-Stake required |
| DNS-H2 | `dns_reorg_allows`: candidate exiting confirmed prefix → gate engages | L4 | forge rejected |
| DNS-H3 | per-branch bond views (candidate=in-loop, canonical=store@prev_sink) | L4 | deterministic, no split |
| DNS-H4 | `reorg_mode` selected by param (every shipped preset is `TwoDimensionalDominance`; `HardCheckpoint` is test-only) | L2 | rule selected by param |
| DNS-H5 | censorship CANNOT forge dominance (can't fake StakeScore) | L4 | reorg attack fails |

### F5 — DnsHealth (degrade, never forge)
| id | item | level | pass |
|---|---|---|---|
| DNS-D1 | `derive_dns_health` 7-case (Active/QualityLow/Censored/Disabled) | L1 | matches table |
| DNS-D2 | health surfaced on `getDnsConfirmation`; never gates block validity | L3 | liveness signal only |
| DNS-D3 | selective attestation censorship → included-fraction drops, φS gates, NOT redistributed | L4 | denominator fixed |

### F6 — Reward economics (§F / §E / §D, staged, Node 0)
| id | item | level | pass |
|---|---|---|---|
| DNS-R1 | §F split dust-free (subsidy 75/25/0, normal 90/10/0, finality 75/25/0); parts sum to input | L1 | exact |
| DNS-R2 | §E participation: stake-proportional, expected-stake denominator, pool cap, unspent not minted | L1 | anti-capture |
| DNS-R3 | §E **full 25%** paid (`validator_participation_bps=10000`, bonus 0) | L1 | participation == validator pool |
| DNS-R4 | §D base inclusion bounty: includer paid ∝ newly-included stake; unspent burned | L1/L4 | reuses §E dedup, C==V |
| DNS-R5 | coinbase carve C==V (construct template == validate) | L2/L4 | byte-identical coinbase |
| DNS-R6 | value conservation: Σ emitted ≤ Σ(subsidy+fees) (service/unspent/bonus burned) | L1/L4 | supply ≤ cap |
| DNS-R7 | staged rollout by DAA: Stage1 100/0 → Stage2 90/10 → Stage3 75/25 (NOT DnsState-keyed) | L2/L4 | stage by daa threshold, C==V |
| DNS-R8 | Node 0: no service pool; service bps = 0 burned | L1 | no node payout |
| DNS-R9 | "validator ⇒ node": attest requires synced node + self-fund; no reward if absent | L4 | non-attester earns 0 |

### F7 — Slashing (equivocation)
| id | item | level | pass |
|---|---|---|---|
| DNS-X1 | equivocation evidence genuineness rule (forged evidence cannot slash) | L2 | forged rejected |
| DNS-X2 | bond→Slashed mutation + reverts on reorg | L2/L4 | reorg-safe |
| DNS-X3 | slashing distribution side-effect (reporter reward + burn), atomic | L2 | output == computed |
| DNS-X4 | non-reveal fault = N/A (commit-reveal removed by ADR-0017) | — | no such path |

### F8 — Validator node (Phase 11/12)
| id | item | level | pass |
|---|---|---|---|
| VAL-1 | `--enable-validator` eligibility → sign → fund → submit (mode=Active) | L3 | shard tx submitted |
| VAL-2 | equivocation guard (persistent SignedEpochStore) blocks double-sign | L3 | second sign refused |
| VAL-3 | `getValidatorStatus` (gRPC+wRPC) 9-variant ladder | L3 | status correct |
| VAL-4 | sidecar (`kaspa-pq-validator`) over 127.0.0.1 wRPC: keygen/status/run | L3 | connects, state machine |
| VAL-5 | mass-based fee + utxoindex funding lookup | L3 | funded correctly |

---

## G. DAG integration test harness (the pre-mainnet gap)

The carve / reorg-gate / reward paths run on every genesis-active network; beyond
pure-fn tests + C==V-by-construction, this harness exercises the **Active overlay**
over a real BlockDAG in-process. Existing anchors: `dns_overlay_active_chain_validates`
(DAG-1) and `pos_v2_reward_bearing_attestation_validates` (DAG-2) in
`consensus/src/pipeline/virtual_processor/tests.rs`.

**G-HARNESS spec**
- Build a `kaspa-consensus` test fixture with `dns_params = Some(..)` and
  `dns_activation_daa_score = 0` (overlay ACTIVE from genesis), parameterized:
  N validators with funded `StakeBond` txs, configurable attestation
  inclusion/censorship, configurable BPS and propagation delay.
- Drive: mine a multi-block DAG, land bonds → activate → include signed
  `StakeAttestationShard` txs → produce **reward-bearing** coinbase → validate.

| id | scenario | asserts |
|---|---|---|
| DAG-1 | overlay-active empty chain (extends `dns_overlay_active_chain_validates`) | activation doesn't break production/validation; C==V on empty coinbase |
| DAG-2 | reward-BEARING: real bond + signed attestation → non-empty §E/§D coinbase | C==V byte-identical; value conservation; rewards to correct spk |
| DAG-3 | staged transition across `dns_activation` and `full_reward_split` heights | ratios switch by daa; C==V at each stage |
| DAG-4 | reorg with competing branches (TwoDim gate) | only out-Work∧out-Stake branch wins; bond/reward/StakeScore revert correctly |
| DAG-5 | selective censorship of one validator across the window | reward fairness (other miners include); finality degrades not forges |
| DAG-6 | slashing on equivocation in-DAG | bond slashed, reporter paid, reorg-safe |
| DAG-7 | multi-node (L4) propagation under load (≥4 miners) | converges, vDaa monotonic, no supply drift |

**Exit:** DAG-1..6 green in CI (in-proc) + DAG-7 on a devnet/staging mesh before mainnet activation.

---

## H. Regression vs upstream rusty-kaspa (must still hold)

| id | item | pass |
|---|---|---|
| REG-1 | GHOSTDAG ordering / blue-work / mergeset unchanged | upstream consensus tests |
| REG-2 | mempool, fee/mass, RBF, orphan handling | mining/mempool suites |
| REG-3 | pruning, IBD, virtual resolution | daemon integration |
| REG-4 | RPC parity (existing methods) gRPC+wRPC | rpc_tests |
| REG-5 | gRPC proto namespace `protowire.kaspapq` unchanged (wire compat) | client interop |

---

## I. Per-stage exit criteria

- **simnet/CI:** L1+L2 + DAG-1..6 green; baseline bar green.
- **devnet:** L3 single-node + L4 mesh (DAG-7); genesis-active overlay stable.
- **testnet (testnet-12):** all above, observed on the genesis-active public network; staged-rollout transition observed.
- **staging-mainnet:** full F (Active) over the harness + a mesh, finality argument restated (ADR-0017 full-participation stake-weighted), reward value-conservation audited, slashing drill, then set the real `dns_activation_daa_score` / `full_reward_split_daa_score`.

---

## J. Known gaps / follow-ups (tracked)

- DAG harness DAG-3..6 (staged transition + reorg + censorship + slashing in-DAG) — not confirmed in tree (this plan's main new work).
- §D quality-gate bonus + §E quality-bonus + urgency multiplier — deferred (need epoch-cumulative inclusion accumulator; gated on the burn-vs-SecurityRollover decision).
- DNS finality fee class — unwired (post-subsidy concern).
- Finality argument restatement (ADR-0017) before mainnet.
