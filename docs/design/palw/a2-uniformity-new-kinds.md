# A-2 uniformity for every kind the live build cannot decode (A2U, 2026-10-08)

**Status:** implemented on `fix/a2-uniform-new-tags` (lane A2U), over the integration line at `b8ae9412b` (the user's `pre`, SMALL,
SHARD2, X12N merged 2026-10-10); the central kind→fence table (§3.5) holds every post-int-12 member, landed or allocated, and the
live build's kinds a later fence judges anew (ADR-0175). No live id moves on this branch's own account (§2.3). Results: §7.
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

### 2.3 The merge of `b8ae9412b` (2026-10-10): what it adds to the class

`git diff 648a9f481..b8ae9412b` was enumerated the same way (every changed declaration in the wire directories against the frozen
manifest; every variant of `PalwConsensusObjectV2` against `0b1c11b87`'s source).

| Change | Lane | Below its fence | Disposition |
|---|---|---|---|
| ADR-0175: tags 27, 28, 29, 81 and 37 (granting `EARLY_VERSION`) recognized and refused by name past `palw_model_immutable_v1`; tags 3, 26, 39, 61, 68, 70, 91 and the epoch sweep folded differently past it | `pre` | int-12's rule; the walk's drop and every fold branch ask the fence (`model_immutable_active` is `false`, the line id is `model_line_id_v1`) | rows `Int12RefusedByName` / `Int12FoldPastFence` (§3.5); the pin carries all five refused kinds, node C arms the fence far above, and a past-fence test shows the block standing (§4.2) |
| `PalwPromotionOutcomeV1::CandidateSelected` (2) appended | `pre` | written only past the fence | `NotCarried` (an epoch row's decision) |
| `SigningPurpose::PalwReceiptAuthV4` (8), `SignerMessageDigest::PalwReceiptAuthV4` | SMALL (RDA4) | offered only under `palw_receipt_spend_v4` | `NotCarried` (the node ↔ signer protocol) |
| `PalwFpWorkerFrameV1::Cancelled` (8) | SMALL (RFC-0001 P1) | node-local | `NotCarried` (the worker ↔ gateway frame) |
| R-1 reporter share 1,000 → 4,900 bps, **unfenced**, and written into the params fingerprint under `palw_rcore_plus` | `pre` (ADR-0032 amendment) | **not fenced on `b8ae9412b`**: testnet-12 arms R-core+ at genesis, so a build of this line pays a different reporter share than int-12 on a conviction and moves the t12 params id | INTF moves it behind `palw_reporter_share_v2` (the Lead's plan); the table holds the row (`StateEncoding`, pending) so INTF's merge is held to it. Not a split int-12 can cause; a split this line would cause if deployed as is |
| SHARD2's part-C hold, X12N's live retention | SHARD, X12N | behind `palw_panel_unavailable_expiry` / DNS retirement, dormant on every preset | no row needed: the fences are older than the change and armed nowhere |

No variant int-12 decodes changed its declaration on this line, `g14/r4-fixes` or `adv/c4r4` (now pinned: §3.4).

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
* An object that needs TWO inner fences has a combined variant: `PalwKernelInnerFenceV1::PanelFreeAndTypedRootsV1` is in force only
  where `palw_panel_free_v1` and `palw_typed_roots_v1` both are — G14R's salted reveal (inner 20) of a typed-root claim
  (`SaltedCommitV1::Spec`, 19), answered by a guarded arm ahead of inner 20's own. (`g14/r4-fixes` judges it by hand-written gate
  arms; its merge moves them into the table, §8.)

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

* **The live build's variants themselves are pinned** (`PALW_A2_INT12_VARIANTS_WIRE_V1`): the manifest pins `PalwConsensusObjectV2`
  as a whole, which every new kind changes (`ObjectEnum`), so a field added inline to one of int-12's 100 variants would have passed.
  The pin is an FNV-1a over those 100 declarations; a change there is classified (an `Int12Inner` row), never re-pinned alone.

Known limits of the freeze, each covered by the int-12 replay instead: a change inside a helper function a hand-written decoder calls;
a `use` that rebinds a name to another type; a formula (a hash or a root) rather than a wire form.

The changes classified today are `PalwConsensusObjectV2` (`ObjectEnum`) and the `NotCarried` state, delta and parameter types the
manifest reports (§7).

### 3.5 The central kind → fence table

`PALW_A2_KIND_FENCE_TABLE_V1` (`palw_lifecycle_objects_v2.rs`) is the one table of every post-int-12 member, landed or allocated, and
the fence below which it rides unjudged. A2U keeps it; the Lead allocates; a lane fills its row's code at merge. Each row names its
fence as a `Params` field name, so a row can be written before its lane merges. An unallocated tag (112, 114–119, 121–129) has no
row, so a kind landing there fails the table test until the Lead allocates it. The Lead's tag allocations themselves are
`PALW_A2_TAG_ALLOCATIONS_V1` (104–109, 110–119, 120–129, 130–139, **140–149 BUDGET** with `palw_bond_budget_v1` /
`palw_model_bond_allocation_v1`, 150–153 DA16): every tag row lies inside one and names one of its fences, and so does every landed kind.

| Slot | Members | Fence | Lane | In this tree |
|---|---|---|---|---|
| object tags 104–107 | onboarding | `palw_probabilistic_constraints_v1` | G14 lane D | yes |
| object tag 108 | `SignedRegistrationV1` | `palw_signed_registration_v1` | RFC-0009 | yes |
| object tag 109 | `ConformanceEvidenceV1` | `palw_probabilistic_constraints_v1` | OB-P0 | yes |
| object tags 110–111 | 110 route, 111 receipt | `palw_probabilistic_constraints_v1` | G14 lane D | yes |
| object tag 113 | `KernelRouteChunkV1` (the route's own chunk lane) — the Lead, 2026-10-10 | `palw_probabilistic_constraints_v1` | G14-R4 (`g14/r4-fixes`, `adv/c4r4`) | no (row first; §8) |
| object tag 120 | `PanelBeaconProofV3` | `palw_permissionless_panel_v1` | RFC-0010 | yes |
| object tags 130–139 | 130 `ExecWorkRootOpenedV2` | `palw_exec_payload_v2` | X8R | no |
| object tags 140–149 | none yet — BUDGET adds one row per kind range, naming `palw_bond_budget_v1` or `palw_model_bond_allocation_v1`, in the commit that creates the kinds | (allocation) | BUDGET | no row |
| object tags 150–153 | `ProviderLeaseV1`, `ProviderChallengeV1`, `ProviderAnswerV1`, `DaTransferV1` | `palw_provider_court_v1` (in force only where the kernel route's fence is too) | DA16 | yes (merged with integration `648a9f481`); wire pinned |
| inner kinds 1–12 | the kernel route | `palw_probabilistic_constraints_v1` (tag 110's own) | G14 lane D | yes |
| inner kinds 13–14 | OPV registrations | `palw_panel_free_v1` | RFC-0015 | yes |
| inner kind 15 | `SealProof` | tag 110's own | G14-R4 | no |
| inner kinds 16–18 | segmented claim, tiled job, prompt tile | tag 110's own | K2S | no |
| inner kind 19 | `Spec` | `palw_typed_roots_v1` | R4X | yes |
| inner kind 20 | `CommitClaimSalted` (salted claim seal v2) | `palw_panel_free_v1` | G14R | no |
| nested in inner kinds | `ProsecutionV1::Segmented` (3), `ClaimBodyV1::Segmented` (2) | tag 110's own | K2S | no |
| nested in inner kinds | `ProsecutionV1::Spec` (4) inside `FileProof` (inner 7); `ClaimBodyV1::Spec` (3) is ledger state, never carried | `palw_typed_roots_v1` | R4X | yes (guarded arm) |
| nested in inner kinds | `SaltedCommitV1::Spec` (19) inside `CommitClaimSalted` (inner 20) | `palw_typed_roots_v1` AND `palw_panel_free_v1` (`PanelFreeAndTypedRootsV1`) | G14R × R4X | no (the fence variant is) |
| header form algo 7 `PFS4` | V4 receipt carriage | `palw_receipt_spend_v4` | RFC-0009 | yes |
| header form algo 10 `PXE2` | EXEC envelope | `palw_exec_payload_v2` | X8R | no |
| coinbase trailer `PXA2` | EXEC anchor | `palw_exec_payload_v2` | X8R | no |
| inside tag 68 (re-read) | `PalwGenProfileV1::Head = 6` | `palw_task_heads_v1` | HFX | yes (no guarded arm: int-12's own `Profile(6)` verdict, probed) |
| inside tag 68 (appended) | `PalwGenProfileOffersV1::Head` (3) | `palw_task_heads_v1` | HFX | yes (guarded arm; fence `TaskHeadsV1`) |
| inside a generative job (tensor commitment, court proofs, accusations; the 0x4a FP form) | `PalwGenBodyV1::Head` (2) | `palw_task_heads_v1` | HFX | yes (guarded arm; the FP form by `palw_fp_head_job_refusal_at_v1`) |
| header formula | `palw_state_root = H(fork-choice leaf ‖ ADR-0043 root)` past the fence | `palw_fork_choice_commitment_v1` | L2FC | no |
| state encoding | per-shard V3 draw (delta 171, tail `0xED`) | `palw_permissionless_panel_v1` | SHARD | yes |
| state encoding | per-segment pricing, the shard engine's encodings | `palw_tir_shard_segment_v2` | SHARD | yes |
| state encoding | bond budget engine Q/B/R/F (deltas 190–199, tail `0xEF`, root block `bond_budget/v1`) | `palw_bond_budget_v1` | BUDGET | no |
| state encoding | model coinbase by locked miner capital `f(S_m)` | `palw_model_bond_allocation_v1` | BUDGET | no |
| state encoding | R-1 reporter share and DA-6 exposure: 1,000 bps below, 4,900 past | `palw_reporter_share_v2` | INTF | no (**unfenced on `b8ae9412b`**, §2.3) |
| int-12 kinds refused by name past the fence | 27, 28, 29, 81; 37 granting `EARLY_VERSION` | `palw_model_immutable_v1` | ADR-0175 (`pre`) | yes |
| int-12 kinds folded anew past the fence | 3, 26, 39, 61, 68, 70, 91 and the epoch sweep | `palw_model_immutable_v1` | ADR-0175 (`pre`) | yes |

**The verdict int-12 gives depends on the carriage**, and "below the fence" means that verdict:

* 0x4b lifecycle payloads: undecodable is **tolerated** (testnet-12 declares the audit fence), the carrier skipped.
* 0x4a free-prompt jobs: undecodable is **refused** — there is no audit tolerance on 0x4a. A job form a lane adds (HFX's Head body)
  must be refused below its fence exactly as int-12 refuses it: the form passes the isolation door only where the ruleset carries the
  fence, and the header-context door below its height (the pattern of `palw_gen_door` and the decode-rules door).
* Header carriage forms: **refused** by int-12's shape gate (§3.2).
* Coinbase trailers: **opaque** miner bytes to int-12 — nothing read, nothing refused (X8R's reader is lenient only where the fence is
  armed, and the length rule is the body stage's at the height).
* Formulas and state encodings: **byte-identical** below the fence, block by block (the int-12 replay checks every block's root).
* Kinds int-12 decodes, judged anew past a fence (ADR-0175): **int-12's own rule** below it. Past it a recognized object is refused by
  name in the acceptance walk (not applied, nothing charged) and the carrying block stands — never an isolation or header-context
  refusal, which would turn a statement int-12 accepts into an invalid block at the fence for every peer that has not yet seen the
  object's meaning change. `palw_int12_kind_refused_past_fence_v1` is the policy the rows are reconciled with.

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
* **`every_tag_row_is_inside_its_allocation`** holds every tag row and every landed kind to the Lead's allocation and its fences
  (`PALW_A2_TAG_ALLOCATIONS_V1`).
* **`the_int12_kinds_refused_past_a_fence_are_the_rows`** asks every int-12 kind (near-zero, and tag 37 granting `EARLY_VERSION`) of
  `palw_int12_kind_refused_past_fence_v1` and demands exactly the `Int12RefusedByName` rows; every `Int12*` row names int-12 tags and
  a `Params` fence; no new kind is refused this way.
* **`every_landed_kind_is_pinned_to_its_wire_form`** pins each landed post-int-12 kind's wire form (its variant and every type it
  reaches by name: `PALW_A2_NEW_KIND_WIRE_V1`) and the live build's 100 variants (`PALW_A2_INT12_VARIANTS_WIRE_V1`). A lane that
  creates or changes a kind re-pins it in the same commit — and confirms the change rides under its row's fence; the failure prints
  the current pins.

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
* **ADR-0175's five refused kinds**, signed (27, 28, 29, 81, and 37 granting `EARLY_VERSION`): int-12 decodes and judges them;
* **every kernel-route inner kind inside tag 110**, generically over `PALW_KERNEL_ROUTE_INNER_KINDS_V1` (an inner kind a lane adds —
  15, 16–18, 20 — is carried the moment its row exists);
* three one-chunk groups of new kinds, and a two-chunk group that completes into a new kind;
* **a mixed block**: a quorum's `ReceiptLicensed` (an int-12 kind that folds and licenses a real claim), the same licence again (an
  int-12 kind the walk refuses), and new kinds riding unjudged.

It asserts each carrier is in its block and each block is valid; the licence licenses; there is no kernel-route state, no Panel V3
state and no completed group; the walk's chunk reader is "undecodable" below the fence and the kind past it; a `PFS4` header is
refused on the header path **and** the pruning-proof path with a refusal int-12's own gate gives, and differently on a node where the
form is in force. An unarmed node and a node with every owning fence armed far above the chain (armed through exhaustive matches over
every fence list, so a new fence must be armed there to compile, and through every LANDED row's fence by name — `palw_tir_shard_segment_v2`,
`palw_model_immutable_v1` with the model lines it needs — so a row that lands with a fence the harness does not arm panics) then replay
every block: the same statuses, refusals, PALW state root **at every block**, sink, UTXO multiset and pruning-proof refusal. That is
the mixed-verdict pin for ADR-0175 too: an old build (unarmed) and a new build (immutable armed) agree on every block below the fence.

`t12_a2u_adr0175_past_its_fence_a_definition_update_is_refused_and_its_block_stands` is the other side: with
`palw_model_immutable_v1` (and model lines) at DAA 1 and its own validation passing, the five refused kinds ride in valid blocks, each
carrier in its block, and no model line is written.

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
7. The live-build lists (`PALW_LIFECYCLE_INT12_KINDS_V1`, the manifest, `PALW_A2_INT12_VARIANTS_WIRE_V1`) are frozen. They change
   only when a new release becomes the live baseline, in their own reviewed commit.
8. **Allocations are checked.** A tag row lies inside the Lead's allocation (`PALW_A2_TAG_ALLOCATIONS_V1`) and names one of its
   fences; a new allocation is a Lead commit to that table and the registry together.
9. **A landed kind's wire form is pinned** (`PALW_A2_NEW_KIND_WIRE_V1`). Creating a kind adds its pin; changing one — a field, a nested
   variant, a type it reaches — re-pins it in the same commit, and the change rides under the row's fence or gets a row of its own.
10. **A kind int-12 decodes, judged anew past a fence** (ADR-0175's pattern) gets an `Int12RefusedByName` row (refused at acceptance
   by name: reconciled with `palw_int12_kind_refused_past_fence_v1`) or an `Int12FoldPastFence` row (the fold, by state), and its
   fence is armed in the pin test's node C. Past the fence the object is not applied and the block stands; never an isolation or
   header-context refusal of a kind int-12 accepts.

**The procedure for lane BUDGET (tags 140–149, ADR-0176/0177).** In the commit that creates each kind: the variant at its tag, an arm
in `palw_lifecycle_kind_owner_v1` (a new `PalwLifecycleKindFenceV1` variant per fence — `BondBudgetV1` → `palw_bond_budget_v1`,
`ModelBondAllocationV1` → `palw_model_bond_allocation_v1` — with its `params_field`, its resolution in
`Params::palw_lifecycle_kind_fences_v1`, its arm in `palw_fold_kind_in_force_v1` and in the pin test's `arm_every_owning_fence`),
its `PALW_LIFECYCLE_NEW_KINDS_V1` entry, an `ObjectTags` row inside 140–149 naming the fence (landed), its `PALW_A2_NEW_KIND_WIRE_V1`
pin, and the `StateEncoding` rows (deltas 190–199, tail `0xEF`, the coinbase allocation) flipped to landed. Fails without them:
`every_kind_in_the_enum_has_exactly_one_owner` (no NEW_KINDS entry), `every_landed_kind_sits_in_its_row_with_the_rows_fence` (no row,
or the row's fence differs), `every_tag_row_is_inside_its_allocation` (a fence outside BUDGET's two), `every_landed_kind_is_pinned_to_
its_wire_form` (no pin), the pin test (an unarmed fence).

**The procedure for lane DA16's re-scope (150–153: the `Artifact` lease subject removed under `palw_provider_court_v1`).** The change
is a wire change to landed kinds: the same commit re-pins 150–153 in `PALW_A2_NEW_KIND_WIRE_V1` (the test prints the values) and keeps
the row's fence; if any part of the change rides another fence (say ADR-0177's), that part gets its own row and fence variant, and the
kinds' owner arm answers it. Without the re-pin `every_landed_kind_is_pinned_to_its_wire_form` fails.

**Worked example — merging G14-R4's tag 113 (`KernelRouteChunkV1`, in `g14/r4-fixes` and `adv/c4r4`).** On those branches, without
this rule, isolation runs its may-ride arm (unsigned, or an index, count or part out of range → `ObjectMayNotRide`) at every height, so
the upgraded node marks invalid a block int-12 tolerates — the split C4R4's `c4r4_a2_tag113` pins. The exact list is §8.

## 7. Results

Status vocabulary: **implemented** / **verified** / **armable** (user, 2026-10-09). Nothing here is armable; no fence is armed.

**2026-10-10 (successor), over integration `b8ae9412b`, `scratchpad` milestones m5–m6** — every invocation through `buildslot.sh`:

| What | Result | Level |
|---|---|---|
| Core A2U, lifecycle, `pow_layer0` and `immutable_models` tests (`palw_lifecycle_objects_v2:: pow_layer0:: immutable_models`) — incl. the new `every_tag_row_is_inside_its_allocation`, `the_int12_kinds_refused_past_a_fence_are_the_rows`, `every_landed_kind_is_pinned_to_its_wire_form`, the four new `NotCarried` classifications, `pre`'s own ADR-0175 tests | 68 passed, 0 failed | verified |
| Wire pins bootstrapped once from the test itself (placeholders only; `PALW_A2_INT12_VARIANTS_WIRE_V1 = 0xd1982421da455cca`, 13 landed kinds), int-12's 100 variants first checked equal to `0b1c11b87`'s source | pinned | verified |
| Pin test, launch ruleset: 49 chain blocks to DAA 22, 71 carriers (every new kind near-zero and hand-built, every malformed form, the reference, the re-read probe, ADR-0175's five refused kinds, every kernel inner kind inside tag 110, chunk groups, a founded line and its published version applied, the mixed block), 1 refused `PFS4` header; unarmed and armed-far nodes agree at every block | ok | verified |
| Pin test, release ruleset: 704 chain blocks to DAA 351, 69 carriers, the same agreement | ok | verified |
| ADR-0175 past its fence: the live rule applies the signed version (2 versions); with `palw_model_immutable_v1` the same publication is refused (1 version); the carrying block (6 definition updates) stands in both | ok | verified |
| Replay through int-12 itself (`0b1c11b87`'s own tree and target): launch 49 blocks, release 704 blocks — every verdict, root and refusal is the live build's | ok | verified |

A harness finding on the way, not a consensus one: arming `palw_model_lines` at node C's far height DISARMED testnet-12's live
registry below it, and node C disqualified the block whose founded line the live rule had applied — the pin caught it. Node C now
leaves a fence the ruleset already runs untouched.

Rows awaiting their lanes (pending, by design): tag 113 / inner 15 / inner 20 / the salted-`Spec` nested row (G14R, §8), 16–18 and the
`Segmented` nested row (K2S), 130–139 / `PXE2` / `PXA2` (X8R), L2FC's formula, BUDGET's two encodings, INTF's
reporter share.

## 8. Merging `g14/r4-fixes` and `adv/c4r4` with this branch (the Lead, 2026-10-10)

Read at `g14/r4-fixes` @ `985463448` and `adv/c4r4` @ `162df6b16` (which contains all of `g14/r4-fixes`). A trial `git merge-tree` of
this branch with either has ONE textual conflict (the kernel gate, item 4). Line numbers: `L` = this branch, `R4` = `g14/r4-fixes`,
`C4` = `adv/c4r4`. Items 1–6 are needed by BOTH branches (adv/c4r4 inherits them through g14/r4-fixes); 7–8 are adv/c4r4's own.

1. **Tag 113 owner** — `L consensus/core/src/palw_lifecycle_objects_v2.rs:1232`: add `| O::KernelRouteChunkV1 { .. }` to the
   `ProbabilisticConstraintsV1` arm of `palw_lifecycle_kind_owner_v1` (the compiler demands it: the match is exhaustive). The variant
   is `R4 consensus/core/src/palw_state_v2.rs:8473` / `C4 :8532`.
2. **Tag 113 tables** — `L …palw_lifecycle_objects_v2.rs:1360`: after the 111 entry add
   `(113, "KernelRouteChunkV1", PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1)` to `PALW_LIFECYCLE_NEW_KINDS_V1`;
   `L …:1752`: flip the tag-113 row's `landed` to `true`. (`every_kind_in_the_enum_has_exactly_one_owner`,
   `every_landed_kind_sits_in_its_row_with_the_rows_fence`.)
3. **Inner kinds 15 and 20** — `L …:1542`: add `| K::SealProof { .. }` to the `None` arm of `palw_kernel_route_inner_fence_v1`;
   `L …:1530` (the guarded arms, ahead of the kinds' own): add
   `K::CommitClaimSalted { commit: misaka_palw_kernel::route::SaltedCommitV1::Spec { .. }, .. } => Some(PalwKernelInnerFenceV1::PanelFreeAndTypedRootsV1),`
   and, at `L …:1543`, `K::CommitClaimSalted { .. } => Some(PalwKernelInnerFenceV1::PanelFreeV1)`. `L …:1563`: add the rows
   `(15, "SealProof", None)` and `(20, "CommitClaimSalted", Some(PalwKernelInnerFenceV1::PanelFreeV1))` to
   `PALW_KERNEL_ROUTE_INNER_KINDS_V1`. Flip to landed: `L …:1767` (inner 15), `L …:1777` (inner 20), `L …:1794` (the nested
   `SaltedCommitV1::Spec` row). (`every_kernel_inner_kind_has_exactly_one_row`, the compiler.)
4. **The kernel gate (the conflict)** — `consensus/src/pipeline/virtual_processor/processor.rs`, `palw_kernel_route_object_is_signed`:
   keep THIS branch's side (`L :13657`ff., the table-driven `palw_kernel_route_inner_fence_v1` / `palw_kernel_inner_fence_at` check);
   DROP the hand-written arms `R4 :13711–13742` / `C4 :13734–13765` (inner 13/14, inner 20 `CommitClaimSalted`, and the `typed`
   check for `Spec` and `SaltedCommitV1::Spec`). They are the table's arms of item 3, with the same fences.
5. **Wire pins** — after items 1–3, run `every_landed_kind_is_pinned_to_its_wire_form`: add the printed `(113, 0x…)` pin to
   `PALW_A2_NEW_KIND_WIRE_V1` (`L …palw_lifecycle_objects_v2.rs:3947`, in the A2U test module) and re-pin
   every landed kind whose reachable types the branch changed (it prints the full list). On `adv/c4r4` that includes the onboarding
   kinds whose types OPVB changed (`ConformanceEvidenceActionV1`, `FreshInputV1`, `ApprovedTupleV1`, …: tags 107/109); each change
   is under `palw_probabilistic_constraints_v1`, the rows' fence, so a re-pin is the whole fix. `PALW_A2_INT12_VARIANTS_WIRE_V1` must
   NOT move (checked: no int-12 variant differs on either branch).
6. **No other site judges tag 113 below its fence once item 1 lands**: its may-ride arm (`R4/C4 …palw_lifecycle_objects_v2.rs:344`)
   then runs only in the header context past the fence; the objects-of-block walk skips it below, so the chunk gate
   (`R4 processor.rs:13227` / `C4 :13250`), the chunk lane's rows and deposit (`palw_kernel_route_v1.rs`) and the fold arm
   (`R4 palw_state_v2.rs:34832` / `C4 :34958`) are reached only past it; `palw_object_is_kernel_route_v1` (`palw_state_v2.rs:8610`)
   stays the second lock. The heartbeat-carrier classification (`palw_heartbeat_carriers_v1.rs:201`) is a mempool/template read.
7. **adv/c4r4 only** — `C4 consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/conformance/c4r4.rs:160`: remove the
   `#[ignore = "FAIL C4R4 A-2: …"]` on `g14_c4r4_a2_a_tag_113_carrier_below_its_fence_is_judged_as_undecodable_bytes`; with item 1 it
   passes (isolation tolerates 113 at every height on an audit ruleset).
8. **adv/c4r4 only — INTF's `palw_reporter_share_v2`** (merged into it): `L …palw_lifecycle_objects_v2.rs:1859` flip the reporter-share
   `StateEncoding` row to landed, and arm the fence in the pin test's node C —
   `L consensus/src/pipeline/virtual_processor/tests/t12_a2u_new_kinds_uniform.rs:157`, beside `palw_tir_shard_segment_v2`:
   `"palw_reporter_share_v2" => params.palw_reporter_share_v2 = Some(at),` (the harness panics on a landed row's unknown fence). Its
   change to `PalwStateParamsV2` is already classified `NotCarried`.
9. **adv/c4r4 only** — nothing else: its other additions (OPVB's beacon v3, E1–E7, the bootstrap types) add no object tag, inner
   kind or header form, and change no other int-12 wire type.

After the merge: the A2U core tests, the pin test on both rulesets, and the int-12 replay of the dumps (§5) — the merge gate (§6.6).
