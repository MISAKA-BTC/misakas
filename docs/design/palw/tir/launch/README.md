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

Still required: exact runtime variants (especially the multiple distinct 35B
artifacts), original metadata/context and local weight/artifact pins; full Pixtral
vision/projector and hybrid MoE conversion; proof-derived complete-model identity
export for native32; the implementation's consolidated checks; and all seven
real-checkpoint trials under the target ruleset. Public Hub file metadata alone
establishes none of those results.
