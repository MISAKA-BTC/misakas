# Initial seven-model target

The user selected six dense tiers and one additional hybrid MoE on 2026-10-15.
`initial-seven-models-v1.json` pins the dated publisher revisions/file metadata.
No weights were fetched and no conversion or execution was run to create it.
Each model currently remains `NOT_RUN`; null runtime/artifact/context fields are
deliberate unresolved inputs, not permission to use a smaller model or context.

Use `misaka/palw/sdk/tools/hf_census/initial_model_launch_v1.py` in the consolidated
validation phase, after completing the implementation and committing the exact
target, seven-member engineering cohort and run plan. It runs the existing full
checkpoint runner with two independently replayed native audits for each chain
stage. A fresh run directory is required. Every model must pass every stage,
including Final and accepted redemption, and the sustained two-window audit.
The original source components/context and exact canonical inventory, PC, program,
native class and source must match the precommitted target. Missing complete-model
identity in the proof-derived native export fails; a single component's Final
cannot prove an entire multimodal pipeline.

```text
python3 misaka/palw/sdk/tools/hf_census/initial_model_launch_v1.py \
  --target docs/design/palw/tir/launch/initial-seven-models-v1.json \
  --committed-target-sha256 <premeasurement SHA256> \
  --cohort <committed seven-member cohort.json> \
  --plan <committed run-plan.json> \
  --committed-plan-sha256 <premeasurement SHA256> \
  --out <fresh measurement directory>
```

This tool prepares release evidence and never changes consensus presets, grants
Active/Final rights, or mints a founding bond. Enabling the target ruleset remains
dependent on the actual native evidence and activation artifact. The launch set
does not replace the A6/A8 cohorts or their original failure denominators.

The receipt checker now also has an explicit `stream-v2` pointer input: one
`{"schema":"misaka.rfc02.receipt-state-stream.v2"}` header followed by JSONL
`{child, selected_parent, collected}` records. Each pointer line is bounded before
allocation. The SDK accumulator verifies every consecutive state and commits all
of them to a rolling transcript root; it retains only the previous checked state
and the two windows' checked endpoints. It never samples or shortens the frozen
windows to fit the old 65,536-point/whole-transcript memory ceiling. Output schema
`misaka.rfc02.proof-derived-receipt-windows.v2` carries the full point count/root
and bounded cumulative checkpoints. The legacy array input remains separately
bounded. Both paths are unverified source until the consolidated phase.

Still required: exact runtime variants (especially the multiple distinct 35B
artifacts), original metadata/context and local weight/artifact pins; full Pixtral
vision/projector and hybrid MoE conversion; proof-derived complete-model identity
export for native32; the implementation's consolidated checks; and all seven
real-checkpoint trials under the target ruleset. Public Hub file metadata alone
establishes none of those results.

Large-model boundaries remain explicit: the original 1540-pixel Pixtral input,
head-sharded attention, complete projected output and old Source28/native input
transport do not all fit the existing v1/v28 limits. The implementation must add
an independently priced versioned contract and execution path for those shapes;
listing the vision adapter or increasing only a readonly auditor's buffer does
not complete that native path. The present local host has 32 GiB physical memory;
the previously authorized build VPS has an 8 GiB service limit. Full seven-model
runtime measurements still require the exact weight locations and suitable
independent verification environments.
