# Public constrained-job replay — 2026-10-11

Full G14 remains unfinished. ADR-0177 D7 continues to assume a verifier holding the correct registered model, including the applicable tokenizer/config. No production fence is activated by this change.

## Fixed execution path

The public fraud filer's independent FP execution previously called `execute_free_prompt` without opening the constraint mask scope. The ordinary decode pipeline refuses a constrained job without this scope, even when the host holds the matching public token table. The interval verifier and producer already establish their own scopes.

The filer now derives its mask from the authenticated job descriptor and its own backend's held token table, using the existing host checks for the committed table root and tokenizer ID. It scopes only its own execution and restores the previous thread state on exit. Missing or mismatched tables return a host error rather than any fraud judgement. Other FP jobs receive the existing unconstrained execution path.

The regression test independently runs a constrained job that permits only lane zero, compares all three roots against the filer's replay, checks scope restoration, and refuses an unavailable table or a held table under another tokenizer. A bare backend call without its mask is explicitly refused.

## Verified real-service coverage

Two new tests use an ordinary registered BASE-0 FP class, real PQ job/funding signatures, ordinary mempool validation and block templates, all Panel members signing Valid, and a fresh database importing the public block history. The original accepted FP body and acceptance rows are then removed; no saved job metadata is supplied to the verifier.

The version-6 job contains a valid second-form DFA with 169 nodes and 128 byte edges per node. Its descriptor exceeds the single-carrier limit; the complete original transaction must fit the preset mass limit and exceed 99% of it. The test arms only the existing constraint/decode prerequisites before validation. The public-prosecution fences remain harness-only, and the shipping activation validator must still refuse that fixture configuration.

The producer builds its own table instance. The outsider independently builds and loads the fixture's public tokenizer file through the normal file decoder/registry. Its run receives neither the producer's table Arc nor its execution capture. The fixture's table is synthetic public test data; this is not an audit of every registered model's tokenizer distribution or derivation.

The answered case supplies the correct descriptor only through a signed manifest and two bounded part carriers, reacquires authenticated input and requires the actual service replay worker to record Honest before releasing its reservation. A smaller wrong direct descriptor must be refused first. The withheld case provides the authenticated binding but withholds the job descriptor, and must reach the existing objective DA default, producer collateral loss and reporter accounting through real service carriers.

The first service trial used a 165-node job (109,711-byte descriptor, 488,864/500,000 transient mass) and failed because the test attempted to replace an immutable signed multipart declaration after sending a wrong descriptor. The protocol correctly retains that failed assembly until its existing deadline; the test now checks wrong-descriptor refusal through direct carriage. That failed trial is preserved as evidence and counted as no successful verification. Final tests use the larger 169-node fixture.

## Verification

29 unique tests passed in this round: 27 filer/input regressions including the new mask test, and both new actual-service tests. The combined service run passed in 625.71 s. The job descriptor was 112,291 bytes and the ordinary transaction's transient mass was 499,184/500,000 (99.8368%). Each fresh database validated 2,065 public blocks before its original FP history was removed.

The answered service case restored the descriptor from two parts, reacquired input, independently replayed all three roots, recorded Honest in the real worker, and released at DAA 1163. The withheld case's existing session deadline was DAA 2299; it reached objective DA default at DAA 2307 after five mined service batches. The test directly checks the producer's consumed DaDefault sanction has positive amount and the public verifier's live reservation has ended. Both service tasks joined and their callback owners were released.

Normal production library check and the final node test build passed. There are no consensus-source, shipping-preset or wire-layout changes in this round; the prior 361 shipping-pin check is not presented as a fresh run here. Counts from previous rounds overlap and are not added. The initial compiler error, zero-test selection and failed immutable-manifest trial are preserved, explicitly excluded from successful verification.

Evidence: [normal check](evidence/constraint-public-check.log), [final build](evidence/constraint-public-final-build.log), [27 regressions](evidence/constraint-public-final-regressions.log), [two actual services](evidence/constraint-public-final-service.log), [source/binary recipe](evidence/constraint-public-recipe.json), [initial compiler error](evidence/constraint-public-initial-build.log), [initial zero selection](evidence/constraint-public-initial-zero-selection.log), [failed initial service trial](evidence/constraint-public-initial-immutable-manifest-failure.log).

## Remaining scope

This change does not establish exact conviction for constrained token-selection fraud, every active computation/output/state family, maximum input/profile budgets, the producer's automatic multipart worker end to end, initial/prefix or other non-prompt state acquisition, cumulative disclosure/scope/hiding, global RAM/economics/inclusion bounds, or authenticated remote/pruned-IBD recovery. Those remain required by the original full-G14 objective. The existing immutable-declaration/default behavior also needs its own real-service adversarial coverage; the failed trial does not prove its eventual default.

In particular, the existing host mask compares the job-selected table root and the table's asserted tokenizer ID. A mismatch returns a host refusal. Full G14 still needs evidence for the relationship between the correct registered model/tokenizer and the committed table root, and an objective public route when a producer commits a different table. Merely matching the job's own root or abstaining on mismatch does not prove objective conviction of that job violation. This patch preserves the existing host predicate and makes no claim that it closes that question.
