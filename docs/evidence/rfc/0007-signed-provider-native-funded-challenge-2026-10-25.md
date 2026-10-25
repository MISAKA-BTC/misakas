# RFC07/08 — signed provider response to real native funded challenge

Status: bounded private multi-process control **PASS**, 2026-10-25. This closes the previously unexecuted signed-socket → actual daemon checker → funded native ChallengeCurrentV4 seam for this fixture. It does not close RFC07/08 or the RFC12 campaign.

## Actual path executed

The consensus writer registers the private paid class/job and commits the same typed-material claim already admitted through the real funded TypedKernelV4 enrollment. It carries the SDK-built paid Fund and Lock objects through actual native transactions. Fund persists the frozen source target required by the native source reader; no source row is synthesized. The writer closes its database completely before the checker opens it.

A separately pinned kaspad test executable opens that database, reconstructs the native route and obtains the query authority from the actual ConsensusApi. A third process serves the exact query on loopback TCP. Its genuine ML-DSA signature, sender/recipient identities, domain, echo and consent all pass the production authenticated transport. The signed answer intentionally contains malformed material bytes `[0]`, so the actual SDK decoder returns a located InvalidMaterial error at **stage 0, position 2**. This is a Byzantine fixture response; no production validation bypass is added.

The same checker then runs the real `PrivateKernelPaidCheckSpecV4::read().prepare().run()` using a real ConsensusManager, FlowContext and MiningManager. It reacquires native query authority, fetches the signed response again, selects an availability challenge, signs it and publishes the production object/transaction WAL. The fixture does not copy the production binder, action selection, signing or funding implementation.

The writer reopens its original database and verifies the exact WAL object and transaction. Native rehearsal accepts the object; the actual mempool accepts the transaction; the actual block pipeline applies it. The checker funding UTXO is spent, its authorized change UTXO is created, and the exact fee is **289,712 sompi**, within the explicit 100,000,000-sompi fixture cap. The material challenge checkpoint has sequence 1 and survives a further full database close/reopen unchanged. Rehearsal/WAL publication alone did not install that checkpoint.

The result is an answer obligation, **not** an immediate computation conviction, slash, receipt, selected-verifier approval or consensus credit. The test asserts that no approved paid Final exists throughout this acceptance path.

## Inputs and limits

The fixture uses explicit private unapproved policies, a deterministic private bootstrap seed, existing mechanics-only class eligibility and skip-PoW harness keys/premine. The paid policy source delay is explicitly eight slots to carry the real Fund before Lock. Both processes derive the same configuration from one test-only shared fixture. Public policy validators still reject these unapproved settings.

Distinct writer/checker/provider process IDs are asserted. Deterministic fixture keys and multiple processes do not demonstrate independent sources, an honest validator fraction or selected approval. The provider answers exactly two requests: one SDK classification control and one production checker acquisition. This does not test every stage/adapter, all rewards/coinbase, provider replacement or the complete restart/reorg/prune lifecycle.

The production private-path guards remain active. The test canonicalizes its own temporary directory and creates its deterministic fixture seed with mode 0600. It does not relax symlink or key-permission checks. A provider-child owner kills/reaps the fixture on checker failure; accept/read/write and parent runtime waits are bounded.

## Verification

Six-package build: **PASS**, 46.15 seconds; exact command/source identities are in the [durable evidence index](0007-signed-provider-native-funded-challenge-2026-10-25.json). Six executable SHA256 identities were pinned before subsequent source edits. Explicit execution used `--ignored --exact` because this cross-crate parent requires a matching pinned kaspad executable. The executed parent result is **1 PASS, 0 failed, 0 ignored**, 9.32 seconds (controller wall 9.43 seconds). Nested child tests are implementation of that one parent and are not counted as additional acceptance tests.

Exact parent filter: `pipeline::virtual_processor::tests::g14_kernel_route_e2e::material_typed_ingress_v4::signed_provider::rfc07_signed_socket_unit_failure_paid_checker_native_funded_challenge`.

Earlier unsuccessful generations remain in the evidence index: non-Send fixture future, missing mechanics eligibility, missing native frozen Fund target, symlinked temporary path, and nonprivate seed file. They were repaired in the fixture/native input construction, without weakening production authorization. No failing generation is counted as acceptance.

Full activation stays `None`; ordinary selected-source/V6 bootstrap refusal and dormant unapproved public economics are unchanged. K3 inputs and the native39 settlement mapping remain absent; all unexecuted campaign gates remain UNKNOWN.

Related controls on the same pinned generation: existing daemon checker tests **19 PASS** with `TMPDIR=/private/tmp`, and the original typed-provider native publication/cold-restore parent **1 PASS**. Its nested writer/reader controls are not extra acceptance counts. An initial checker-group run inherited the symlinked macOS default temp path and returned 8 PASS/11 FAIL at the existing outbox path guard; the unmodified executable passed all 19 after the explicit canonical temp environment was restored. Both runs are embedded in the evidence index. Thus 21 distinct parents passed in this provider scope, without summing failed or child runs.
