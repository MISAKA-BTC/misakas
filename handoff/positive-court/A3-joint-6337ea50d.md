# A3 — same-candidate validation of accepted A0 base 6337ea50d

**129 required selected tests passed, zero failures**, on exact source commit `6337ea50d7211b3eedcd4c39737555fc1b081723` (tree `fa1944c63e6904792f86190e8b89f139b1b723d3`). Validation branch: `codex/a3-a0-joint-validation`. No source implementation or fixture was changed in this validation branch; the resulting evidence commit can be selected independently.

The candidate includes accepted A2 initial `e2c2d13f0` + protected-prefix fix `56f5332ba`, A0's actual A3 fixture adaptation `6337ea50d`, A1's signed-packet adapter and the A5 phase-3/typed-fence baseline supplied by A0. Accepted A2 recovery source hashes match the upstream `1b8e4e5c9` / `fd9d4c5d7` versions. The applied A3 source fixture SHA256 exactly matches the prepared patch recorded in `a2-status-followup/api-preparation.json`; it was not reconstructed with different semantics.

| Required suite | Passed |
|---|---:|
| Common contract / funding / native rows / namespace / TIR | 29 |
| A1 checker, hardening and signed port + A3 builder | 33 |
| Actual signed Panel transport and A2→A3→A1 source pipeline | 11 |
| Source allocation guard | 1 |
| A2 responder, stored-provider, inference and protected recovery | 51 |
| Default-feature-off legacy G14 pursuit | 4 |

The cold source fixture uses the actual A2 responder/provider fallback, authenticates G14 inventory and prompt material, reconstructs the existing artifact, performs independent Panel replay, creates the original author's signed witness/frames and reaches the actual A1 checker terminal on the same candidate. The Completed duty requery returns zero prepared units and increments only `status_checks` by the canonical requested unit count. Every other meter field, provider read count and retained cache quantity stays unchanged. Cache import still preserves the whole original meter; a foreign-source request consumes zero status lookups. Both private process-limit literals explicitly cap status checks at 64.

The 51 A2 tests include restart counter preservation, forged unknown-tail discounts, corrupted/reordered journal refusal, explicit protected-prefix repair, prepared-byte identity disagreement, live-lease protection and separate-process restart. Their protected store is a fixture capability. **A native case/scope-bound `ProtectedAnchorStoreV2` implementation is still incomplete.** These results do not create native authority, a real UTXO reservation/lock, a payment/fee entitlement, canonical admission, Gate C or consensus credit.

[Validation metadata and hashes](A3-r1-evidence/joint-6337ea50d/validation.json) pin the source tree, each exact Cargo command, exit code, raw log hash, build settings and actual test binaries. All six required commands completed successfully. Raw logs are unedited. Documentation no-new-regressions passed with 43 known issues and zero new occurrences; Rust formatting, CI shell syntax and source diff checks passed.

An additional optional core typed-fence unit-test build was stopped after it triggered a full RocksDB rebuild. Its command, signal exit and raw build log are retained under `optional_checks`; no typed-fence runtime test is counted or claimed. The fixed source's public presets retain the absent Positive fence, and this evidence-only validation changes no activation setting. Full native/multi-node acceptance and real protected-anchor installation remain separate A0/A4/A5/A6/A7 work.
