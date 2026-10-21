# Handover — G14 TIR frame / genesis lane (2026-10-18)

Branch `integ/g14-tir-pricing-r2`, worktree `.claude/worktrees/agent-abd1b9bfd824ba6a9`.

## Running (do not kill)
- Genesis #2 long fit: pipeline PID 71987 (`python3 scripts/genesis/genesis10_pipeline.py run --model 2 --steps download,fit --redo --no-upload --ram-accounting`), palw-tir-fidelity PID 72982. Log: `/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet--claude-worktrees-14-lane-restart-builds-c00813/7adc9332-ba5a-4def-b8fd-2d73c009e942/scratchpad/g2b.log`. Started ~02:40 JST 10-18; integer pass ~19.7K/32,768 at 09:00. It started before the upload gate change, but runs with --no-upload.
- Check: `tail /private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet--claude-worktrees-14-lane-restart-builds-c00813/7adc9332-ba5a-4def-b8fd-2d73c009e942/scratchpad/g2b.log` (line "#2 long fit: ... -> within|OUT OF TOLERANCE"); report `~/Downloads/genesis-models/work/02-huihui-qwen35-2b/fit-long-report.json`; state `~/Downloads/genesis-models/work/state/02-huihui-qwen35-2b.json`.
- If within: `python3 scripts/genesis/genesis10_pipeline.py run --model 2 --steps upload,verify,cleanup`, then record root/sha/HF URL in genesis8-status.md. Short fit: KL 0.0027, top-1 0.977. If out of tolerance: keep the already uploaded segment 00000.palwseg (user 10-18) and mark the README "unverified".

## Genesis queue
#1 (re-convert A16 at 32K; run `target/release/qwen25-window-compare` for RFC13) → #3 → #4 → #5, one at a time (disk ≥25 GB free after download). Blocked on this 32 GB Mac: #6 (integer program 29.5 GB), #8 (≥64 GB), #10 (70B). The pipeline now stops when a fit cannot be measured (upload_refusal).

## Remaining code
- T-N1 node: show a quantum PAYMENT (more quanta or a deterministic win in the harness config) and a colluding-seat SLASH (licence must land before the conviction) — kaspad/src/palw_service_tir_frame_e2e.rs.
- 02-B: node run with a non-default ClassSemanticsDeclaredV1 (needs tokenizer table + rendering rule).
- RFC20: run scripts/genesis/rfc20_acceptance.py on #2 (8K first; 32K ≈ 6 h here). RFC13 PALWQ361 (#8) comparator not written.
- Unrun: consensus t12_a2u_new_kinds_uniform after the tag-175 fence.
