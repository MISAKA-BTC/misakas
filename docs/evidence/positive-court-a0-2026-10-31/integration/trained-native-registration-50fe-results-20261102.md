# Same-source native registration closure — 50fe

Source `50fe43e4c9ded609b1be9ab51125caa33ffffd8b`, tree
`fe664df213a7f67b2fbe25bf95880f60e027769c`: **19 distinct small Node tests
and three retained trained-LLM registration cases passed** with one compiled
library-test executable on the default libtest stack. The separate focused
tiny test repeats one of the 19 and is not counted again.

| Model | Original maximum context | Native registration, replay, same-DB restore | Case seconds | Sampled owned-PG RSS maximum |
| --- | ---: | --- | ---: | ---: |
| HuggingFaceTB/SmolLM2-360M | 8192 | PASS | 43.243 | 542687232 bytes |
| HuggingFaceTB/SmolLM2-135M | 8192 | PASS | 23.219 | 286490624 bytes |
| EleutherAI/pythia-70m | 2048 | PASS, i64 Q24 final output | 25.765 | 209928192 bytes |

All model cases preserved the pinned class, program, layout, artifact root and
tokenizer identities. Each checked the actual funded registration carrier and
signature, production relay fee and standard MiningManager/template route,
accepting-block native fold, full schema-1 row and origin aux141, independent
second-node three-block replay, and reopening the same database. Every stage
passed and every exact invocation reported one executed success.

Small-suite closure includes invalid signatures/bonds/targets, both court
versions, the one-registration-per-block limit, actual reorg/re-signing and
UTXO/bond rollback, same-DB recovery, and the 14 native journal tests. The reorg
fixture now derives fork length from effective K and merge depth and requires
the old carrier to be absent from virtual acceptance before asserting coin
restoration. PALW selected-parent rollback alone does not imply virtual UTXO
rollback: valid red-block transactions also participate in virtual acceptance.
No production DAG rule or parameter was changed for this fixture.

The production fix initializes an absent route header through the existing
policy-checked, resource-admitted journal writer before the first origin row
is written at genesis. Other changes in this phase are private test ownership,
initialization and continuation boundaries; they preserve the values,
signatures, checks and current-thread runtime.

The executable has 429673920 bytes and SHA256
`5e942b83cb35ffcf8878c5c1ac6ddb37fd2e24406503c235cf40cc95a2ce37e7`.
Actual Cargo JSON verified the audited Rust 1.93/Borsh Source profile, package
Core/Consensus optimization 0/debug info 0, and the pinned native link archives. Source,
19 static native/historical-binary inputs and executable bytes stayed stable;
each owned process group was closed before its raw record was finalized.

- [SMALL19 record](native-node-runtime-only-50fe-small-20261102.json), SHA256 `d4ff6b1a561bed851ac180e6a3d1ad95f8c0f17ac6f9ab612d7b9a94b81ec88c`.
- [Trained HF3 record](native-node-runtime-only-50fe-hf-20261102.json), SHA256 `a73b7bc123124d68e2190c7f68c58618a6bb287364f4e2fb94966fca48637283`.
- [Focused prerequisite](native-stack-focused-50fe-20261102.json), SHA256 `84c2c1b8d4216b50fc77de41929313615e17761ac61a15441efddd3af5541066`.

The separate same-source [Core regression record](core54-focused-50fe-20261102.json)
passed 54 distinct outer tests across 14 selectors, including the old state-root
golden and legacy wide refusals. Its nested Source-facade child is not added.
The record SHA256 is
`b575b1cdde0c100e90a460f2a10d18514c09a35176fd65cfd20229385204f791`.

The [strict library lint attempt](strict-clippy3-library-50fe-20261102.json)
passed Core, then stopped on two `manual_is_multiple_of` diagnostics in the
executor's bounded row reader. SDK was not run. This is an incomplete lint
matrix, not a failed registration or a complete lint pass; the equivalent
style fix and its checks belong to the next source candidate.

Both successful executables were losslessly compressed and actually restored
into fresh temporary files to verify bytes, mode and UUID. The originals were
retained. [Control and audit snapshots](control-snapshots-20261102/index.json)
preserve the exact drivers, plans, independent audits and archive manifests;
their original absolute paths and content are intentionally unchanged.

Qualification is the private isolated court2 registration path and the stated
Node regressions. This does not establish new/full-context inference, an
independent Hugging Face numerical reference match, claim certification,
public activation, physical hard RSS enforcement, or completion of native
Positive Court authority/funding integration. RSS figures are samples, not
certified memory bounds. Earlier failures and different-source successes stay
preserved; old Core54/Exec71/SDK19/OS-main1 results are not added here to claim
a whole same-source matrix.
