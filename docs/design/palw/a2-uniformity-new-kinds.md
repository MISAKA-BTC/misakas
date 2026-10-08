# A-2 uniformity for every kind the live build cannot decode (A2U, 2026-10-08)

**Status:** implemented on `fix/a2-uniform-new-tags` (lane A2U), over the integration line at `35a9ae1c8`. No live id moves.
**Finding:** X8R's review (2026-10-08). **Live build:** testnet-12 runs int-12, `rcore/int-12` @ `0b1c11b87`.

## 1. The class

testnet-12 declares the audit fence (`palw_audit_2026_09_11`). Under its A-2 rule a lifecycle payload (subnetwork 0x4b) that the
node **cannot decode** is *tolerated*: `validate_palw_lifecycle_tx` accepts the transaction, the block stands, and the extraction walk
skips the carrier. int-12 cannot decode any `PalwConsensusObjectV2` variant added after it, so for int-12 every such object is
bytes, tolerated and skipped.

A newer build **decodes** those bytes. It splits from int-12 the moment a mixed fleet exists, before any fence, in either of two
ways:

* **it refuses bytes int-12 accepts.** A may-ride arm, a shape check or a size bound runs at isolation. For example, it refuses an
  unsigned kernel route, an oversized Panel proof, or an envelope that wraps no registration. The upgraded node marks invalid a block
  that int-12 accepts. The same holds for a header carriage form int-12 has no arm for.
- **it folds, charges or counts what int-12 skips.** A per-block cap, a rent, a budget or a state row moves on one build and not on
  the other. Then the PALW state root differs, or a later object in the same block is accepted on one build and dropped on the other.

The mirror image also exists. On a ruleset that does **not** declare the audit fence, int-12 *refuses* undecodable bytes. A newer
build that decodes a well-formed new kind and lets it ride accepts a block int-12 refuses.

**The rule** (now enforced centrally): below the fence that owns a kind or form, the newer build reads its bytes **exactly as int-12
reads them**. That means the same block verdict, the same skip, no charge, no budget spent and no state write. At and above the
fence, the kind's own rules apply.

## 2. The members, how int-12 treats them, and how the integration line treated them before this fix

`git diff 0b1c11b87..35a9ae1c8` was enumerated over:

* the object enum (by scanning its source for variants and tags);
* every Borsh-derived type in the changed files;
* the isolation, header-context and UTXO-context validators;
* the header processor, the pruning-proof gate and the PoW state;
* coinbase construction and validation;
* the mempool and template paths;
* every site that decodes a `PalwConsensusObjectV2` or a lifecycle payload.

### 2.1 Members (each one fixed)

| # | Member | Owning fence | int-12 (t12 declares audit) | Integration line before the fix (fences unarmed) | Split? |
|---|---|---|---|---|---|
| 1 | Tags **104–107, 109** (onboarding) on 0x4b | `palw_probabilistic_constraints_v1` | undecodable → tolerated; skipped | decoded; **unsigned → `ObjectMayNotRide` → block invalid**; well-formed → dropped by name in the acceptance walk | **yes** (unsigned) |
| 2 | Tag **108** `SignedRegistrationV1` | `palw_signed_registration_v1` | undecodable → tolerated | **unsigned, or wrapping a non-registration → block invalid**; well-formed → unwrap refused, dropped | **yes** |
| 3 | Tag **110** `KernelRouteV1` | `palw_probabilistic_constraints_v1` | undecodable → tolerated | **unsigned, empty or > `PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1` → block invalid** | **yes** |
| 4 | Tag **111** `KernelConstraintReceiptV1` | `palw_probabilistic_constraints_v1` | undecodable → tolerated | **unsigned → block invalid** | **yes** |
| 5 | Tag **120** `PanelBeaconProofV3` | `palw_permissionless_panel_v1` | undecodable → tolerated | **proof > `MAX_BEACON_PROOF_BYTES_V1` → block invalid** | **yes** |
| 6 | Tags 104–111 and 120, **well-formed**, on a ruleset **without** the audit fence | (as above) | undecodable → **refused** (`Undecodable`) | decoded, rides → block **valid** | **yes** (non-audit rulesets) |
| 7 | `ObjectChunk` (tag 15, int-12's) **completing a group whose bytes are tag 110, 105 or 109** | the inner kind's | completion: `ChunkedObjectUndecodable`, chunk dropped. With D14 dormant, the walk counts it against `PALW_CERTIFICATION_MAX_PER_BLOCK` (structural completion) | the walk **excluded it from the certification count** (`palw_kernel_chunk_inner`, no fence). The gate judged its signature; the fold decoded the inner | **yes**: `certifications_graded` differs, so a later `FamilyCertified` in the same block is graded on one build and dropped on the other (state root) |
| 8 | **`PFS4`** receipt carriage on an algo-7 header | `palw_receipt_spend_v4` | `check_palw_commitment_shape_at` has no `PFS4` arm: the carriage is decoded as `PFS3` and refused for its magic, or as over the 8,192-byte cap | the shape gate **admitted** a well-formed `PFS4` (with a 4× cap). The header path still refused it, by name (`BelowFence`, a different refusal). **The pruning-proof path runs only the shape gate, so it accepted the header.** The UTXO validator's block admission admitted it too | **yes** (pruning proof / IBD) |

### 2.2 Checked and found not to be members

Each of these was checked against int-12, and each gives the same verdict with nothing extra written.

* **Legacy kinds' wire forms.** A script compared every Borsh-derived type in every changed non-test file at `0b1c11b87` and HEAD.
  No type carried inside an int-12 variant changed. Four types changed, and none of them is carriage; each is state or delta:
  `PalwVoidReasonV2` (+120–122), `PalwDeltaEntryV2` (explicit discriminants, implicit ones unchanged), `PalwNoChangeReasonV1` (+6)
  and `PalwStateParamsV2`. Nested enums of the int-12 variants gained no variant.
* **Legacy may-ride arms and their shape helpers.** The diff of `palw_lifecycle_object_may_ride_v2` only adds arms. The helpers it
  calls for int-12 kinds (vertex, mesh, adapter, held-close, TIR/gen one-move) are unchanged; their files' diffs are stateful functions.
* **Other decode sites.** The activation-pool, improvement-pool and model-market sink bindings answer `_ => None` for both "another
  kind" and "undecodable". The court-close chunk assembly keeps only `CourtClosed`. The graded-vector counters keep only
  `FamilyCertified`. Every carrier-binding filter in the extraction walk acts on int-12 kinds.
* **Rent burned in the coinbase.** `palw_object_rent_ceiling_v1` answers `_ => 0`, so it is equal today. The UTXO walk now asks the
  central rule first, so a later kind with a rent cannot reopen this.
* **Heartbeat H1-carrier classification and the RPC registration status.** These are mempool, template and RPC reads; no block
  verdict uses them.
* **Subnetworks, FP job versions, header algo ids, the coinbase payload parser.** All unchanged since `0b1c11b87`.
* **V4 payouts, the widened coinbase output cap, DNS retirement.** Each is behind its own fence, and none is armed on testnet-12 (the
  output cap only widens, and only where the fence is declared).
* **ADR-0125's semantic amendment** (mergeset classification, `round_lane_members`, the heartbeat bound). int-12's GHOSTDAG colours
  every round block red (`round_flags` → `add_red`), and a round block never becomes a selected parent. So the "genuine reds" and the
  members HEAD reads are the same set int-12 reads.
* **The IBD sidecar's transport cap.** It decides which peer serves the state, not any block's verdict.
* **Kernel-route inner kinds 12–19** (inside tag 110's bytes). These are covered by tag 110's fence. Above it, a kernel decode failure
  drops the object (the gate's `KernelRouteObjectV1::decode` and the fold's `let Ok(..) else { return Ok(()) }`) and never invalidates
  the block.
* **RFC-0008 v2, tag 130 and `PXE2`/`PXA2`** (X8R, `rfc8/x8r-review`, not on this line). X8R's fix reads tag 130 as undecodable at
  isolation and refuses `PXE2` with the v1 gate's error, which is this rule. When that branch merges, the table test
  (`every_kind_in_the_enum_has_exactly_one_owner`) requires a row `(130, "ExecWorkRootOpenedV2", ExecPayloadV2)`, a new
  `PalwLifecycleKindFenceV1` variant and an owner arm. Its header forms join `PalwHeaderFormFenceV1`.

**int-12 itself is not affected.** Every member above is a verdict the *newer* build gives differently. int-12's own handling (A-2
tolerance) is what the fix converges to.

## 3. The fix: one central rule

`consensus/core/src/palw_lifecycle_objects_v2.rs` holds the rule and its tables.

* **`palw_lifecycle_kind_owner_v1(object) -> Int12 | Fence(PalwLifecycleKindFenceV1)`.** This is an exhaustive `match` with no
  wildcard. A new `PalwConsensusObjectV2` variant does not compile until it names its owner.
* **`PALW_LIFECYCLE_INT12_KINDS_V1`** lists the 100 `(tag, name)` pairs int-12 decodes. It is frozen and pinned by an FNV-1a.
* **`PALW_LIFECYCLE_NEW_KINDS_V1`** lists every later kind with its owning fence:

  | Tags | Owning fence |
  |---|---|
  | 104–107, 109, 110, 111 | `palw_probabilistic_constraints_v1` |
  | 108 | `palw_signed_registration_v1` |
  | 120 | `palw_permissionless_panel_v1` |

* **`PalwLifecycleKindFencesV1`** holds the fences' activations, resolved once by `Params::palw_lifecycle_kind_fences_v1()`.
  `kind_in_force_at(object, daa)` is the single question every height-holding site asks.

### 3.1 Where the rule is applied

| Site | What it does below the fence |
|---|---|
| Isolation (`validate_palw_lifecycle_tx`, no height) | A kind owned by a fence gets int-12's verdict at **every** height: tolerated where the audit fence is declared, `Undecodable` where it is not. This is X8R's tag-130 treatment, generalised. |
| Header context (`TransactionValidator::check_palw_lifecycle_kind_in_context`, block DAA; also run by the mempool and template) | At or above the fence, the kind's may-ride rule applies, and a refusal invalidates the block. Below the fence, nothing is asked. |
| Objects of a block (`palw_v2_objects_of_block`) | A kind not in force is moved to the skip log as "payload does not decode" **before** the acceptance walk. No slot, rent, budget, cap or refund is consulted, and the walk sees exactly int-12's object list. The per-kind "dropped by name" checks stay as second locks. |
| Chunk reader (`palw_kernel_chunk_inner`, used by the certification cap and the gate) | An inner kind not in force is `None`, as undecodable bytes. The cap counts the chunk as int-12 does, and the gate asks nothing. |
| The fold's chunk completion (`palw_fold_kind_in_force_v1`, read from the kernel route's extras and the Panel mirror) | An inner kind not in force gives `ChunkedObjectUndecodable("Unexpected variant tag: N")`, which is borsh's own message and so int-12's words. |
| UTXO walk rent | Nothing is burned for a kind not in force. |
| `Params::validate_palw_lifecycle_kind_fences_v1` | Arming any owning fence without `palw_audit_2026_09_11` declared is refused. Without the audit fence, isolation could only refuse the kind. |

### 3.2 Header forms

`consensus/core/src/pow_layer0.rs` handles header forms the same way.

* `PalwHeaderFormFenceV1::ReceiptSpendV4` is owned by `palw_receipt_spend_v4`, and `palw_header_form_owner_v1` identifies it.
* `PalwHeaderFormFencesV1` is resolved by `Params::palw_header_form_fences_v1()`.
* `check_palw_commitment_shape_with_forms_at` admits a form only where its fence is in force at the header's own DAA. Below the fence
  the cap, the decode and the refusal are int-12's.
* `check_palw_commitment_shape_at`, the four-argument entry point, now reads **no** later form. It is int-12's gate byte for byte.

All three production gates pass the forms:

* the header processor (`pre_ghostdag_validation`);
* the pruning proof (`PruningProofManager::with_header_forms`);
* the UTXO validator's block admission (`check_palw_block_admission_v2`).

The by-name `BelowFence` refusal in `palw_carriage_stateless_v2` stays as a second lock, and is now unreachable below the fence.

### 3.3 One level down, and inside the live build's own kinds

The same rule applies below the top-level object tag, in two places.

**Kernel-route inner kinds (inside tag 110).** `palw_kernel_route_inner_fence_v1` is exhaustive and wildcard-free.
`PALW_KERNEL_ROUTE_INNER_KINDS_V1` lists each inner kind with the fence it needs beyond tag 110's own:

| Inner kinds | Owning fence |
|---|---|
| 1–12 | none (tag 110's fence is the whole rule) |
| 13, 14 (OPV mode) | `palw_panel_free_v1` |

The processor's kernel gate (`palw_kernel_route_object_is_signed`) now asks the table instead of a hand-written OPV arm. An inner kind
whose fence is not in force is dropped and the block stands, which is exactly what a build without the kind does at the kernel's decode.

In-flight lanes join the table when they merge:

| Lane | Inner kinds | Owning fence |
|---|---|---|
| G14-R4 | 15 | (its fence) |
| K2S | 16–18 | the K2 fence |
| R4X | 19 | `palw_typed_roots_v1` |

Adding an inner kind without a row does not compile, and `every_kernel_inner_kind_has_exactly_one_row` reconciles the table with
`KernelRouteObjectV1`'s source.

**Appended variants and fields inside a kind int-12 decodes.** For example, HFX's `PalwGenProfileV1::Head = 6` and further Borsh
variants inside tag 68, which **is armed on live testnet-12** (`palw_gen_v1`). int-12 fails the whole payload's decode and tolerates it,
so the newer build must read such a payload as undecodable below the appended form's fence.

* Every Borsh wire type int-12 compiled is frozen in `consensus/core/src/palw_lifecycle_objects_v2/int12_borsh_manifest.tsv`. It holds
  one FNV-1a per derived `struct`/`enum` and per hand-written `impl BorshDeserialize`, after removing comments and whitespace. It covers
  consensus-core, hashes and the five PALW crates consensus-core decodes with, and was made by the test itself from a worktree at
  `0b1c11b87`.
* `every_int12_wire_type_is_unchanged_or_classified` fails on any changed or removed int-12 type that is not classified in
  `PALW_INT12_WIRE_CHANGES_V1` as one of:
  * `ObjectEnum`: the top-level enum, whose kinds the kind table owns;
  * `NotCarried(why)`: state, delta or params;
  * `Carried { fence, digest }`: the digest is pinned, so a further change is classified again.
* A `Carried` change also needs a **guarded arm in `palw_lifecycle_kind_owner_v1` before the `Int12` arm**. For HFX that is
  `ClassRegisteredGenV1 { .. } if <carries the appended profile> => Fence(<HFX fence>)`, for every int-12 kind that can carry the form.
  From that arm, isolation, the header context, the objects-of-block walk, the chunk reader, the fold and the rent all read the payload
  as int-12 does, with no further edit.

The changes classified today are:

* `PalwConsensusObjectV2` (`ObjectEnum`);
* `PalwDeltaEntryV2`, `PalwVoidReasonV2`, `PalwStateParamsV2` and `PalwNoChangeReasonV1` (all `NotCarried`).

### 3.4 Other in-flight members (the Lead's list, 2026-10-08)

* **DA16, tags 150–153 → `palw_provider_court_v1`.** These are new top-level kinds. The enum scan fails until each has a
  `PALW_LIFECYCLE_NEW_KINDS_V1` row. The owner `match` does not compile without an arm, and the fence needs a `PalwLifecycleKindFenceV1`
  variant, a `PalwLifecycleKindFencesV1` field and an arm in the fold's `palw_fold_kind_in_force_v1`.
* **G14-R4, tag 113 (the G14 chunk table).** Same as DA16, under its fence.
* **SHARD (delta 171, tail `0xED`).** These are state encodings. A change to an int-12 state type is classified `NotCarried`. A new type
  is not an int-12 type and needs nothing.
* **L2FC (a header-root formula change behind `palw_fork_choice_commitment_v1`).** This is a header form in this sense: below its fence,
  every pre-fence header must hash and validate byte-identically to int-12. It must be pinned at merge by a dormant-root/hash parity
  test. The manifest test does not see formulas.

## 4. Tests

### 4.1 Core unit tests

In `palw_lifecycle_objects_v2::tests::a2u`:

* **`every_kind_in_the_enum_has_exactly_one_owner`** reads `PalwConsensusObjectV2`'s source and applies Rust's discriminant rule. It
  requires every `(variant, tag)` to sit in exactly one table, and every table row to exist in the enum. **A kind added without a row
  fails here.**
* **`the_int12_kind_list_is_frozen`** pins the int-12 list with an FNV-1a.
* **`the_new_kind_table_is_the_owner_function`** checks a sample of every new kind against its table row and owner.
* **`below_its_fence_a_new_kind_is_judged_as_the_live_build_judges_undecodable_bytes`**: every new kind, well-formed or malformed
  (unsigned, oversized, empty encoding, an envelope wrapping no registration), gets the isolation verdict of a tag-254 payload no
  build decodes, both tolerated and refused. The in-context rule asks nothing below the fence (or at `never()`) and is exactly may-ride
  at the fence.
* **`a_not_in_force_kind_is_undecodable_in_the_live_builds_words`** pins `palw_lifecycle_unknown_tag_reason_v1` against borsh.
* **`every_kernel_inner_kind_has_exactly_one_row`** reconciles the inner-kind table with `KernelRouteObjectV1`'s source, and checks
  each row's fence against `palw_kernel_route_inner_fence_v1`, decoding each inner kind from a zero-filled body.
* **`every_int12_wire_type_is_unchanged_or_classified`** checks the frozen manifest (§3.3). Each run reports any unclassified change and
  any stale row.
* **`an_owning_fence_needs_the_audit_fence_declared`** checks the params validation.

### 4.2 The processor pin test

`t12_a2u_every_new_kind_below_its_fence_is_the_live_builds_undecodable_payload_and_every_node_agrees`, in
`consensus/src/pipeline/virtual_processor/tests/t12_a2u_new_kinds_uniform.rs`, generalises X8R's P11. It runs on testnet-12 with
harness cards and every owning fence unarmed. Every block is a heartbeat, and every carrier is a funded 0x4b transaction. The chain
carries:

* every new kind, well-formed;
* every signed kind, unsigned;
* the malformed forms;
* a tag-254 reference payload;
* three one-chunk groups of new kinds;
* a two-chunk group that completes into a new kind.

It asserts:

* each carrier is in its block, and each block is valid;
* there is no kernel-route state and no Panel V3 state;
* no group completed;
* the walk's chunk reader is "undecodable" below the fence and the kind past it;
* a `PFS4` header is refused on the header path **and** the pruning-proof path with int-12's own refusal (the unchanged gate), and
  differently on a node where the form is in force.

An unarmed node and a node with every owning fence armed far above the chain then replay every block. They give the same statuses,
refusals, sink, PALW state root, UTXO multiset and pruning-proof refusal.

### 4.3 Adapted test

`rfc9_v4_chain_e2e` step 1 now expects the `PFS4` probe below the fence to be refused as int-12 refuses it: over int-12's 8,192-byte
cap. It previously expected `BelowFence`.

## 5. Replay through int-12 itself

The pin test writes its chain when `A2U_INT12_REPLAY_OUT=<file>` is set. The file contains the genesis hash, every block's header and
transactions, the refused block with its refusal, and the final view (sink, PALW root, multiset). It is Borsh; `Header` and
`Transaction` are unchanged since `0b1c11b87`.

A test placed in a worktree at `0b1c11b87` rebuilds the same chain with int-12's own `t12_with_harness_cards()` and
`t12_genesis_chain`, and checks it against the file:

* the genesis must be equal;
* every block must be accepted;
* every refusal must be equal;
* the final view must be equal.

Result: see §7.

## 6. The rule for every future kind (also in `remaining-rfc-integration-matrix.md` §2)

0. The same holds for a kernel-route inner kind (`PALW_KERNEL_ROUTE_INNER_KINDS_V1`) and for any change to a type the live build
   decodes (`PALW_INT12_WIRE_CHANGES_V1`, plus a guarded owner arm when the change is carried).
1. **Every new `PalwConsensusObjectV2` kind needs a kind→fence entry.** That means a row in `PALW_LIFECYCLE_NEW_KINDS_V1`, an arm in
   `palw_lifecycle_kind_owner_v1` (the compiler demands the arm, the table test demands the row), and its fence in
   `PalwLifecycleKindFenceV1` / `PalwLifecycleKindFencesV1`.
2. **Every new header carriage form or coinbase trailer** gets a `PalwHeaderFormFenceV1` variant. Every gate that holds the header's
   height must admit it only through `check_palw_commitment_shape_with_forms_at`, or the trailer's equivalent.
3. **Below its fence a kind or form rides unjudged.** Its bytes are read exactly as the live build reads them: the same verdict, the
   same skip, nothing charged, counted or written. Never add a may-ride arm, size bound or shape check that refuses at isolation for a
   kind the live build cannot decode. Put it in the kind's own rule, which the header context asks at its fence.
4. The live-build list (`PALW_LIFECYCLE_INT12_KINDS_V1`) is frozen. It only changes when a new release becomes the live baseline, in
   its own reviewed commit.

## 7. Results

(Filled in by the build record below.)
