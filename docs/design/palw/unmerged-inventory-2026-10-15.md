# Unmerged-work inventory and merge sweep — 2026-10-15

- **Branch:** `integ/merge-sweep`, cut from `origin/integ/rfc02-rfc06-12-g14` at `6b1a01acd` (which contains `origin/pre` `5aa52b886`). Later integ moves (`f21d224d5`, `a431fbd56`) are merged in.
- **Scope:** every local or remote branch not merged into integ (304 refs at the start, from `git branch -a --no-merged`), every worktree with uncommitted changes (52 of 308 worktrees), and the item list the coordinator gave.
- **Method:** the 10-12 reorg moved the PALW crates under `misaka/palw/`, apps under `misaka/apps/`, and scripts into purpose folders, so `git cherry` patch-ids miss renamed paths. Each class below was decided by content: grep at integ for the functions, constants, test names, files and doc sections a branch adds. Three read-only helper agents did the per-branch checks for the 10-01..10-11 and 09-19..09-29 groups. I re-checked their NEEDS-MERGE verdicts before acting on them.
- **Rules applied:**
  - Codex branches: only pushed commits are merged, never worktree WIP.
  - One merge per source.
  - Pre-reorg content is ported onto current paths, not replayed.
  - Consensus-visible numbers are never renumbered silently; a collision is reported (§3).
  - New consensus behaviour stays dormant.
  - **PRINCIPLES §8 (permanent user decision, 2026-10-15):** anything whose purpose is a cryptographic proof of computation (ZK, SNARK/STARK, zkVM/zkML, PCS succinct proofs, recursive proofs) is EXCLUDED and not merged.
- **Heads merged (sources keep moving):** codex/integration-followup `d3b94dc7f`, codex/workspace-rfc02-completion `cffcc4f60`, codex/rfc-completion-integration `717105d4d`, integ/validation `4165239b4`, integ/rfc0018-dag `6fcd5baf3` (later heads arrive via integ `7a444c1f4`), integ `7a444c1f4`. Anything pushed after these needs another round.
- **Not done here:** no cargo build, check or test was run. A separate validation lane compiles this branch (§7).

## 1. Summary

| class | refs | notes |
|---|---:|---|
| NEEDS-MERGE, merged or ported in this sweep | 26 | about 20 sources, §2 (local and `origin/` twins are counted separately) |
| NEEDS-MERGE, **not merged: user decision or a build-capable port needed** | 26 | §6: r8-work, audit/rpc-index, mn, p0a-held-hybrid, the rfc3/rfc4 ref2 crates, issue-103, evm-bridge-caps, bridge BR-2/5, MTP, q6vnqe |
| ALREADY-IN | 103 | includes zero-unique merge and release refs, the 26 `worktree-agent-*` refs, and `codex/rfc12-remove-dns-bft` (ported by integ `c6efcb7b2`) |
| SUPERSEDED | 105 | mostly t11-era (08-xx..09-14) branches and the 06–08 `main` lineage, replaced by t12 / palw_state_v2 / ADR-0144+, ADR-0172 (no VM), ADR-0169 and RFC-8 v2 |
| EXCLUDED (PRINCIPLES §8) | 1 | `origin/feat/mil-v0` (SP1/Plonky3 STARK shielded spends). Also excluded at module level: `claim_circuit_v5` inside the Codex branch (removed, §2) |
| ABANDONED / EXPERIMENT | 43 | probes, drills, measurements, scratch, rescue snapshots, dependabot bumps |
| **total refs** | **304** | `git branch -a --no-merged` against `6b1a01acd` at the start |
| DIRTY-ONLY (worktrees) | 52 of 308 | §5; nothing committed from them |

The rows in §4 group refs; the counts above are per ref.

## 2. What was merged (first-parent commits on `integ/merge-sweep`)

| commit | source | content |
|---|---|---|
| `e03fb8aec`, `c79e9114b` | `origin/codex/integration-followup` (`f85a3e68e`, then `d3b94dc7f`) | G14 TIR acquisition: source units, owned acquisition rows, protected work debits. Brings `dc5bc6bc7` (kaspad lib test fixes). |
| `ec05ca010`, `cabc35fe6`, `c75c9efcd` | `origin/codex/workspace-rfc02-completion` (`02bc81893`, `53d9054ba`, `cffcc4f60`) | RFC-0002 owning39 native sources, v39 jobs, DA proofs, Master bindings, receipt windows, live audit collection, paged Master39 authority. |
| `b52870dde`, `76e1517dd`, `429c1a6f0`, `bf064303f` | `origin/codex/rfc-completion-integration` (`fd86751bd`, `19565a458`, `0cdf53c48`, `717105d4d`) | Native recovery and snapshot capacity; RFC07 compact coverage; `palw_claim_compact_v4`; `palw_material_release_recovery_v2`; typed provider material ownership; liability and vesting mutation cascades. Codex withdrew the circuit compiler itself (`317033a56`) and now rejects retired computation-validity transcripts (`1b3a7c7e2`). |
| `fece783af` | `origin/integ/validation` (`4165239b4`) | Check-ladder compile fixes for ADR-0176/0177 and RFC-0016/0017, plus test-only guard fixes. |
| `566416582`, `c2e20d71e` | `origin/integ/rfc0018-dag` (`f9276f924`, `6fcd5baf3`) | RFC-0018 dormant fences, Gate C SecurityCertified (aux 136), premise-based settlement RPC, 300M closed-model drill. Integ later merged this line itself (`a431fbd56`). |
| `7913b0218`, `81cebe3ff`, `fa838438d` | `origin/integ/rfc02-rfc06-12-g14` (`f21d224d5`, `a431fbd56`, `7a444c1f4`) | The no-computation-proof decision, RFC-0012 DNS-BFT retirement (`palw_dns_retirement_v1`), and the 10-15 user decisions. |
| `e992f0c73` | exclusion | Removes `misaka/palw/kernel/src/claim_circuit_v5` (Boolean/hash circuits and R1CS "required by a polynomial IOP"), which arrived with `b52870dde`. Codex later withdrew the same module (`317033a56`). |
| `6ac10adcb` | `p2/hb-flake` | Test only: `hb_fork_choice_probe` g2/h1 lottery-flake fix, twin replay order, `#[ignore]` `sink_order_probe`. `7913b0218` restores three `use` lines that the first resolution dropped. |
| `6f0004408` | `fe4addb72` (first commit of `codex/scan-dynamic-panel`) | The panel view keeps proof-bearing seats; explorer `#/llm` and `#/models` pages. RPC view only. |
| `1ee9ce88c` | `tir/ref2-h7check` (+ `tir/ref2`) | ref2 finding H7, the TopK demand row, and the fence2 differential, ported to `testing/reference/palw-tir-ref2`. |
| `660b8f5a3` | `rfc5/evm-jobs` `25ef65c37` (doc only) | `docs/design/palw/vm/gate-report-2026-10.md`: the RFC-0005 Part I gate is closed (0 of 131 configs need a VM). |
| `e471b9d71` | `origin/fix/network-dos-hardening` (06-27, never merged) | p2p `MAX_REQUEST_HASHES = 128` on relay, IBD and body requests; 320 MiB transport ceiling (was 1 GiB); route-id wrap; eth-rpc WS idle timeout, strict handshake and header cap; bridge dashboard routing and health-check cap. |
| `82b870ab3` | `origin/audit/p0-bridge-dnsseeder` (06-27, never merged) | Bridge static path-traversal fix; negative-duration config rejection; dnsseeder TCP slowloris bound, ported to `misaka/apps/dnsseeder`. |
| `80c312be0` | `adv/c4r4` leftovers | `consensus/core/tests/c4r4_reporter_share_49.rs` (F-C4R4-16 is fixed, so its guard runs; F-C4R4-18 stays ignored) and the round-4/4b record sections. |
| `24df95834` | `adv/c4-e2e` | The ten round-1 adversarial test files, ported to `misaka/palw/{challenge,kernel,remote,sdk}/tests`. |
| `ceef62383` | `feat/kv-codec` `69386573b` | Node-only: the panel residency site carves the producer's attempt reserve, as the producer site does. |
| `fcac2d1f4` | `feat/misakaoptions-access-first` `36902502d` (gas fix only) | Options site: a 120,000 gas ceiling for a topping-up wallet the node cannot estimate. |
| `8467e7a69` | `docs/restructure`, `rcore/cap-spec2`, `audit/claim-capacity`, `e633f820a` | Docs only, added files only: ADR-0152 with its verbatim archive, ADR-0154, ADR-0155, ADR-0160 with its capacity notes, the claim-capacity map, and the explorer user dictionary. |

## 3. Collisions and conflicts

### 3.1 Numbering

| space | values | status |
|---|---|---|
| aux tables | 137 and 138 | **Resolved 2026-10-15 (integ/rfc06-12-remaining):** Codex `PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2` moved 134 → 137 and `PAID_CLAIM_COMPACT_V4` 135 → 138; RFC-0016 keeps 132–135 and RFC-0018 136. Compact V4 is retired under PRINCIPLES §8 (no generation, acceptance or credit; aux 138 keeps fail-closed decoders only). |
| aux tables | 136 | RFC-0018 `PALW_SECURITY_CERTIFICATION_TABLE_V1`. New and free. |
| stage-execution actions (tag V14) | 34..=40 (v28), 41..=70 (v39) | Codex extends the stage namespace. Funded actions (tag V11) stay ≤ 46. No collision. |
| proto message fields | settlement 13/14, claim view 39 | Integ's RFC-0018 additions. A scan of `rpc.proto`, `messages.proto` and the p2p protos after the merges finds no duplicate field number in any message. |
| object tags, kernel inner, RPC ops, proto, p2p | — | No new values in anything merged. Range in use is unchanged: tags 164–169, 230, 240–242; inner 63–71, 73–75; RPC ≤ 244 and 255; proto ≤ 1307 and 1330–1331; p2p 108. |
| not merged, would collide | see §6 | `audit/rpc-index`: RPC 230–243 against integ's 230/231/240–244. `mn/model-declarations`: object tag 140, carriage tail `0xE2`, its own "ADR-0171". `rfc5/gvm`: tags 120–124. `rfc5/evm-jobs`: tag 110. `rfc8/claim-backed-blocks`: tag 96 and deltas 107/108. `rcore/exec-class`: deltas 111/112. These numbers must not be reused. |

### 3.2 Content conflicts resolved

- **Module lists** (`consensus/core/src/lib.rs`, `misaka/palw/kernel/src/lib.rs`, `palw_state_v2.rs` `#[path]` folds, `palw_state_v2/tests.rs`, `hb_fork_choice_probe.rs` tail): resolved as the union, with duplicate `mod` lines dropped.
- **`config/params/identity.rs`:** took Codex's fence order (lifetime, challenge, compact_v4, release_recovery_v2, monetary) so the fingerprint write order matches the source branch.
- **`palw_rule_e.rs`:** integ's `Result`-returning `palw_rule_e_prefers_v1` (checked pair) combined with RFC-0018's `capital_v2` order and even split.
- **`rpc/service` `get_palw_settlement_call`:** kept Codex's post-read agreement recheck and added RFC-0018's `uncertified_final_anchors` and `security_certification_premise` to the settled-branch response.
- **`palw_panel_view_v1.rs`:** took the scan branch's eligibility rule (declared or registry-active, composite ancestry ready).
- **`contrib/misakascan-t12/app.js`:** kept integ's 10-07 ADR-0125 round-block classification. DEPLOY.md keeps both sections; the branch's becomes 10e.
- **`AGENTS.md`** (add/add): integ's text first, then Codex's work rules.
- **`docs/PRINCIPLES.md`:** now holds two §8 sections: Codex rfc-completion's detailed text and integ's. Both are kept verbatim.
  - Codex workspace-rfc02 (`cffcc4f60`) carries a third wording of §0/§6/§7/§8. Integ's wording was kept in those 5 hunks, so that variant is not in this branch.
  - **The Lead should consolidate §8**, because PRINCIPLES changes only by user decision.
  - `AGENTS.md` keeps all three versions; RFC-0007 keeps integ's sentence plus the honest-fraction sentence.
- **`palw_state_v2/tests/public_prefix_fold_v1.rs`:** the two Codex workspaces diverged here. Kept integration-followup's setup, which closes the claim through `staged_selected_default_charge_v1`, over rfc-completion's direct `Voided` write.
- **`wire_generated.rs`:** a hand union of both generated outputs. `scripts/drills/gen_rfc06_native_wire.py` **could not be re-run.** It stops at its shape-review gate on `crate::palw_tir_public_owned_v1::PalwTirPublicChunkRecordV1`, a handwritten Borsh codec from Codex integration-followup with no `REVIEW` entry (digest `394604fa…6d92`). A partial run rewrote 11 files; I restored them. Someone has to review that codec and add the entry before regeneration.
- **Not merged, reverted, or aborted:** `origin/fix/issue-103-invalid-parent-message` was aborted. It turns `RuleError::MissingParents` into a two-field variant, and 27 call sites at integ match the one-field shape (§6).

## 4. Classification table

### 4.1 Codex and other lines active on 10-15

| item | class | evidence | action |
|---|---|---|---|
| origin/codex/workspace-rfc02-completion (5 + 1 newer) | NEEDS-MERGE | pushed; based on pre `5aa52b886` | merged `ec05ca010`, `cabc35fe6` |
| origin/codex/integration-followup (7 + 1) | NEEDS-MERGE | pushed; contains `integ/validation` `dc5bc6bc7` | merged `e03fb8aec`, `c79e9114b` |
| origin/codex/rfc-completion-integration (22 + 6) | NEEDS-MERGE, one module EXCLUDED | pushed; based on RFC-0017 `3ee84387c` | merged three times; `claim_circuit_v5` removed (`e992f0c73`; Codex `317033a56` agrees) |
| origin/integ/validation (4 unique) | NEEDS-MERGE (missed by the list) | compile fixes for integ content | merged `fece783af` |
| origin/integ/rfc0018-dag (2, then 5) | NEEDS-MERGE (missed by the list) | RFC-0018, dormant | merged `566416582`, `c2e20d71e`; integ took it too (`a431fbd56`) |
| origin/codex/rfc12-remove-dns-bft (1, `344a1b882`) | ALREADY-IN (ported) | integ `c6efcb7b2` carries it onto the reorganized tree as fence-gated retirement (approach b) | none |
| claim_compact_v1 (in integ via pre) / Codex claim_compact_v4 | **ruling needed** | sum-check with a Fiat–Shamir transcript beside Freivalds; Codex is rewriting v1 as "direct probabilistic checks and historical wire decoding" | flagged (§6) |

### 4.2 main and docs

| item | class | evidence | action |
|---|---|---|---|
| main = origin/main = codex/post-commit-challenge-policy = claude/native-redemption-impl-d36ce9 (`3408af2d5`, 2 docs commits) | ALREADY-IN | `51c94d36e` is in integ (`rfc-0008-v0-test-record.md`, later edits); `3408af2d5` content is in integ: ADR-0171 §7, ADR-0044/0074 notes, `docs/rfc/evidence/0007-post-commit-randomness-alignment-2026-10-08.md` | none |
| 21 `worktree-agent-*` refs at `3408af2d5` | ALREADY-IN | same tip as main | delete when their worktrees are cleaned |
| worktree-agent-a654091211fbc1daa, a66cc3dbfce9e6b30, a94a4a9854599c385, aa4e68d12b0016b5a, aba3d06634734aab8 | ALREADY-IN | ancestors of integ | none |
| origin/codex/round-exec-acceptance-2026-10-10 | ALREADY-IN | `docs/design/palw/palw-round-exec-additional-acceptance-2026-10-10.md` in integ (+5-line Lead note) | none |
| codex/rfc8-exec-lane (`51c94d36e`) | ALREADY-IN | first `main` commit | none |
| origin/claude/docs-cleanup (11) | SUPERSEDED | 10-02 docs reshuffle to `docs/archive`; integ uses `docs/history/` after the 10-12 reorg | none |
| rfc/0002-tensor-ir, rfc/0003-image-generation, rfc/0004-0005-vm (local + origin) | SUPERSEDED | docs-only: the 09-27 restructure plus early RFC-0002..0005 text; integ's RFC texts are later revisions (RFC-0005 不採用). Their restructure part is the same as `docs/restructure`, see §4.4 | none |

### 4.3 Lane branches 10-01..10-11

| item | class | evidence | action |
|---|---|---|---|
| rfc12-wip = origin/rfc12/r12-wip (`e96e93234`) | ALREADY-IN | missing at `6b1a01acd`, ported by integ `c8ff90948` (rfc12_authority, rfc12_x17, `l2.rs` rule_e, gate facts, attestation deficits) | none |
| r8-work = origin/rfc8/complete (`2bd78e84b`) | NEEDS-MERGE (partial), **not done** | missing: `palw_exec_v2_retention_terms_for_class_v1`, `exec_class_liability_daa_v1`, `bond_budget_close_holding_v1`, `r8_neutral*` tests, "Lane R8 引継ぎ" doc. H-1, `PalwMarketFeePolicyV1` and `native_final.rs` are superseded | hand port; consensus-visible but dormant; signature changes across about 10 call sites (§6) |
| adv/c4r4 (20/7) | NEEDS-MERGE (small) | F-C4R4-03/05/10 fixes and the kernel/challenge/da16 c4r4 tests are in integ; the 49% test and the round-4/4b record sections were not | ported `80c312be0` |
| adv/c4-e2e (15/11) | NEEDS-MERGE (tests) | fix commits are patch-equal in integ; the 10 round-1 test files were absent | ported `24df95834` (unbuilt) |
| g14/d-node-e2e (25/6) | ALREADY-IN | RPC 210/211/212/230 with proto 1234–1239 and 1274/1275, `g14_onboarding_*`, `palw_onboarding_v1.rs`, `palw_panel_free_v1.rs` | none |
| rfc10/c2-panel (13/2) | ALREADY-IN | `GetPalwPanelV3Status = 220`, proto 1254/1255, `palw_panel_v3.rs` CLI, production-path record | none |
| onboard/a-frontend (19/1) | ALREADY-IN | `misaka/palw/sdk/src/census/onboarding.rs` | none |
| rfc6-12/merge2, g14/merge-codex-501a6a2, lead/e2e-verify | ALREADY-IN | single merge commits whose parents are in integ | none |
| g14/b-kernel, rfc15/x15-panel-free, rfc9/c1-remote, rfc1/c3-inference, onboard/c-beacon-pack | ALREADY-IN | all commits patch-equal | none |
| release/t12-daa9000, release/t12-int13-rc1 | ALREADY-IN (release abandoned 10-08) | patch-equal; DAA 9,000 release cancelled | none |
| rfc5/gvm (41) | SUPERSEDED | ADR-0172: BVM/GVM "will not be implemented"; RFC-0005 不採用; tags 120–124 collide | none |
| rfc5/evm-jobs (5) | SUPERSEDED (doc salvaged) | ADR-0172; tag 110 taken; `kaspa-evm` path gone | gate report ported `660b8f5a3` |
| audit/rpc-index (3) | NEEDS-MERGE, **blocked by collision** | accounting index and supply/evidence RPCs absent; RPC 230–243 collide | §6 |
| census/h4-budget, fix/int12-preflight-height | ALREADY-IN | `tir_with_search_deadline_v1`, `--judge-budget-secs`, `tir_choose_layout_judged_v1`, `max_window` | none |
| codex/scan-dynamic-panel (2) | NEEDS-MERGE (1 of 2) | `fe4addb72` view fix and explorer page absent | `fe4addb72` merged `6f0004408`; `2e8914205` (off-chain torrent model repository) is a user decision (ADR-0177) |
| rfc8/claim-backed-blocks (14) | SUPERSEDED | ADR-0169 "algo-11 deleted"; replaced by `palw_work_slice_v2.rs` | none; tag 96 and deltas 107/108 must not be reused |
| mn/model-declarations (11) | NEEDS-MERGE, **rework and collisions** | `ModelLineNamed`, `ModelDistributionDeclared`, `ModelSourceDeclared` absent; GAP-10 open; tag 140 and tail 0xE2 collide; its ADR-0171 number is taken; its Candidate gate conflicts with ADR-0177 | §6 |
| scan/backup-all (4) | ALREADY-IN | `palw_exec_view_v1.rs`, `palw_lane_view.rs`, lane-health card | none |
| p2/hb-flake (3) | NEEDS-MERGE (test) | `absorb_launched_landing`, `sink_order_probe` absent | merged `6ac10adcb` |
| rcore/exec-class (13) | SUPERSEDED | ADR-0168 banner: superseded by RFC-8 v2 (`palw_exec_v2_anchor.rs`) | none |
| origin/fix/ci-fmt-and-empty-range (1) | ALREADY-IN | `#[allow(clippy::reversed_empty_ranges)]` at `palw_verification_v2.rs:143`; the mass rustfmt is not taken | none |
| origin/rfc/0005-borrow-mainnet-vms | SUPERSEDED | ADR-0172 | none |
| rfc4/ref2 (42/35) | NEEDS-MERGE (optional), **not done** | `misaka-palw-improve-ref2` (~16k lines) absent; its head/promotion machine conflicts with ADR-0175 | §6 |
| rfc3/ref2 (29/22) | NEEDS-MERGE (optional), **not done** | `misaka-palw-gen-ref2` (~24k lines) and `ref2-findings.md` absent; integ's `palw_gen_vectors_v1.rs` cites its script | §6 |
| tir/ref2-h7check (3/2), tir/ref2 (1/1) | NEEDS-MERGE (small) | H7 finding cited by `palw_tir_fence2_v1.rs` but absent | merged `1ee9ce88c` |
| tir/fence2-sizing-wip (1) | SUPERSEDED | parked; integ uses `palw_tir_close_range_v1.rs` | none |
| scratch/g-measure | ABANDONED | "scratch … not for merging" | none |
| tir/tensor-store-design, rfc4/int11-rel | ALREADY-IN | `tensor-store.md`, `tools/drills/improve/dmdrive.py` | none |

### 4.4 Branches 09-19..09-29

| item | class | evidence | action |
|---|---|---|---|
| rcore/p0a-held-hybrid (9/4) | NEEDS-MERGE, **decision needed** | first 5 commits (`palw_gdn_key_heads`) are in; `56b7fc8db`, `2d29d5ad5`, `94637523e` are not (25 of 47 identifiers missing; `palw_class_admission_v2.rs:2321` still says no windowed builder answers the hybrid held site) | §6 (one dormant top-level fence `palw_held_hybrid_dissection`; no tag/inner/aux/RPC/proto/p2p/funded ids) |
| docs/restructure (22) | NEEDS-MERGE (records), partly ported | integ cites ADR-0152 in ~358 files with no file; ADR-0154/0155 absent; spec chapters 00–16, `docs/INDEX.md`, `docs/templates/`, 17 design notes, `docs/design/palw/archive/` absent | ADR-0152 + `archive/0152`, ADR-0154, ADR-0155 ported (`8467e7a69`). The spec chapters, templates and the remaining archive/design notes are not ported: they predate the ADR-0162..0178 amendments (§6). Never merge the branch itself; it slims ~126 ADRs amended later. |
| rcore/cap-spec2 (3), rcore/cap-spec (1, subset), audit/claim-capacity (3) | NEEDS-MERGE (records) | ADR-0160 v3 is cited by ADR-0166/0167/0178 and 114 files; the claim-capacity map is its evidence; `t12_claim_capacity.rs` is already in | ported (`8467e7a69`) |
| rcore/bridge-fixes (WIP `2d5aa205d`, "do not merge"), audit/bridge-0925 (2) | NEEDS-MERGE, **not done** | BR-1 and BR-6 are in integ. BR-2 (`misaka/evm/core/src/snapshot.rs:71` accepts an info-empty account with root-invisible storage) and BR-5 (`rpc/eth` resolves `safe`/`finalized` to the head in `eth_getLogs`) are open. BR-3/4 have reproducers only. The reproducer tests implement an older `EthBackend` trait | §6 |
| feat/kv-codec = origin/feat/kv-codec (`69386573b`) | NEEDS-MERGE (node-only) | the panel residency site still used `palw_class_residency_within_share_v1` | ported `ceef62383` |
| feat/misakaoptions-access-first (`36902502d`) | NEEDS-MERGE (gas fix only) | `addEstimatedGas` had no short-of-funds guard | gas ceiling ported `fcac2d1f4`; the storefront rewrite is a product decision |
| fix/audit-2026-09-19 (11/3) | NEEDS-MERGE (1 file) | 8 of 11 commits in; `docs/explorer/user-dictionary.md` missing although the explorer names it its contract; `805056aca` is an obsolete t11 disarm | file ported `8467e7a69` |
| mtp/t12 (16/15), mtp-recover-0910 (5), mtp/v6-deployed (4), feat/mtp-c6-options-usage (4), macbook/rescue/460defeb-wt-mtp (6) | NEEDS-MERGE **only if the points program is live** | the MISAKA Testnet Points scorer (off-chain, consensus-neutral) is absent at integ except `docs/testing/mtp-epoch2-partition-policy.md`; C6 rules v7 is `181c12ef1`. `460defeb-wt-mtp` is an empty-tree commit and must never be merged | §6 |
| origin/audit-q6vnqe = origin/claude/misaka-security-audit-framework-q6vnqe (`0ba444a46`, 7) | NEEDS-MERGE (docs), **decision needed** | 17 docs under `docs/security/audits/2026-09-claude-pre-freeze/` with exploit PoCs; retest "not started" | §6 (publishing PoCs) |
| rcore/hf-court-total-1300, push/main-daa1700, push/main-daa2000, tir/node-only, tir/node-only-int8, tir/large, study/big-models, rfc4/pipeline-da, phase-h/tools, fix/t12-ibd-order, rcore/f1-lane-accept-order, fix/t12-split-cbid, rcore/n7-lifecycle-ux-review, rcore/int-3, rcore/carrier-mempool-evict, rcore/beat-future-cap, rcore/panel-seed-exec-commitment, rcore/hb-attempt-eligibility-probe, launch/t12-candidate, origin/claude/wiki-t12-spec-sync-w2khzl, seat0/live-cec601e7, fix/t12-live-seat0, fix/base0-seat-memo-isolation, feat/t12-audit-period-100, palw-llm-pow-bps01, worktree-wf_8f1fb519-50c-1/2/3, macbook/rescue wt-m5b / wt-p2file / wt-court-proto / wt-deadline | ALREADY-IN | identifiers or lines present (e.g. `topological_order.rs`, `with_current_tx_ids`, `palw_lead_cap_virtual_parents`, `palw_h1_gated_object_at`, the SEAT-0 fix 57/57 identifiers); merges of ancestors | none |
| rcore/registry-resilience, rcore/hb-transparency-fix, rcore/position-fixes, codex/palw-state-root-fix-pre-origin, review/shipped-0925, feat/canonical-work, docs/adr-0152-v31-postedits, handoff/t12-rcore-20260924, macbook/rescue/1f9f6172-wt-ready, origin/next/misaka-next | SUPERSEDED | by `rcore/f1-registry-resilience`, `rcore/f1-hb-transparency`, the landed sink bind and ADR-0162 `virtual_v1_from`, `template.rs:928`, `palw_memory_ledger.rs:853`, the ADR-0152 archive, `rcore/readiness-escalation` | none (`position-fixes` leaves an optional CLI `EvmFloor` min-out floor) |
| audit/t12-lifecycle (78), verify/v02-determinism, verify/laneA-opanchor-probe, verify/sink-bind-probe, verify/pptake-probe, verify/n3-operator-da-probe, verify/hf-slash-probe, verify/hf-hdrcrash-probe, verify/br1-probe, drill/seat0-new, drill/seat0-old, drill/t12-pre-e93be0f2, drill/t12-pre-c8652a97, measure/t12-m13-2m-fp-price, next/misaka-next (docs-only "Consensus Book"), macbook/rescue wt-audit / wt-court / wt-live / wt-m3 / wt-pos / wt-s / wt-heldnode / wt-pool / 24499bec-wt-t12 | ABANDONED / EXPERIMENT | harness logs, probes "not for merge", "DRILL ONLY — never ship", `#[ignore]` measurement, pre-move rescue snapshots | none. Possible harvest from the rescue snapshots: `dos_l1_claim_flood.rs`, `dos_l3_probe.rs`, `dos_l5_*`, `adr0152*_probe`, `review_*_probe.rs`. Never merge `macbook/rescue/*` directly. |

### 4.5 Branches before 09-19 (t11 era and the 06–08 `main` lineage), grouped

| group | refs | class | evidence | action |
|---|---|---|---|---|
| Never-merged security fixes on the old `main` lineage | origin/fix/network-dos-hardening, origin/audit/p0-bridge-dnsseeder | NEEDS-MERGE | `MAX_REQUEST_HASHES`, `yaml_number_to_ms`, `safe_static_rel` and the dnsseeder semaphore are absent at integ; the target files still exist | merged `e471b9d71`, `82b870ab3` |
| Issue #103 missing-parent message | origin/fix/issue-103-invalid-parent-message, fix/issue-103-invalid-parent-message | NEEDS-MERGE, **not done** | `known_invalid_parents_note` absent; the variant shape change hits 27 sites | §6 |
| EVM bridge caps (fenced) | evm-bridge-caps, origin/evm-bridge-caps (`93169967c`) | **unresolved** | `BridgeBudget`, `BRIDGE_MAX_PER_TX_SOMPI` absent; no ADR or doc mentions the design | §6 |
| Old EVM lane work | origin/hotfix/evm-pp-serveability, origin/feat/evm-storage-profile, origin/feat/evm-rpc-index-prune | SUPERSEDED | integ has a later EVM reconstruction (`reverse_replay_to_block`, `materialize_snapshot`, flat backend, `delete_if_matches_batch` with a new signature); DNS-finality self-wedge is moot after RFC-0012 retirement | none |
| ZK shielded spends | origin/feat/mil-v0 (326) | **EXCLUDED (PRINCIPLES §8)** and SUPERSEDED | SP1 / Plonky3 STARK prover and verifier, recursion chains | do not merge |
| 07-xx `main` lineage snapshots | origin/test/partition-deadlock-harness, origin/main-backup-8107bfb-20260807 (+ local), origin/fix/palw-c01-c02-fork-relative, origin/claude/elastic-sammet-a7e804, origin/feat/compute-set-registry, origin/fix/ibd-invalid-poison-recovery, origin/feat/dns-dormancy-core-on-main, origin/pr-19-s5f-generator-mldsa-recalibration | SUPERSEDED / backups | 170–460 commits each on the pre-t11 lineage; VLT/DNS/compute-set designs retired by the PALW-only t12 line and RFC-0012 | none |
| t11 PALW design branches | palw-v2 (+origin), palw-llm-pow-unified, palw-freeprompt-v3 (+origin), palw-cross-class-v1, palw-rc-audit-fixes, palw-mainnet-rc-integration, palw-base0-depth, palw-remint-gpu-class (+origin), t11-launch-fixes-2026-08-23, t11-fixes-onto-main, launch-integration, palw-merge-resolved (+origin), drill-below-wall, palw-launch-derivation-drill, palw-rehearsal-fixes, palw-opening-siblings-derived (+origin), palw-certify-bind-artifact (+origin), palw-adr0080-uncharged-anchor, palw-adr-0063-0065-operator-tooling-and-bond-rules (+origin), palw-fixed-pricing-adr-0066 (+origin), palw-carrier-funding (origin + ibm), palw-jobs-export-fp (origin + ibm), feat/adr-0095-membership-serving (+origin), feat/adr-0096-court-arms, feat/adr-0096-partb-drill (+origin), origin/claude/adr-0096-partb-fence-uwo64p, fix/panel-pays-consensus-rent, fix/audit-2026-09-11 (+origin), backup/audit-5bed4405-base, fix/share-growth-counts-final-work, fix/borsh-enum-append-only, backup/adr-0118-wip, feat/adr-0124-on-palw-cleanup, codex/palw-cleanup-main, codex/palw-cleanup-main-v2, phase1-vlt-shadow-fixes, backup/cost-ceilings-pre-* | SUPERSEDED | t11 consensus (palw_v2, the ADR-0063..0124 drafts, ADR-0107 share growth: `share_growth_final_active` already in integ) replaced by palw_state_v2 and ADR-0144+ | none |
| t11 wallet, panel and tooling fixes | fix/wallet-send-foreign-floats (+origin), fix/issue-90-carrier-float (+origin), fix/issues-83-84 (+origin), fix/issue-89-windows-mmap (+origin), genesis/t11-faucet-float (+origin), fix/wallet-core-collapsible-if (+origin), cli-wallet-settlement (+origin), options-any-node (+origin), feat/misakaoptions-access-first | ALREADY-IN or SUPERSEDED | `wallet send --coinbase-only` is in `misaka/apps/cli/src/wallet.rs`; #83 locks DNS `StakeBondRecord` (retired); #89: integ's `base0/src/mmap.rs` refuses off-POSIX instead of mapping on Windows; t11 genesis floats are gone | none |
| MTP (older) | mtp-restore (+origin), mtp-onto-main (+origin), mtp-llm-weight, docs/mtp-participation-and-points (+origin) | follows the §4.4 `mtp/t12` decision | `mtp-onto-main` and `mtp-llm-weight` are contained in `mtp/t12`; the participant docs live on `docs/mtp-participation-and-points` | none until the MTP decision |
| Token program / Studio apps | token-program-phase-a (+origin), token-phase-b (+origin), origin/claude/misaka-studio-llm-app-8l6u8t, origin/claude/lm-studio-block-generation-62p4ct, chore-prune-main (+origin) | SUPERSEDED / ABANDONED | separate product lines on the pre-t12 tree; nothing references them at integ | none |
| Zero-unique / release / drill refs | release/t11-0911-owner-leg, release/t11-0911-bridge-liveness, docs/adr-0103-…, docs/palw-mainnet-audit-2026-09-06, palw-adr0082-impl, palw-adr0082-into-5f, palw-adr0082-c-capture, origin/scratch/5f-plus-impl-buildtest, palw-w5-court-close-carriage, palw-adr0077-p07-drill-pr (+origin), fix/palw-single-artifact-mapping, fix/bridge-freshness-structural-lag, fix/pow-unknown-algo-id-remote-panic (+origin), fix/chain-switch-counter-runaway (+origin), codex/qwen25-2m-converter (+origin), origin/add-misaka-probe, origin/add-misaka-miner-address-check, docs/t11-relaunch5d-identity, fix/ibd-candidate-selection (RC7 docs) | ALREADY-IN / ABANDONED | 0 patch-unique, or a record of a retired release | none |
| Dependency bumps | 11 `origin/dependabot/*` refs, origin/ci/rustfmt-all | ABANDONED | stale bumps against the old lineage; a mass rustfmt | none |

## 5. Dirty worktrees (uncommitted, nothing committed from them)

| worktree | state | finding | needs the user? |
|---|---|---|---|
| `/Users/wata/Downloads/MISAKA-testnet` (main checkout, Codex `codex/workspace-rfc02-completion` at `53d9054ba`) | 49 files | live Codex RFC02 WIP: v39 folds, native producer/tensor store, `AGENTS.md`, `PRINCIPLES.md` | no (Codex is active) |
| `~/.codex/worktrees/integration-followup` (`d3b94dc7f`) | 13 | live Codex G14 WIP: TIR public allocation, prepared source | no |
| `~/.codex/worktrees/rfc-completion-integration` (`0cdf53c48`) | 20+ | live Codex WIP: kernel material enrollment V4, claim_compact rewrite to direct checks | no |
| `.claude/worktrees/agent-a682d38f33f32112d` (`integ/rfc0018-dag`) | 8 | live RFC-0018 test WIP | no |
| `.claude/worktrees/agent-af59abf1ce09a6e52` (`integ/validation`) | 2 | live validation fixes (`opv_bootstrap.rs`, `palw_service_public_e2e.rs`) | no |
| `.claude/worktrees/agent-abec340cece24d425` (`g14/merge-codex-501a6a2` at `4747e264b`, 10-11) | 44 (7 UU) | **a half-done merge** of `ada770cbb` into `4747e264b`. Both parents' content is in integ (`ada770cbb` is an ancestor; the merge's own `palw_tokenizer_json_stream_v1.rs` etc. exist) | **yes: safe to `git merge --abort` and remove** |
| `~/.codex/worktrees/rfc2-hf-coverage` (detached `312e3ba50`, 10-03) | 2 | RFC-0002 §II.10–II.12 and RFC-0005 drafts; integ has later II.10–II.12, and RFC-0005 is 不採用 | discard |
| `MISAKA-wt-b/wt-cap-s1` (09-27) | 3 untracked | **`docs/adr/0153-2m-verification-sharding-plan.md` (draft) and `docs/adr/0161-a-claim-is-not-an-emission.md` (Proposed)** are referenced by integ (RFC-0001, ADR-0167, launch checklist) but have no file at integ. ADR-0167 covers 0161's fixed per-DAA budget | **yes: add 0153 (and 0161 as history) or update the references** |
| `MISAKA-wt-b/wt-bigmodels` (09-29) | 3 | `tir_layout.rs` +50 lines and a big-model admission study test | discard, or keep as a study |
| `MISAKA-wt-b/wt-lifecycle` (09-27) | 13 | audit-lifecycle ledger data (~35k lines) and fenceD/waitC scripts | archive or discard |
| `MISAKA-wt-b/wc-lead` (10-10) | 2 untracked | `docs/evidence/hf-onboarding/20261010-qwen25-0.5b-lead{1,2}/` evidence dirs | **yes: commit as evidence or discard** |
| `MISAKA-wt-b/wc-meas` (10-08) | `scratch/` | OPV measurement scratch | discard |
| `MISAKA-wt-b/wc-a2u-int12`, `wt-int12`, `wt-e-fence2`, `wt-e-int8`, `wt-int4-audit2b`, `wt-hbelig`, `wt-cap-*-vprobe`, `wt-cap-escrow-verify`, `wt-d11-rfc1` | 1–4 each | probe tests (`a2u_int12_replay.rs`, `e_probe_*`, `zz_audit2b_*`, `hb_attempt_eligibility_verify.rs`, capacity re-measures) | discard (probes) |
| `MISAKA-wt-b/wt-int3`, `wt-main-1300`, `wt-main-750` | untracked | explorer deploy stage dirs | discard |
| `MISAKA-testnet-main` (`074cc1511`) | `.ibd-check/` | IBD scratch | discard |
| `MISAKA-testnet-main-final` (`codex/palw-cleanup-main-final`, 09-14) | 60 | t11-era Codex removal of DNS finality, VLT, stake bonds and the validator service | SUPERSEDED by RFC-0012 retirement; discard |
| `MISAKA-testnet-main-port` (`codex/palw-cleanup-main`, 09-14) | 8 | the same removal, earlier | discard |
| `~/.codex/worktrees/t11-stale-diagnosis` (09-20) | 1 | `pre_pow_validation.rs` +82 lines, t11 diagnosis | discard |
| 18 August `.claude/worktrees/*` (palw-*, token-*, phase1-*, p0-hotfix, quarantine-fix, practical-leavitt, exciting-raman, palw-adr0068-phase2, palw-adr0073-fp-weight, agent-a94a4a…) | ~717 deletions + 0–11 edits each | the 717 are committed devnet datadirs and logs (`.misaka-vlt-*`, `.palw-rc-relaunch/`) deleted on disk; the few edits are t11-era `palw_v2.rs`, worker and runbook changes superseded by t12 | discard |

## 6. Not merged: needs a user decision or a build-capable port

1. ~~**aux 134/135 collision**~~ resolved 2026-10-15: Codex's tables moved to 137/138 (§3.1).
2. **claim_compact_v1 (in integ) and claim_compact_v4 (Codex) under PRINCIPLES §8.** Their compact proofs are sum-check with a Fiat–Shamir transcript. Codex is converting v1 to "direct probabilistic checks". Rule whether v4 is withdrawn too; that would also free aux 135.
3. **audit/rpc-index:** the node-only accounting index and supply/evidence RPCs. Its RPC ops 230–243 collide. Pick a free range (245–254 or ≥ 256) and port onto today's coinbase/processor. Compile risk is high.
4. **mn/model-declarations:** model names, sources and distributions. Needs rework (drop the torrent and availability gate per ADR-0177) and new numbers (tag 140 and tail 0xE2 are taken; its ADR must be ≥ 0179).
5. **rcore/p0a-held-hybrid:** held-hybrid dissection for Qwen3.6-style recurrent held classes. Confirm the route is still wanted now that TIR/kernel routes exist. It adds a dormant top-level `Params` fence.
6. **r8-work (= origin/rfc8/complete):** G14-B class-horizon retention terms, `bond_budget_close_holding_v1` and two tests. Consensus-visible but dormant. A hand port across `native_final.rs`'s rewrite with signature changes at about 10 call sites; do it in a lane that builds.
7. **rfc3/ref2 and rfc4/ref2:** the independent reference crates `palw-gen-ref2` (~24k lines; integ's `palw_gen_vectors_v1.rs` cites its script) and `palw-improve-ref2` (~16k lines; conflicts with ADR-0175's immutable models). Both are new workspace members that need a build.
8. **Issue #103** (`known_invalid_parents_note`): a diagnostic-only fix. Re-implement without changing `RuleError::MissingParents`'s arity.
9. **evm-bridge-caps** (`93169967c`, fenced per-move/window caps): no ADR and no trace at integ. Decide whether the t12 bridge wants it.
10. **codex/scan-dynamic-panel `2e8914205`:** an off-chain BitTorrent model repository service. ADR-0177 decision.
11. **Bridge audit BR-2 and BR-5** (rcore/bridge-fixes WIP, audit/bridge-0925): two open findings.
    - BR-2: root-invisible storage is accepted at snapshot import. This touches EVM state import, so it needs a fence decision.
    - BR-5: `eth_getLogs` resolves `safe`/`finalized` to the head. RPC only.
    - The branch is labelled "do not merge". Write the fixes in a lane that builds, and re-run the BR-3/4 reproducers at integ first.
12. **The rest of docs/restructure:** spec chapters 00–16, `docs/INDEX.md`, `docs/templates/`, 17 design notes, and the non-0152 archive.
    - Integ's ADR-0152/0154/0155 now link to these chapters, so those links dangle until they arrive.
    - They were written on 09-27, before ADR-0162..0178, so they need a currency pass before import.
13. **MTP (testnet points program):** `mtp/t12` plus C6 `181c12ef1`. Import only if the program is live; otherwise mark these refs ABANDONED.
14. **origin/audit-q6vnqe:** pre-freeze security audit docs with exploit PoCs. Decide whether to publish them.
15. **Dirty worktrees** with user-facing content:
    - `wt-cap-s1`: the ADR-0153 draft and ADR-0161 are cited at integ but missing (ADR-0160 is now ported).
    - `wc-lead`: HF-onboarding evidence.
    - `agent-abec340cece24d425`: the stuck half-merge.

## 7. Compile risks (nothing here was built)

- **Codex pushes are labelled "(unverified)" or "UNCOMPILED".** They add about 60k lines: RFC02 v39, native recovery, the compact V4 fold, TIR acquisition.
- **`wire_generated.rs`** is now the generator's own output again (2026-10-15: the missing shape-review entries for the TIR public owned codecs were added; the pipeline re-runs idempotently). Mismatches between the type graph and the generated scanners still only show at build or test time.
- ~~**Aux 134 arms**~~ resolved by the 137/138 renumbering.
- **`get_palw_settlement_call`:** builds `GetPalwSettlementResponse` with RFC-0018's two new fields. Any gRPC or wRPC converter that destructures the struct exhaustively must include them (RFC-0018's own commit handles its side).
- **`palw_rule_e_prefers_v1`:** now `Result`-returning (Codex) with the RFC-0018 body. Callers come from Codex's side; RFC-0018 added no new caller.
- **adv/c4-e2e tests** (`24df95834`): ten integration test targets written against round-1 kernel seal semantics. `adv_c4_route.rs` and `adv_c4_fuzz_pipeline.rs` probably need seal-flow updates. Any compile failure blocks `cargo test` for that crate.
- **`c4r4_reporter_share_49.rs`:** imports verified by grep. The F-C4R4-16 guard now runs; F-C4R4-18 stays ignored.
- **dnsseeder:** the ported TCP bound adds `tokio::sync::Semaphore` / `tokio::time::timeout` imports beside existing code; its test module is renamed `tcp_tests`.
- **kv-codec port** (`ceef62383`): uses the panel match arm's `court` binding (`PalwCourtParamsV2` is `Copy`) and `palw_configured_producer_class`; both exist at integ.
- **Restored ADR-0152/0154/0155:** docs only. Their links into `docs/spec/palw/00..16` dangle until the restructure chapters are decided (§6 item 12).
- **Network DoS merge:** behaviour change. The p2p transport cap drops from 1 GiB to 320 MiB. The largest legitimate frames found are 4 MiB PALW snapshot chunks and the 256 MiB EVM snapshot; an IBD drill should confirm.
