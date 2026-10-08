# HF onboarding — closed-loop matrix (H1)

Owner: H1 (HF Onboarding Closed-Loop). Branch `onboard/h1-hf-closed-loop` (base `89ffb1fb7`, plus the cherry-picked fixes named
below). Every cell is a measurement on a private devnet with a pinned checkpoint, or it says why it is not. **Separate columns** keep
apart what must never be read as one another: header/shape-ready, text-only scope, short context, synthetic-beacon conformance,
dormant registration (Candidate), and full-task Active/Final.

Evidence per run: `docs/evidence/hf-onboarding/<run-id>/{model,environment,preflight,conversion,pack-verification,registration,
consensus-state,g14-gates,failures}.json`, `reproduction.md`, `summary.md`. Runner: `tests/hf-onboarding/` (see §5).

## 1. Levels

| level | what it takes here |
|---|---|
| L0 source | the pinned revision readable on the Hub (metadata only), and a local checkpoint's every required file matching it (LFS sha256 / git blob id) |
| L1 shape | `misaka model preflight hf://REPO@REV --depth shape --max-context N`: convert **and** register ok, from headers only |
| L2 artifact | full checkpoint → calibration → `palw-class pack build --declare` → `pack verify --strict --rebuild --model` on the **declared** class file: VERIFIED, nothing failed, nothing skipped (HF reference fit included) |
| L3 registered | client U's own signed `ClassRegisteredTirV1` folded on the devnet (U: own key, own funds, own bond, no node, no seat; RPC only) |
| L4 replayed | A, B, C agree on the class row, the registry row and the `classes` state proof at one pinned block; a node restarted over its DB agrees; a fresh node by IBD agrees; U proves the registration against the pin; a 1-bit-modified artifact is refused by `pack verify` and by the registration's pack gate |
| L5 eligible | conformance + DA + G14 hold **on the chain** — this build has no chain state for any of the three (G14_INCOMPLETE; on-chain conformance waits for OB-P0) |
| L6 useful work | a real claim executes, is verified, goes Final, reward attributed — only once L5 exists |

`SYNTHETIC_BEACON_CONFORMANCE_PASS` (lane C's commit → synthetic facts → run → fresh verify) is a separate column: it is never on-chain
conformance and never L5.

## 2. Devnet

Salted testnet-12 drill (ADR-0152 §8.2), loopback only, its own genesis and keyring written by the binary under test. The shipped
release's flag days compressed exactly like the combined drill (`audit-combined/dc.sh`): `--palw-drill-fence-at 6 --fence2-at 10
--fence3-at 14 --tir-at 16 --tir2-at 24 --int11-at 26` — kaspad's own log lists every fence as MOVED from its shipped height
(750/1000/1300/1700/2000/3600/5300/5395/5490/5585), none ARMED from dormant. `palw_bond_maturity` (1,000) has no drill flag and stays at
1,000 (the devnet runs below it). Nodes: A (seat 0, floor producer + heartbeat clock), B (seat 1, heartbeat, U's RPC), C (keyless
observer), D1–D6 (seats 2–7); transient: the bond registrar, a fresh IBD node Z. `--ram-scale 0.3`, 2 GiB host share per node.

Run r1: integration `89ffb1fb7` (kaspad sha256 `6e9b1936…`, misaka `211e708d…`; palw-class rebuilt at `be5808434` = `89ffb1fb7` +
`5048b7880` (calibrated_context) + `fa13939b9` (offline gate height), sha256 `e789d9c4…`), genesis `76a7e10d…`, params `f2da0947…`,
Apple M1 Max 32 GiB, macOS, shared with other agents (load 30–370).

## 3. Model matrix

<!-- filled from the evidence at the end of the run -->

## 4. Failures routed

<!-- filled from failures.json -->

## 5. Runner

```bash
bash tests/hf-onboarding/devnet.sh up            # salted t12 drill, the release's fences compressed
bash tests/hf-onboarding/devnet.sh user          # U: key, one funding send, own bond (transient registrar — a WORKAROUND, not node-less)
RUN=<id> bash tests/hf-onboarding/onboard.sh source|preflight|artifact|conformance|register|observe|reregister|lifecycle|reorg-register|summary <model-id>
```

Stages run from a snapshot of the runner (an edit never reaches a stage in flight). The artifact cache is keyed by (revision, palw-class
sha256, context, calibration spec, build options); `CACHE=0` builds into a fresh directory (the clean-source run).
