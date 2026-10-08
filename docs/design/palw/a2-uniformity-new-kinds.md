# A-2 uniformity for every kind the live build cannot decode (A2U, 2026-10-08)

**Status:** implemented on `fix/a2-uniform-new-tags` (lane A2U), over the integration line at `35a9ae1c8`; the central kind→fence
table (§3.5) holds every post-int-12 member, landed or allocated. No live id moves. Results: §7.
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

* **`PalwLifecycleKindFencesV1`** holds the fences' activations, resolved once by `Params::palw_lifecycle_kind_fences_v1()` into
  an array indexed by the fence's discriminant (`with(fence, activation)` sets one). `kind_in_force_at(object, daa)` is the single
  question every height-holding site asks. A fence missing from `PalwLifecycleKindFenceV1::ALL` is never in force: its kinds ride
  unjudged, the safe reading (`the_fence_list_is_the_enum` keeps `ALL` equal to the enum).

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

### 3.3 One level down: the kernel route's inner kinds

Tag 110's bytes are a `KernelRouteObjectV1`. Below `palw_probabilistic_constraints_v1` all of them are int-12's undecodable payload
(§3). Past it, a build that does not know an inner kind fails the kernel's decode at the gate (`KernelRouteObjectV1::decode`) and drops
the object, the block standing. So an inner kind added later is read the same way below ITS fence.

* `palw_kernel_route_inner_fence_v1` is exhaustive and wildcard-free; `PALW_KERNEL_ROUTE_INNER_KINDS_V1` lists each inner kind with
  the fence it needs beyond tag 110's own (`None`: tag 110's fence is the whole rule).
* The processor's kernel gate (`palw_kernel_route_object_is_signed`) asks the table (`palw_kernel_inner_fence_at`) instead of a
  hand-written arm. RFC-0015's inner 13 and 14 are `palw_panel_free_v1`'s.
* A variant appended INSIDE an inner kind (R4X's `ClaimBodyV1::Spec` inside a `CommitClaim`) is answered by a guarded arm of
  `palw_kernel_route_inner_fence_v1` ahead of the kind's own, as the top-level guarded arms work (§3.4).
* `every_kernel_inner_kind_has_exactly_one_row` reconciles the table with `KernelRouteObjectV1`'s source, decoding each inner kind
  from a zero-filled body.

Inner kinds added before tag 110's fence is first armed on a live network ride with it (`None`): G14-R4's 15 and K2S's 16–18 are
such. Once tag 110 is live, every later inner kind needs a fence of its own. R4X's inner 19 `Spec` and its typed-root proof
(`ProsecutionV1::Spec` inside a `FileProof`, by a guarded arm) are `palw_typed_roots_v1`'s (`PalwKernelInnerFenceV1::TypedRootsV1`);
the hand-written arm R4X put in the gate is replaced by the table's.

**DA16 (tags 150–153) and the completing chunk.** On integration a `ProviderAnswerV1` (152) assembled from `ObjectChunk`s was judged at
the completing chunk by the gate below `palw_provider_court_v1` (refused for the fence, as lane D's 105/109), and the walk's chunk
reader excluded the chunk from the certification cap — a reading int-12 does not share: to int-12 those bytes are undecodable, the
chunk counts against the cap and the fold refuses it as `ChunkedObjectUndecodable`. With 150–153 owned by `ProviderCourtV1`, below the
fence `palw_kernel_chunk_inner` answers `None` (no gate judges it, the cap counts it as int-12 does) and the fold's
`palw_fold_kind_in_force_v1` refuses it in int-12's words; isolation tolerates 150–153 at every height (their may-ride arm, unsigned →
refused, runs only past the fence). The pin test carries a signed 152 directly, as a one-chunk group and as a two-chunk group. G14R's inner 20 `CommitClaimSalted` (the salted claim
seal v2) is `palw_panel_free_v1`'s, beside the OPV registrations: below that fence the kernel refuses it and the gate drops it through
the table (`PalwKernelInnerFenceV1::PanelFreeV1`), never through a hand-written arm. Inner kinds 21 and 22 are not allocated.

### 3.4 Inside the live build's own kinds: its wire types are frozen

A kind int-12 DOES decode can still carry bytes it cannot read as a newer build reads them. There are two shapes, and they need
different readings.

| Shape | Example | int-12's verdict | The newer build below the fence |
|---|---|---|---|
| **appended**: a variant appended to a nested Borsh enum, or a field appended to a nested struct | HFX's `PalwGenProfileOffersV1::Head` (3) inside tag 68 | the whole payload fails to decode: tolerated on 0x4b, skipped | a **guarded arm** in `palw_lifecycle_kind_owner_v1`, before the `Int12` arm, names the fence that owns every object carrying the form; from that arm every site of §3.1 reads it as undecodable |
| **re-read**: a new meaning for bytes int-12 decodes, such as a tag byte it reads by hand | HFX's `PalwGenProfileV1::Head = 6` in the class's `profile: u8` | decoded; judged by int-12's own code at int-12's own stage (for tag 68, a refusal where its `from_tag` fails, in the acceptance walk) | **no guarded arm** — reading it as undecodable would skip it before the acceptance walk, a different verdict wherever int-12 charges, counts or refunds before refusing. The kind's own rule refuses the new meaning below its fence, at the stage int-12 does; a probe in the pin test (§4.2) and the int-12 replay (§5) are the evidence |

The freeze:

* Every wire type int-12 compiled is frozen in `consensus/core/src/palw_lifecycle_objects_v2/int12_borsh_manifest.tsv`: one FNV-1a per
  Borsh-derived `struct`/`enum`, per hand-written `impl BorshDeserialize` and per `#[repr(uN)]` enum (a tag read by hand from a byte),
  after removing comments, `#[cfg(test)]` items and whitespace; the `#[borsh]`/`#[repr]` attributes on either side of the derive are
  part of the digest. It covers consensus-core, hashes, math and every PALW crate consensus-core decodes with that existed at
  `0b1c11b87` (tir, tir-exec, tir-lower, tir-sketch, gen); panel, challenge and kernel did not exist there, and their types are carried
  only by post-int-12 kinds. The manifest is made by the test itself from `git archive 0b1c11b87` of those directories
  (`A2U_WIRE_MANIFEST_OF=<tree> A2U_WIRE_MANIFEST_OUT=<file>`).
* `every_int12_wire_type_is_unchanged_or_classified` fails on any changed or removed int-12 type not classified in
  `PALW_INT12_WIRE_CHANGES_V1` as one of `ObjectEnum`, `NotCarried(why)`, `CarriedAppended { fence, digest }` or
  `CarriedReread { fence, digest }`. A carried digest is pinned, so a further change is classified again. **`NotCarried` is checked, not
  trusted**: no such type may be reachable, by name, from `PalwLifecycleTxPayloadV2` through the declarations of the current tree.
* `every_appended_form_has_a_guarded_owner_arm` asks, of every `CarriedAppended` change, a sample object carrying the form that the
  owner function maps to the named fence.
* Every carried change needs its `Int12Inner` row in the central table (§3.5), with the same fence.

Known limits of the freeze, each covered by the int-12 replay instead: a change inside a helper function a hand-written decoder calls;
a `use` that rebinds a name to another type; a formula (a hash or a root) rather than a wire form.

The changes classified today are `PalwConsensusObjectV2` (`ObjectEnum`) and the `NotCarried` state, delta and parameter types the
manifest reports (§7).

### 3.5 The central kind → fence table

`PALW_A2_KIND_FENCE_TABLE_V1` (`palw_lifecycle_objects_v2.rs`) is the one table of every post-int-12 member, landed or allocated, and
the fence below which it rides unjudged. A2U keeps it; the Lead allocates; a lane fills its row's code at merge. Each row names its
fence as a `Params` field name, so a row can be written before its lane merges. An unallocated tag (112, 114–119, 121–129, 140–149)
has no row, so a kind landing there fails the table test until the Lead allocates it.

| Slot | Members | Fence | Lane | In this tree |
|---|---|---|---|---|
| object tags 104–107 | onboarding | `palw_probabilistic_constraints_v1` | G14 lane D | yes |
| object tag 108 | `SignedRegistrationV1` | `palw_signed_registration_v1` | RFC-0009 | yes |
| object tag 109 | `ConformanceEvidenceV1` | `palw_probabilistic_constraints_v1` | OB-P0 | yes |
| object tags 110–111 | 110 route, 111 receipt | `palw_probabilistic_constraints_v1` | G14 lane D | yes |
| object tag 113 | `KernelRouteChunkV1` (the route's own chunk lane) | `palw_probabilistic_constraints_v1` | G14-R4 | no |
| object tag 120 | `PanelBeaconProofV3` | `palw_permissionless_panel_v1` | RFC-0010 | yes |
| object tags 130–139 | 130 `ExecWorkRootOpenedV2` | `palw_exec_payload_v2` | X8R | no |
| object tags 150–153 | `ProviderLeaseV1`, `ProviderChallengeV1`, `ProviderAnswerV1`, `DaTransferV1` | `palw_provider_court_v1` (in force only where the kernel route's fence is too) | DA16 | yes (merged with integration `648a9f481`) |
| inner kinds 1–12 | the kernel route | `palw_probabilistic_constraints_v1` (tag 110's own) | G14 lane D | yes |
| inner kinds 13–14 | OPV registrations | `palw_panel_free_v1` | RFC-0015 | yes |
| inner kind 15 | `SealProof` | tag 110's own | G14-R4 | no |
| inner kinds 16–18 | segmented claim, tiled job, prompt tile | tag 110's own | K2S | no |
| inner kind 19 | `Spec` | `palw_typed_roots_v1` | R4X | yes |
| inner kind 20 | `CommitClaimSalted` (salted claim seal v2) | `palw_panel_free_v1` | G14R | no |
| nested in inner kinds | `ProsecutionV1::Segmented` (3), `ClaimBodyV1::Segmented` (2) | tag 110's own | K2S | no |
| nested in inner kinds | `ProsecutionV1::Spec` (4) inside `FileProof` (inner 7); `ClaimBodyV1::Spec` (3) is ledger state, never carried | `palw_typed_roots_v1` | R4X | yes (guarded arm) |
| header form algo 7 `PFS4` | V4 receipt carriage | `palw_receipt_spend_v4` | RFC-0009 | yes |
| header form algo 10 `PXE2` | EXEC envelope | `palw_exec_payload_v2` | X8R | no |
| coinbase trailer `PXA2` | EXEC anchor | `palw_exec_payload_v2` | X8R | no |
| inside tag 68 (re-read) | `PalwGenProfileV1::Head = 6` | `palw_task_heads_v1` | HFX | no |
| inside tag 68 (appended) | `PalwGenProfileOffersV1::Head` (3) | `palw_task_heads_v1` | HFX | no |
| FP job form (0x4a) | `PalwGenBodyV1::Head` (2) | `palw_task_heads_v1` | HFX | no |
| header formula | `palw_state_root = H(fork-choice leaf ‖ ADR-0043 root)` past the fence | `palw_fork_choice_commitment_v1` | L2FC | no |
| state encoding | per-shard V3 draw (delta 171, tail `0xED`) | `palw_permissionless_panel_v1` | SHARD | no |
| state encoding | per-segment pricing, the shard engine's encodings | `palw_tir_shard_segment_v2` | SHARD | no |

**The verdict int-12 gives depends on the carriage**, and "below the fence" means that verdict:

* 0x4b lifecycle payloads: undecodable is **tolerated** (testnet-12 declares the audit fence), the carrier skipped.
* 0x4a free-prompt jobs: undecodable is **refused** — there is no audit tolerance on 0x4a. A job form a lane adds (HFX's Head body)
  must be refused below its fence exactly as int-12 refuses it: the form passes the isolation door only where the ruleset carries the
  fence, and the header-context door below its height (the pattern of `palw_gen_door` and the decode-rules door).
* Header carriage forms: **refused** by int-12's shape gate (§3.2).
* Coinbase trailers: **opaque** miner bytes to int-12 — nothing read, nothing refused (X8R's reader is lenient only where the fence is
  armed, and the length rule is the body stage's at the height).
* Formulas and state encodings: **byte-identical** below the fence, block by block (the int-12 replay checks every block's root).

`every_landed_kind_sits_in_its_row_with_the_rows_fence` holds the table to the code: rows never overlap one another or int-12's tags;
every landed top-level kind, inner kind, header form and carried change sits in exactly one row whose fence is the one the code
answers; a row is marked landed exactly when the code in the tree fills it; a landed row's fence is a `Params` fence
(`palw_fences_v1`). A lane that merges a kind outside its allocation, under another fence, or without flipping its row fails there.

## 4. Tests

### 4.1 Core unit tests

In `palw_lifecycle_objects_v2::tests::a2u`:

* **`every_kind_in_the_enum_has_exactly_one_owner`** reads `PalwConsensusObjectV2`'s source and applies Rust's discriminant rule. It
  requires every `(variant, tag)` to sit in exactly one table, and every table row to exist in the enum. **A kind added without a row
  fails here.**
* **`the_int12_kind_list_is_frozen`** pins the int-12 list with an FNV-1a.
* **`the_new_kind_table_is_the_owner_function`** checks a sample of every new kind against its table row and owner.
* **`the_fence_list_is_the_enum`** keeps `PalwLifecycleKindFenceV1::ALL` equal to the enum, and checks each fence's slot.
* **`every_landed_kind_sits_in_its_row_with_the_rows_fence`** reconciles the central table with the code (§3.5) and prints the rows
  awaiting their lanes.
* **`every_appended_form_has_a_guarded_owner_arm`** (§3.4).
* **`below_its_fence_a_new_kind_is_judged_as_the_live_build_judges_undecodable_bytes`**: every new kind — hand-built well-formed,
  every malformed form (unsigned, oversized, empty encoding, an envelope wrapping no registration), and **each kind decoded from a
  zero-filled body, generically over the table** — gets the isolation verdict of a tag-254 payload no build decodes, both tolerated and
  refused. The in-context rule asks nothing below the fence, at `never()`, or when only other kinds' fences are armed, and is exactly
  may-ride at the fence.
* **`a_not_in_force_kind_is_undecodable_in_the_live_builds_words`** pins `palw_lifecycle_unknown_tag_reason_v1` against borsh.
* **`every_kernel_inner_kind_has_exactly_one_row`** (§3.3).
* **`every_int12_wire_type_is_unchanged_or_classified`** (§3.4).
* **`an_owning_fence_needs_the_audit_fence_declared`** checks the params validation.

### 4.2 The processor pin test: mixed verdicts over every new kind

`t12_a2u_mixed_verdicts_every_new_kind_below_its_fence_{launch,release}_ruleset`, in
`consensus/src/pipeline/virtual_processor/tests/t12_a2u_new_kinds_uniform.rs`, generalises X8R's P11. It runs testnet-12 with harness
cards and every owning fence unarmed, on two rulesets: **as launched**, and **the live release compressed** (every flag day live
testnet-12 has crossed — the DAA-750 list, the second list, capacity, `palw_tir_v1`, `palw_tir_fence2`, int-11 — at heights 20–40, the
chain carried past DAA 330 where the compressed release is the present one). Every block is a heartbeat or a real attempt; every
carrier is a funded 0x4b transaction. The chain carries:

* **every new kind generically**: each `PALW_LIFECYCLE_NEW_KINDS_V1` row's kind decoded from a zero-filled body, so a kind a lane
  adds is carried the moment its row exists; plus a hand-built well-formed, signed one of each;
* every may-ride refusal a new kind has, a tag-254 reference payload, and a **re-read probe** (a tag-68 class whose profile byte is
  `Head = 6`);
* three one-chunk groups of new kinds, and a two-chunk group that completes into a new kind;
* **a mixed block**: a quorum's `ReceiptLicensed` (an int-12 kind that folds and licenses a real claim), the same licence again (an
  int-12 kind the walk refuses), and new kinds riding unjudged.

It asserts each carrier is in its block and each block is valid; the licence licenses; there is no kernel-route state, no Panel V3
state and no completed group; the walk's chunk reader is "undecodable" below the fence and the kind past it; a `PFS4` header is
refused on the header path **and** the pruning-proof path with a refusal int-12's own gate gives, and differently on a node where the
form is in force. An unarmed node and a node with every owning fence armed far above the chain (armed through exhaustive matches over
every fence list, so a new fence must be armed there to compile) then replay every block: the same statuses, refusals, PALW state root
**at every block**, sink, UTXO multiset and pruning-proof refusal.

### 4.3 Adapted test

`rfc9_v4_chain_e2e` step 1 now expects the `PFS4` probe below the fence to be refused as int-12 refuses it: over int-12's 8,192-byte
cap. It previously expected `BelowFence`.

## 5. Replay through int-12 itself

The pin test writes each ruleset's chain when `A2U_INT12_REPLAY_OUT=<dir>` is set (`a2u-launch.borsh`, `a2u-release.borsh`: the
genesis, every block's header and transactions, every block's PALW root, the refused block with its header and pruning-proof
refusals, and the final view). `Header` and `Transaction` are unchanged since `0b1c11b87`.

`scripts/a2u-int12-replay.sh <work-dir> <dump>…` extracts the source of `0b1c11b87` (`git archive`, no worktree), adds
`a2u_int12_replay.rs.int12` as a test module, and runs it through the shared build gate. With int-12's own harness and fence lists it
demands the same genesis, every block accepted and UTXO-valid **with the same PALW root block by block**, every refusal in the same
words (header and pruning proof), and the same final view.

This is the merge gate for every row of §3.5 that a unit test cannot see — L2FC's formula, SHARD's unarmed fold, HFX's re-read
profile, a decoder helper's change: a lane re-runs the pin test and this replay after it merges.

## 6. The rule for every future kind (also in `remaining-rfc-integration-matrix.md` §2)

0. **The Lead allocates; the row comes first.** Every post-int-12 kind, inner kind, nested variant, header form, coinbase trailer,
   appended or re-read form inside an int-12 kind, FP job form, header formula and state encoding has a row in
   `PALW_A2_KIND_FENCE_TABLE_V1` naming its fence. A merge flips its row to landed; the table test holds the code to the row.
1. **Every new `PalwConsensusObjectV2` kind** needs a row in `PALW_LIFECYCLE_NEW_KINDS_V1` (the table test demands it), an arm in
   `palw_lifecycle_kind_owner_v1` (the compiler demands it), and — if its fence is new — a `PalwLifecycleKindFenceV1` variant, its
   `params_field`, its resolution in `Params::palw_lifecycle_kind_fences_v1`, its arm in the fold's `palw_fold_kind_in_force_v1` and
   its arming in the pin test's `arm_every_owning_fence` (each demanded by an exhaustive match).
2. **Every new kernel-route inner kind** needs its row in `PALW_KERNEL_ROUTE_INNER_KINDS_V1` and its arm in
   `palw_kernel_route_inner_fence_v1`; a fence of its own goes in `PalwKernelInnerFenceV1` and `palw_kernel_inner_fence_at`, never as a
   hand-written arm in the gate.
3. **Every new header carriage form or coinbase trailer** gets a `PalwHeaderFormFenceV1` variant (and its `ALL` entry). Every gate
   that holds the header's height admits it only through `check_palw_commitment_shape_with_forms_at`, or the trailer's equivalent.
4. **A change inside a type int-12 decodes** is classified in `PALW_INT12_WIRE_CHANGES_V1`: appended (a guarded owner arm and a
   sample) or re-read (the kind's own rule gives int-12's verdict at int-12's stage below the fence, and a probe in the pin test).
5. **Below its fence a kind or form rides unjudged.** Its bytes are read exactly as the live build reads them: the same verdict, the
   same skip, nothing charged, counted or written. Never add a may-ride arm, size bound or shape check that refuses at isolation for a
   kind the live build cannot decode on 0x4b; put it in the kind's own rule, which the header context asks at its fence. On 0x4a the
   live verdict for unknown bytes is refusal, and below the fence a new form is refused.
6. **Merge gate:** the A2U unit tests, the pin test on both rulesets, and the int-12 replay of its dumps (§5).
7. The live-build lists (`PALW_LIFECYCLE_INT12_KINDS_V1`, the manifest) are frozen. They change only when a new release becomes the
   live baseline, in their own reviewed commit.

**Worked example — merging G14-R4's tag 113 (`KernelRouteChunkV1`, in `g14/r4-fixes` and `adv/c4r4`).** On those branches, without
this rule, isolation runs its may-ride arm (unsigned, or an index, count or part out of range → `ObjectMayNotRide`) at every height, so
the upgraded node marks invalid a block int-12 tolerates — the split C4R4's `c4r4_a2_tag113` pins. At the merge with this branch:
add `O::KernelRouteChunkV1 { .. }` to the `ProbabilisticConstraintsV1` arm of `palw_lifecycle_kind_owner_v1` (the compiler demands
it), add `(113, "KernelRouteChunkV1", ProbabilisticConstraintsV1)` to `PALW_LIFECYCLE_NEW_KINDS_V1` and flip the tag-113 row to landed
(the table tests demand both). Nothing else: isolation then tolerates it at every height, the header context asks its may-ride arm
only past `palw_probabilistic_constraints_v1`, the objects-of-block walk skips it below (so the chunk gate, the chunk lane's rows and
deposit, and the UTXO walk never see it), and the pin test carries it generically, well-formed and near-zero.

## 7. Results

(Filled in by the build record below.)
