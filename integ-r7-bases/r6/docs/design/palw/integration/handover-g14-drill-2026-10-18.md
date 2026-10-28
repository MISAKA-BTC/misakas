# Handover — G14 drill lane (integ/g14-drill), 2026-10-18

## Gap status

| Gap | Fix | State |
|---|---|---|
| G-D02a: no kind-3 filing after a legacy public prosecution | `kaspad/src/palw_fraud_filer.rs`. When the fraud filer builds a terminal, it keeps the terminal's refutation (`book.terminal_proofs`). Each tick it reads the claim's phase. Once the claim is `Voided{CourtFraud}`, it hands the proof to the false-Valid filer (`note_step_refutation_v1`). If the filer refuses the refutation as too large, it hands over the chain's `CourtFraud{voided_daa}` contradiction instead. A hook at the engine's `Done` was useless, because a voided claim leaves the candidate list before its case reaches `Done`. | **Not yet shown live.** Runs 8 and 10 used the earlier code: the producer was convicted, but the 3 seats were not slashed and no "P2-8c" line was logged. Run 12 (below) is the first run of the watcher. Look for the log line `G14 G-D02a` and for `bondSlashed > 0` on cards 1–3. If the line shows `false`, debug `note_proof_v1` / `palw_false_valid_scan_v1`. |
| G-D02b: a reservation lost in a reorg is never refiled | `carried` and `reorged_out_v1`: an item that was carried but is still requested 4 or more DAA later is resent immediately, outside the retry budget. Unit test `g_d02b_tests`. | Shown live in run 8 (7 resends across fork-id flag days). The fold-lag false positive seen in run 10 is fixed by the 4-DAA margin. |
| G-D02c: one bond cannot keep up with a lie-every-draw producer | Focus on one claim per producer. The producer's other claims are deferred until they are licensed and their Final deadline is within 60 DAA. Unit test included. | Shown live in run 8 (lie on every draw from DAA 4): 1 claim `court_fraud`, 92 `aggregate_forfeit`, 0 lying claims reached Final, producer fully slashed. |
| F: unpaid but not prosecutable | `palw_unpaid_reward_retirement_floor_v1` (`consensus/core/src/palw_state_v2.rs`, under the dormant G-15 fence `palw_reward_liability_horizon_v1`). An unpaid Final claim is not retired; the sweep and the DL-1 derivation share one function. | `g14_accept_f_under_g15_*` passes. lg14a suite: 25/25, including the fingerprint pin. |

Other changes in this period:
- Merged origin/pre (`c68d385d0`) and `integ/g14-court-capacity-r2`.
- `scripts/drills/rfc0018_dag_drill.py`: the panel checks are now mandatory (RFC18 item 1). Not run yet.
- Driver options added: `--lie-claims`, `--lie-from-daa`, `--skip-d02`. Peers are dialled in one direction only. Each injection uses its own producer card. Only claims accepted after the lying restart count as lies.

The draft §12.7 text is `/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet--claude-worktrees-14-lane-restart-builds-c00813/7adc9332-ba5a-4def-b8fd-2d73c009e942/scratchpad/g14drill/sec12_7.md`. It has not been appended to `docs/audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md` yet. Append it once run 12 has a result.

## Running now (do not kill)

- **Driver**: PID 84986.
  - Command: `python3 scripts/drills/g14_collusion_drill.py --mode genesis --lie-claims 1 --lie-from-daa 4 --no-d11 --no-d16 --work /private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet--claude-worktrees-14-lane-restart-builds-c00813/7adc9332-ba5a-4def-b8fd-2d73c009e942/scratchpad/g14drill/run12 --out /private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet--claude-worktrees-14-lane-restart-builds-c00813/7adc9332-ba5a-4def-b8fd-2d73c009e942/scratchpad/g14drill/run12.json`
  - Log: `/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet--claude-worktrees-14-lane-restart-builds-c00813/7adc9332-ba5a-4def-b8fd-2d73c009e942/scratchpad/g14drill/run12.log`
- **Nodes**: PIDs 89890 and 91014 (P = producer + 3 colluding seats, O = outsider).
  - Logs: `/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet--claude-worktrees-14-lane-restart-builds-c00813/7adc9332-ba5a-4def-b8fd-2d73c009e942/scratchpad/g14drill/run12/{P,O}/node.log`
  - JSON RPC ports: P 34103, O 34173 (`scripts/drills/misaka_wrpc_json.py`).
- **Reading the result**:
  - The D-02 row is in `run12.json` under `rows[]`.
  - `grep 'G-D02a\|P2-8c' /private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet--claude-worktrees-14-lane-restart-builds-c00813/7adc9332-ba5a-4def-b8fd-2d73c009e942/scratchpad/g14drill/run12/O/node.log`
  - `getPalwClaims` for the seat bonds (cards 1–3 in `/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet--claude-worktrees-14-lane-restart-builds-c00813/7adc9332-ba5a-4def-b8fd-2d73c009e942/scratchpad/g14drill/run12/keyring/manifest.json`) shows `bondSlashed`.
- **Pace**: the chain advances about 0.3 DAA per minute, so D-02 takes about 3–4 hours.

## Not run / remaining

- Re-run sparse and skip-all. skip-all only reaches its DA default at DAA 1,250, about 57 hours at the current pace.
- RFC18 multi-model acceptance (a–f), including the RFC-0023 staged measurements (per-claim Final, Gate C, hold release, payout `settled`, lag by cause, the hand-over attack).
  - Blocker: only the floor class is servable on this host. The other genesis classes need artifacts that are not here, and downloading them needs user approval.
- The final release phase, after the coordinator sends "release start" (see the coordinator's earlier message).

Build rules: `CARGO_INCREMENTAL=0`, `-j 3`, keep only one target profile (currently release), at most 3 nodes at a time.
