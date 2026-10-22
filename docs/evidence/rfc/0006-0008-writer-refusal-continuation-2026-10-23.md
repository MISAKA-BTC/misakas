# RFC-0006 / RFC-0008 writer refusal continuation

Base: `6a128eb65775c9cf1a26f975b7028c2d4a8b1187`; branch
`codex/rfc06-12-continuation`.

`3c70b4078` propagates `write_kernel_row` failures from reader/router V2,
selected-source bootstrap, V6 paid bootstrap, and the two material-challenge
writers in `transition_builder.rs`. Previously these five consumers discarded
the `Result`, although the writer can refuse local resource admission before
changing its map or journal.

`b9ba2e757` propagates both the provider receipt-stream checkpoint write and
the material row write from `routed_answer_v2`. It also routes the common
monetary material row writer through `write_kernel_row_encoded_v1`, which
reserves the actual key and canonical row buffers before allocating them.
These changes retain `NativeResource` as a local retryable error rather than
continuing a transition with a missing write. They do not enable a fence.

## Verification boundary

The baseline Rust test started before these writer changes completed:

```text
CARGO_TARGET_DIR=/Users/wata/Downloads/MISAKA-testnet/target cargo test \
  -p kaspa-consensus-core --test rfc0012_native_evidence_fold -- --nocapture
test result: ok. 6 passed; 0 failed; 0 ignored
```

This verifies the earlier tree's fold evidence extraction, retirement retention,
and post-Final conviction retraction. The fixture drives a floor claim and
explicitly relabels its record for part of the evidence-reader exercise; it
does not establish an actual registered-model useful-work campaign. It is
also not a regression run of the new writer changes.

The campaign driver control suite separately passed 14 tests. The new writers'
library check was launched with output at
`/tmp/misaka-rfc06-12-writer-check.log`; its result must be inspected before
claiming compilation success. `git diff --check` succeeds.

Still required: targeted capacity-refusal controls at the outer consumers,
restart/reorg/prune and physical resource acceptance, the full model/source/
adapter/payment requirements, and the 15-gate real-process campaign. No gate
is promoted from UNKNOWN by these checks. Public presets and the unarmed
full-activation height remain unchanged.
