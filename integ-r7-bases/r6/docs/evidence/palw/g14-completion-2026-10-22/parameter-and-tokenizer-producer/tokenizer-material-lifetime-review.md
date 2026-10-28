# Production generator / Store / signing-context lifetime review

Read-only during build39261. No repo edits/builds/RPC.

## Current ownership finding
The production generator returns NativeRetained<Generated>; Generated has private proof Vec and NO Clone, Deserialize or consuming bare-value accessor. proofs(&self)->&[Proof] borrows the native owner. Passing that slice directly into TokenizerMaterial makes the returned TokenizerTerminalEvidence borrow the same owner until its last recheck. Rust then prevents dropping/moving the owner while evidence is subsequently used. No Store/evidence proof clone occurs: evidence stores the borrowed Material. Raw and rendering are likewise borrowed through the same minimum lifetime.

The actual-native test currently does `produced.proofs().to_vec()`. This deep clone creates an additional bare proof payload without a new native reservation; keeping `produced` in scope does NOT transfer its ticket to that separate allocation. It is test-only (not current production generator/Store behavior), but does not demonstrate the intended direct owner loan and should be corrected. `borrow-positive.patch` is the minimal follow-up: positive evidence borrows `produced.proofs()` directly. The explicit foreign-proof negative keeps its deliberately mutated test-only Vec; it is not evidence of a resource-owned production path.

Raw Vec and table.opening_of output in this fixture also originate in existing unreserved fixture construction. The generator's lease explicitly excludes borrowed raw/rendering. Thus current test cannot establish production acquisition or all-material resource ownership. Do not say output lease covers raw file, table source, opening-of transient builder, or Store/source/verification scratch. The newly returned owner only covers generator-owned construction/output. Existing Store docs already require these separate admissions.

## Concrete next production path without a new complete/paid helper
1. Raw acquisition must return a pre-reserved actual-byte owner (e.g. existing bounded native decode/read path), held by caller; EOF certificate authenticates hash/length/time but is NOT a physical raw owner or retrieval mechanism.
2. Rendering/opening construction must obtain its own reservation before allocating the actual nested rendering Vec/step path. PalwTokenTableOpeningV1 already has generated NativeHeap visitor. NativeRetained::try_from_clone_v1 can safely clone an EXISTING borrowed opening under a new ticket; retain_existing_v1 alone does not cover prior opening_of allocation.
3. Generate using borrowed raw_owner slice and actual native Court. Keep generated owner, raw owner and rendering owner in caller scope.
4. Build material with `raw_owner.as_slice()`, `generated.proofs()`, and `&rendering_owner`; sign that exact material identity; call unchanged Store builder and recheck while all three owners remain borrowed. No proof Vec copy is required. This can be demonstrated in a test by pre-reserved clones of existing fixture operands, explicitly labeled a clone-handoff test rather than original acquisition.
5. Full native invocation still needs source/frame/view reconstruction, signing/key handling, canonical replay clone/packet scratch, and evidence.intent signature clone reservation. Generator ticket is not a global override of these independent allocations.

For compile-time restriction at the eventual accepting entry, take references to the three actual NativeRetained owners and construct Material internally. Do not add a generic caller complete bool or bare DTO proof of ownership. Existing primitive borrowed-slice API may remain a pure verifier; acceptance entry must separately bind actual source/policy/actor/time and tickets. We should not introduce another unattached public quote/opaque helper merely to enforce a fixture property.

## Local generation context before final signature
Data path construction calls shared palw_json_key_path(namespace,key_hash), present_leaf(path), node(height,left,right). Namespace is parser next_object sequence or fixed ordinary/added-id namespace; context is absent from those inputs. Parser uses context only to reject method calls with a different control context. Raw flat hash is tokenizer identity + exact raw length/data, independent of context. Therefore local generation can precede final material_id/signature without a circular signature dependency.

This does NOT authorize a move signed in one context in another: Store reconstructs its final Court from actual state and intent.signing_message(network), verifies current active filer signature, material identity, original immutable semantics and EOF, and replays all paths again. The new two-context test is a REQUIRED empirical guard, not yet a result. If any actual data proof/root differs or final Store rejects generated proof because of context dependence, stop and report; no wire or signature change is justified by this review.

Known remaining gaps: public raw acquisition/delivery, complete original graph and paid ingress, raw/rendering pre-reserved producers, native replay scratch/evidence intent clone ownership. None are supplied by local proof generation success.
