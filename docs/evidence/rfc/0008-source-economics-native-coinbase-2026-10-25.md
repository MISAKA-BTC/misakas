# RFC-0008: selected-source fees and principal on the native coinbase path

## Status

The exact native pipeline regression **passed 1/1** in 115.64s after its corrected executable built in 6m27s (including coordinated target-lock wait). Log: `/tmp/misaka-rfc-source-economics-coinbase-pipeline-tests-retry3-20261025.log`. It reached actual signed source funding/selected approval, native settlement, fee and unused-principal coinbase acceptance, consumed-prefix acknowledgement, pending/final restart, native replay and a heavier-branch reorg. This is mechanism evidence for the explicit private fixture; no campaign acceptance is claimed. Public activation remains `None`, and normal startup still rejects selected-source bootstrap and verifier economics.

Test binary: `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspa_consensus-807ee71a89c08594`,
SHA-256 `7c66120688312aa652076516e095095fba2dcafafe205635534ba2b2683d3995`.
Subsequent source changes before saving this result only clarified two comments;
they changed no executable behavior. The digest identifies a local artifact,
not K3 or campaign acceptance.

## Initial runtime defect and correction

The exact pipeline test initially failed with local `HistoryUnavailable` at the source freeze boundary. The diagnostic rerun recorded matching accepted sink/state hashes, a readable parent header, stable pruning-import status, and `own_next=Ok(Some((30, None)))` immediately before the same refusal. Thus the actual own-history resolver could authenticate the epoch; the pre-object fold had received plain fee extras with both source context fields absent.

`palw_accept.rs` now attaches source-only context to fee extras against the original accepted parent before pre-object preparation. `source_bootstrap_v1.rs` factors the exact existing object resolver into one shared attachment helper. Rehearsals retain that context and its typed fault rather than resolving synthetic state points. Ordinary model/class court extras retain their prior behavior, and capacity faults retain their local resource classification. The temporary diagnostic probe was removed before the corrected run; the final fixture only follows the native accepted-chain path.

Baseline logs: `/tmp/misaka-rfc-source-economics-coinbase-pipeline-tests-20261025.log` and `/tmp/misaka-rfc-source-economics-coinbase-pipeline-tests-retry1-20261025.log` (diagnostic build 10m28s; exact test failed in 0.95s). These are failure evidence, not acceptance evidence.

The corrected executable built in 4m10s and accepted the actual signed funding/approvals through native settlement and DAA 600. It failed after 38.50s because the fixture expected one heartbeat to exceed the inclusive horizon. Consecutive native blocks can share a DAA; the fixture now asserts that each further block at the same DAA remains unclosed with zero fee/refund allocation, then waits for the first actual accepted block with DAA strictly greater than the horizon. The production release rule was retained. Log: `/tmp/misaka-rfc-source-economics-coinbase-pipeline-tests-retry2-20261025.log`.

## Verified path

`consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/source_verifier_economics_v2.rs` extends the existing real-pipeline `Net`/`World`/`T12Chain` harness. Two distinct small native OPV program claims are registered, posted, sealed and committed through signed carriers. The source policy uses the processor's real own selected-chain history to freeze the stake population and fix its anchor. The work statement comes from the native source getter.

The first source receives real signed `Open`, `FundVerifierEconomicsV2` and drawn `ApproveWithLiabilityV2` carriers through mempool validation, the node's own templates and the persisted block fold. A fresh deterministic verifier checks the fixture's public material before each selected signature; a matching replay supplies no credit by itself. The second real source is explicitly funded and receives no approvals. It supplies the unused principal refund control. Both funding debits are checked against actual native bond collateral; R is not money.

The test waits for the source's actual native settlement and the inclusive liability horizon. It observes the fee and principal queues created by those contracts rather than importing or pre-filling payout rows. A same-database node restart must retain the exact pending state and UTXO set. The next template's actual coinbase contained the exact source allocations, and the accepted block was `StatusUTXOValid`. The native consumed-prefix journal must record the accepted block hash, DAA and fee amount once. A following block made the coinbase outputs visible in the accepted UTXO set. This does not assert coinbase maturity or immediate spendability.

An independent native replay reaches the same earned queue, then submits a copy of the payment block with one source output changed by one sompi and the transaction/merkle/header hashes recomputed. Full coinbase comparison must disqualify that block without acknowledging the pending source receipts. A longer competing branch is then built from the earned queue, accepted by the first node, and must rewind the old acknowledgement and install the new branch's acknowledgement. A final restart and genesis replay compare persisted roots and UTXOs.

All financial assertions count the complete virtual coinbase UTXO set, including any retained side-branch outputs. Counting only selected-chain coinbase IDs could conceal duplicate outputs from a competing branch.

## Limits

This is an explicit private mechanism fixture over the existing small Wide128 program and harness keys. It retains the existing test-only artifact-attestation and OPV-eligibility seams, skips PoW difficulty, and uses the harness's simulated clock with its EVM lane inert. It is not K3 or independent production deployment. Its finite policies are unapproved test inputs and are not public defaults. No production worker deployment or independent source acceptance is established by this harness. The 15-gate campaign, signed operational inputs, real multi-process work, OS-crash and pruned-IBD acceptance, source independence, and broad RFC-0008 reward/material coverage remain separate outstanding work. Unknown gates remain unknown.
