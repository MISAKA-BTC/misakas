# ADR-0163 — An adapter class is its parent plus an adapter, and is listed

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-10-03 (RFC-0001 §2.10, lane U); **IMPLEMENTED the same day on `rfc1/serve`**, behind its own
  dormant fence `Params::palw_adapter_class_v1` — `None` on every preset and in no testnet-12 flag-day list, so no network's
  fingerprint, schedule or fold moves.
* Builds on: RFC-0004 §6.3 (composite artifacts, PALW-MIP-15; `palw_improve_composite_v1`), RFC-0002 (the IR class, its
  registration and its court), ADR-0075 (a class is a consensus object).
* Supersedes nothing.

## 0. The sentence this ADR is

A LoRA is not a new model: a class that is exactly its parent's inventory plus an adapter section is registered the way any IR class
is, and is then **listed** — one signed object that records the composite reference in the registry the court's composite openings
and a seat's possession proof both read — so an adapter costs a section, not a re-commitment of the model.

## 1. What exists, and the gap

RFC-0004 already built the composite: `artifact_root(Composite) = H(parent_class ‖ parent_root ‖ adapter_root ‖ le32(P))`, a candidate
lowered with its adapter's params last (`misaka-palw-tir-lower/src/lora.rs`, `lower::adapter_params_last`), its admission
(`palw_tir_candidate_artifact_admits_v1`: the family rule, the composite rule, every terminal close carriable in the composite form) and
the registry `improvement_composite_classes` (delta 99) that a court opening and a seat's adapter-section possession proof read. But the
only door to that registry is `CandidateSubmitted` (tag 80): a governed model line's epoch, its fees, its bar and its pool. A person who
wants to serve a LoRA over a registered class is not entering a competition; they have no line, and should not need one.

## 2. Decisions

1. **A listing object, tag 94: `AdapterClassListed { payload: PalwAdapterClassListingV1 { class_id, artifact: PalwTirCompositeRefV1,
   layout }, lister: PalwBondKeyV2, signature }`.** The class is a registered IR class (its `ClassRegisteredTirV1` was priced and
   admitted as any registration is, with the composite root as its artifact root); the listing adds the reference. Tag 94 is declared
   explicitly (spec 17 §17.0: a lane asks the core lane before taking a tag; 91–93 are RFC-0006's and RFC-0007's, in flight on their
   branches, and 94 is the next one free after them. If the core lane assigns it differently the number is the only thing that moves).
2. **Acceptance is the composite candidate's, without the line.** The lister's signature (ML-DSA-87, context
   `misaka-palw-adapter-class-v1`, over the network domain, the payload and the lister — so it is neither replayable across networks nor
   liftable onto another bond); then `palw_tir_candidate_artifact_admits_v1` over the class's record and the parent's, with full weights
   NOT allowed (a listing is for adapters; a full-weight class is a registration). It sizes an IR program, so it shares the block's one
   place at admission sizing with IR registrations and composite candidates (the walk drops a second by name, the block standing).
3. **The fold is the cheap second lock, and writes one row.** Before the write, every refusal: the lister an Active bond; the class
   registered and IR; the reference's artifact root the class's registered one; the parent a registered IR class whose registered root is the
   reference's `parent_root`; `p > 0` and the class not its own parent; the class not already listed. The write is
   `write_improvement_composite_class` — the one writer of the registry, journaled `ImprovementCompositeClass` (99), so replay and revert
   already cover it. Nothing else moves: no bond is locked, no pool is touched, no reward exists. Spam control is the registration's own cost
   (a class must be registered, and a registration is priced), plus one row per class and the sizing slot.
4. **Seat possession is the existing record.** A seat proves possession of a composite over its adapter section against the root this
   registry names (RFC-0004 §6.7, `improvement_composite_classes_v1`, the node lane's read); the parent's section is the parent's
   inventory, which the seat already holds. Listing therefore makes the adapter class a class seats can be asked about, and nothing in
   readiness changes: a derived class is a class with a possession proof over two sections.
5. **Dormant behind `palw_adapter_class_v1`.** Prerequisites, refused by name by `validate_palw_v2`: `palw_tir_v1` and
   `palw_improvement_v1` in force at or below it (the machinery lives behind both); a ConsensusV2 network; the bundle's mirror equal to the
   fence (`sync_palw_adapter_class_v1`, as every fold-read fence). Below it the acceptance walk drops the object by name and the fold refuses
   it (second lock). Written Some-only into the params and schedule ids, collapsed from `Some(never())`, named in `palw_fences_v1` and the
   fork-id probe; armed only by a salted drill entry (`--palw-drill-adapter-at`).

## 3. What this does not do

* It does not make an adapter servable: the node's worker must hold the parent and the adapter section and run the composite program, and
  the court's composite opening needs `palw_improvement_v1` armed. This ADR is the registry and the gate.
* It does not price an adapter class differently from its registration, and it adds no reward to the lister.
* A governed line's candidate still enters the registry through tag 80 and is untouched.

## 4. Evidence

`consensus/core/src/palw_adapter_class_v1.rs` (payload, message, form), `palw_adapter_class_fold_v1.rs` (the fold and its refusals, replayable
through the delta), `consensus/core/tests/palw_rfc1_fences.rs` (dormancy, Some-only fingerprint, `never()` collapse, prerequisites by name,
in no t12 list), and `t12_an_adapter_class_listing_is_gated_by_its_fence_its_signature_and_its_acceptance` at the processor (below the fence
dropped by name; armed, a stranger's signature refused, a signed listing of an unregistered class refused by the acceptance half). The
composite admission itself is RFC-0004's, exercised on real PEFT fixtures in `misaka-palw-sdk/tests/improve_composite_lora.rs`.

## Mission alignment amendment — 2026-10-07

adapter classはparentに加えadapter weights/versionを公開認証し、adapterによる誤計算も外部bondがlocalize/convictできるprofileを必要とする。parent certificateだけでadapter部分の新gateを免除しない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
