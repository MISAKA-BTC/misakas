# 82bb43a focused native registration regression

Status: **FAILED; native registration qualification remains incomplete**.

Source `82bb43a383ce3d5160d588ff2e775ed039713ce9`, tree
`0b3d74ee8db5727a56083dd5edcf73edaf7f8dcd`. This run builds the Node library
test target and executes one exact isolated-origin registration fixture on the
ordinary libtest thread with its default stack. It executes no retained LLM
case, OS-main substitute, inference, conversion, or public activation.

The build passed in 42.270 seconds. The actual Core build reported
`misaka_source_local_audited_rust193_v1` and
`MISAKA_SOURCE_LOCAL_BUILD_PROFILE_V1=rust-1.93.0-borsh-1.5.1-v1`.
The exact test aborted with stack overflow, exit `-6`, before producing a
libtest result summary. Passing cases: **zero**. HF cases executed: **zero**.
The controller confirmed stable source, immutable inputs, and binary bytes;
all owned process groups ended without live members.

The raw controller record is [native-stack-focused-82bb4-20261102.json](native-stack-focused-82bb4-20261102.json),
SHA256 `40f0cbb537ceeb4ca17232552afe93551fe660b7bf92bbadf5ba7b96fcb5387f`.
Do not retry this record prefix or combine this failed source with older
passing matrices to claim a complete same-source run.

The failed executable has 429802896 bytes, SHA256
`e2d98475d5f4d9222edfe40c793e8cd9e31921dbf167ee890c5fd56938c7e67d`,
Mach-O UUID `0d6530ea-1710-3778-9590-6ab2fef3a90a`. It is preserved in
`/Users/wata/Downloads/MISAKA-testnet/target/history-native-focused-82bb4-failure-preserved-20261102/`.
Its gzip archive was actually restored to a temporary file and checked for
identical size, SHA256, executable mode, and UUID before that temporary file
was removed. The original Cargo alias was retained. Preservation manifest
SHA256: `e0c8b9838d3d73f83564f7f644c3ded6e5fb5e5f68dadec89fb7b9d4f01797a1`.

The matching macOS crash report identifies PID86902 and the same UUID. The
first distinct application unwind frame is `palw_v2_params_on_base+640`,
immediately after a branch to `palw_fp_bundle_with_windows_and_floor_v3`.
Its parents are the existing public-parameter assembly and the fixture's
configuration helpers. Neither the boxed async case nor Tokio Runtime is
active in that chain. This localizes the next investigation to configuration
assembly; the signal unwind alone does not establish which instruction first
touched the stack guard or whether the callee body was entered.

The separate earlier `3a550341` Core54/Exec71/SDK19 and OS-main1 successes
remain valid only for their recorded source and scope. The original-context
trained cohort (SmolLM2-360M 8192, SmolLM2-135M 8192, Pythia-70M 2048) still
requires actual native registration execution.
