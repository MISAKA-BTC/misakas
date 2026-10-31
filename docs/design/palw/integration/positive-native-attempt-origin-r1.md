# Positive Court r1: native own-template origin annex

Status: private implementation annex / unresolved header-work projection. This
annex implements the new claim-version separation required by
[RFC-0044](../../../rfc/0044-positive-court-full-coverage.md) and
[ADR-0212](../../../adr/0212-positive-court-without-model-prepossession.md).
It does not activate a header form, alter old envelope meanings, adopt new
issuance or declare native readiness. The governing
[principles](../../../PRINCIPLES.md) continue to apply.

## Origin choice and acyclic ordering

The new lane retains the ordinary producer's **own-template** job derivation.
An arbitrary past header is not a substitute job assignment. A prospective
new lightweight post-PoW Notice carries the literal r1 Miner Result, its
signature, producer bond/key, a layout preimage and a signed header-position
binding. It carries no full trace, step tree or Panel evidence. The Node does
not infer computation correctness from this Notice.

The construction order is:

1. Resolve an independent selected-parent ancestry anchor and the real native
   state for the proposed origin. Build the ordinary transaction template.
2. Compute its existing pre-PoW hash. Derive the canonical job from registered
   TIR facts, class, producer bond and the ordinary nonce bucket. Derive prompt
   and seed by the existing functions, without a peer job envelope.
3. Derive source/quote requirements from that job and the adopted policy's
   actual preimages. Obtain payer consent for the acyclic funding projection.
   The requirements describe future reservations; an actual lease ID is not
   a pre-consent input.
4. Run ordinary inference and sign the literal r1 Result. Construct/sign the
   new Notice and perform the unchanged required base PoW / eligibility
   checks, subject to the new form's unresolved work-projection contract below.
5. Insert the Notice into `Header::palw_commitment`, which is excluded from the
   pre-PoW digest. Receive an accepted origin header and record Pending only.
6. A later native body0, or centrally assigned additional local operation,
   references that accepted `origin_block` and the same literal ResultId. Its
   transaction is absent from the origin template's transaction Merkle root.
   At this step the native owner acquires actual funding/resource leases and
   atomically admits the case and response contract.
7. Full Node Court checking, the new P-subject selected-verifier certification
   and the applicable finality/window predicates remain independently
   necessary before inference reward or credit. Pending is never credited.

`result.scope.branch` names the independently resolved ancestry anchor. It
must not be the Notice's own origin hash. The latter depends on the Notice.
Funding basis/catalog/quote/schedule preimages must not contain the final
origin hash, ResultId, carrier TxID or actual admission lease ID. The existing
funding projection continues to zero only `result.service_contract` and
`service.funding_consent`; no additional r1 field is silently omitted.

## Existing production derivation

The actual source path is `kaspad/src/palw_producer.rs:1789` (template and nonce
bucket), `:1865` (`backend.job_for_anchor`), then
`misaka/palw/tir/exec/src/node/backend.rs:1602` and
`consensus/core/src/palw_tir_attempt_v1.rs:184`. A verifier independently reads
the origin header in `kaspad/src/palw_panel.rs:7645` and `:7664`.

The native owner reuses these rules, rather than accepting their outputs from
the Result:

| Quantity | Existing derivation / native source |
| --- | --- |
| Network domain | `palw_network_domain_v2_for(network_id, Some(genesis.hash))` |
| Origin pre-PoW hash | `hashing::header::pre_pow_hash_64(actual_origin_header)` |
| Nonce bucket | `palw_nonce_bucket_v1(origin.nonce)`; current shift is 22 |
| Job anchor | `palw_job_anchor_v1(network, pre_pow, class, producer_outpoint, bucket)` |
| Canonical counts | `palw_tir_attempt_canonical_of_v1(registered.facts.max_context)` |
| Prompt, commitment and seed | `palw_tir_attempt_job_for_anchor_of_v1(registered.facts, anchor, canonical, native_prompt_form)` |
| Prefill draw | `palw_attempt_job_v1(context, Params::palw_prefill_draw_active_at(origin.daa_score))` |
| Job identity | Existing derived `PalwJobContextV2::job_id` |
| Execution identity | Existing derived `PalwJobContextV2::context_hash()`; never a Panel execution root |
| Input identity | Existing derived `context.prompt_token_ids_hash` |
| Result tokenizer | **Existing derived `context.tokenizer_id`, including canonical-attempt zero** |
| Registered tokenizer | Separately authenticated `state.tir_class_v1(class).tokenizer_id` and class ID |

The existing TIR attempt yardstick intentionally has zero tokenizer, model,
runtime and CU identities (`palw_tir_attempt_v1.rs:98`). The r1 Result/API
contract is unchanged: `result.tokenizer == job.tokenizer_id`. Registered
tokenizer must not be substituted into that Result field. The native metadata
record authenticates the registered tokenizer separately. The existing
free-prompt public-binding wrapper requires equality with the registered
tokenizer and cannot be reused for an attempt context without a distinct
attempt-origin adapter.

The generic `PalwJobEnvelopeV2::from_envelope` path is not native assignment
authority: that envelope still permits supervisor-chosen identities and seed.
Owning39 mining consumes a complete Master Final
(`palw_native_mining_fold_v39.rs:428`); it is not an origin factory for a new
trace-free Result. The old staged original / public free-prompt stores contain
the old trace-bearing commitment and are not new job authority.

## State at origin and at later admission

The native header owner must resolve its real selected-parent/current accepted
prefix state. A peer's state copy, a caller boolean or a historical row of the
same ClaimId does not replace this provenance.

At the origin, require the actual registered class and TIR record to exist,
registration to precede the origin, and all existing attempt eligibility,
class/budget, bond-key/status/collateral/maturity and PoW requirements to hold
at that origin. Resolve the producer key from the native bond; check the
carried key equals it and verify both Result and Notice-position signatures.
`palw_bond_may_take_work_v2` is only one existing predicate, not a complete
eligibility implementation. The origin's class/weight root, graph root,
layout digest and canonical job must all be consistent. This checks registered
metadata identity; it is not model review or a model-tier promotion.

The bounded layout preimage authenticates against the existing rooted
`layout_digest`; the registered program remains in the existing Arc-backed
`tir_classes` row and is not duplicated in either Notice or body0. The
read-only `palw_positive_metadata_v2.rs` resolver performs these byte/identity
checks without granting native admission.

At body admission, independently locate the already accepted Notice and its
native ancestry/origin state. Require exact Result bytes/ResultId, bond, class,
layout identity, derived job/context/input/seed/counts and adopted rule scope.
Revalidate current prefix generation, no previous Result for this execution,
case/all-attempt budgets, actual funding output and actual reservations in one
native checkpoint. A later frozen class or retiring bond must not erase a
previously accepted response duty or turn Panel failure into Miner fraud;
new eligibility and historical obligations have their own clocks. Missing
origin state/material must remain unavailable, never replaced by peer facts.

There is no production new-P RolePools/provider-consent catalog in the current
base. The native bond table supplies keys/status only; old claim-keyed Panel
assignments do not authorize a new P claim. `ProviderSubjectV1::Artifact` is
withdrawn and cannot be used as an operand-availability requirement. The old
KernelClaim provider lease cannot be reinterpreted as a new-P supply promise.
The production old escrow schedule resolver returns `None`, and the old
aggregate `reservation_readiness_v1` is still `Err`; neither is a successful
new-P lease. Actual signed consents, canonical schedule preimages and the
existing native resource owner must supply the remaining admission contracts.

## Private payload codec and identity

`palw_positive_notice_v2.rs` defines only the bounded payload, without native
magic, algorithm ID, global tag, domain or activation:

| Field order | Encoding |
| --- | --- |
| version, revision | u16 LE, u16 LE; 2 / 1 |
| header-position challenge | Hash64; existing `challenge_v2` recipe |
| producer bond | native outpoint: 64-byte TxID + u32 index |
| producer key | 2592 bytes |
| literal r1 Result | canonical fixed Borsh, 1076 bytes |
| layout length, layout | u32 LE + canonical Borsh layout |
| Result signature | 4627 bytes, existing Result signing context |
| Notice-position signature | 4627 bytes; new centrally allocated context |

The fixed overhead is 13062 bytes plus layout bytes. The layout has 22 fixed
bytes plus four bytes per combined commit/state entry. Scan total frame cap,
layout byte cap, both vector counts, checked lengths and exact EOF before
allocating either vector. Decode the fixed Result without dynamic allocation.
The caller acquires the actual native reservation before the explicit owned
layout decode. Malformed transport is not accepted Miner misconduct.

The preferred new-form block identity is a domain-separated digest of the
whole canonical unsigned Notice payload. Both signatures are authentication
witnesses, excluded from this identity. A position signature signs the new
unsigned digest including the existing header-position challenge, literal
Result, bond/key and layout. The separate Result signature continues to sign
the existing r1 ResultId. Do not make the position signature's message depend
on either signature or the final origin hash.

Current `hashing/header.rs:155` hashes raw `palw_commitment` bytes into block
identity. Its existing PAV2/PAV3/PBC1 behavior stays unchanged. The preferred
new normalization requires a strictly new-form-only hashing arm, strict
bounded framing, signature checks at relay and native admission, and matching
header-form/fence/pruning/sync handling. Changing existing forms' raw hashing
would change old identities and is prohibited here. Requiring a deterministic
signer implementation is insufficient: a validator cannot establish signature
uniqueness from public verification alone.

## Unresolved work projection: no permissive implementation

There is a real contract delta, rather than a codec detail:

- `execution_commitment_v3` hashes the entire old unsigned attempt with only
  `challenge` zeroed. Its old trace, execution and manifest roots are mandatory.
  `class_ticket_v3` and the L1 tag consume this commitment. A no-trace Notice
  cannot be cast into the old attempt or filled with zero/dummy roots.
- Replacing that hash with the whole new Result/Notice permits free
  `service_contract`/payer-consent/fee-term changes to redraw a ticket for the
  same correct inference. Those fields are not computational outputs.
- Excluding those fields from a new computation-only ticket while retaining
  them in unsigned header identity admits multiple distinct header identities
  for one solved position unless an additional canonical binding rule closes
  that difference. A single case budget prevents repeated credit but does not
  itself close header relay malleability or define a class lottery.

Thus the new header-work projection, exact anti-grinding/canonical binding
condition and linkage to existing class eligibility require an explicit
reviewed formal contract before a successful live header decoder/finalizer is
installed. This annex does not choose new issuance, weaken base PoW, allow a
notice to bypass class eligibility, or claim that Pending is a valid mining
claim. The payload codec and metadata resolver remain useful independent
components; all released Params remain `None` and readiness remains closed.

## Validation scope

The new payload is private/unregistered. Unit tests cover fixed Result width,
borrowed layout framing, truncation/extra bytes, hostile lengths and unsigned
bytes independent of signature variation. They do not establish native
header acceptance, live funding, selected-verifier certification or credit.
No full core build or public activation is performed by this annex task.

## Alternative under review: Panel-owned roots, unchanged ordinary PAV2 price

This alternative resolves the missing-old-root **price calculation** without
requiring the Miner to construct a trace. It is not yet a successful header
route. The primitive r1 Result remains unchanged and root-free. A Panel runs
the same registered program/job, constructs the existing TIR record, and signs
its Witness. The Miner receives the small root metadata and authenticated
reference; receiving it does not transfer ownership of record construction or
Court evidence to the Miner. Its outer Notice signature authenticates public
references and position, not a statement that it built the Panel's record.

The exact old `PalwAttemptUnsignedV2` has the following fields. It contains no
Panel author, signature, provenance, WitnessId, ResultId or service contract.

| Old projection field | New alternative source |
| --- | --- |
| version | Existing attempt-lane version resolved at the origin; no arbitrary v6 substitution for historical lanes |
| network_domain | Native network/genesis derivation |
| challenge | Existing `challenge_v2` over actual header position/class/bond; zeroed by the old price function |
| class_id | Native registered class |
| executor_bond | Native Miner bond |
| executor_pubkey | Exact registered Miner key, with the existing Borsh Vec length |
| operator_id | Native keyed/length-prefixed `palw_operator_id_v2(key)` |
| artifact_root | Actual registered artifact root |
| trace_root | Panel record `full_logits_trace_root` |
| output_root | Existing `palw_attempt_output_root_v1(native_job, unchanged_generated_ids)` |
| pwu | Existing actual origin class-target/work derivation |
| trace_manifest_root | Existing constructor's exact manifest rule, not a freely selected root |
| trace_chunk_count | Existing constructor's native witness-profile rule |
| trace_retention_daa | Exact old numeric projection `origin.daa.saturating_add(native_min_retention)` |
| execution_root | Panel record `committed_execution_root` |

Use `palw_attempt_from_execution_v1` as the price adapter, without constructing
or accepting an old signed envelope. `node/backend.rs:773` supplies precisely
these roots in the ordinary TIR execution path. The Panel encoder in
`node/positive_panel.rs:202` preserves the same capture roots. A Node must
reconstruct/check the binding from native class/job and later check every
mandatory relation. Matching roots and a Panel signature alone are not
computation correctness or credit.

When `facts.witness_chunks == 0`, the old TIR outcome uses the v1 manifest with
one chunk. When it is nonzero, the constructor uses the **v2 mesh domain** and
`1 + facts.witness_chunks`. Replacing that with a v1 manifest is a concrete
counterexample: same execution roots, different old priced commitment and
ticket. The numeric retention/chunk fields here are price-compatibility data;
they do not silently impose the old producer's trace-service obligation on the
new Miner. New response/Panel duties must use their explicit new version.

With exactly equal native facts, job and computational roots, the whole
unsigned projection is byte-identical. Therefore the existing
`execution_commitment_v3`, `class_ticket_v3` and `l1_tag_v2` outputs are equal.
Neither a full Result ID, fees, consent, Panel key/signature, provenance nor a
Witness attempt is added to those bytes. False roots still move the draw, as
they do on the old route; Court checks and the old eligibility/exposure
conditions remain necessary. No additional fraud conclusion is assigned to
the Miner merely because a Panel constructed false intermediate data.

This equality covers **ordinary PAV2 pricing only**. Existing PAV3 is different:
`PalwAttemptMaterialEnvelopeV3::priced_execution` additionally hashes Ready
session/consent/provider roster, and its class-ticket function uses a different
domain (`palw_attempt_material_v3.rs:205`, `:222`; the finalizer consumes this
at `consensus/pow/src/lib.rs:458`). Preserving those PAV3 outputs while excluding
these fields is not established. This alternative must not silently downgrade
PAV3's custody/eligibility requirements to ordinary PAV2.

An independent Python serializer and fixed vectors are in
[the verifier](../../../../scripts/verify/positive-notice-projection-v2.py) and
[the vector artifact](../../../evidence/positive-court-a0-2026-10-31/integration/panel-root-pav2-projection-vectors.json).
It checks the actual Rust field order/domain spellings, exercises both witness
branches and performs 32 assertions. Synthetic roots make this a
serialization/hash check, not an LLM execution test. The corresponding Rust
unit test calls the actual existing constructor/hash/ticket/tag functions and
checks those fixed values when the private codec module is registered.

### Remaining identity and two-phase conditions

Pricing equality does not by itself solve header identity. If two Notice
payloads differ only in a post-PoW funding term or Panel/Witness reference, the
old computational projection gives the same ticket and tag. Hashing both full
payloads into distinct block identities would introduce that extra identity
freedom. Removing those references from identity while storing their literal
values in origin consensus state would instead make one block hash have two
possible native states. Both are invalid shortcuts.

Two-phase candidates must distinguish the following contracts:

1. **Accepted origin then body:** the header Notice commits the computational
   projection and independently authenticated public attachments; later body0
   binds the exact accepted origin. If attachments are interchangeable for
   block identity, the origin consensus row must contain only their canonical
   semantic projection. Exact Result/Witness/service choices become authority
   only through later accepted transaction bytes. This differs from requiring
   the origin hash itself to fix one complete literal r1 Result, so it needs
   an explicit contract decision; it cannot be called equivalent silently.
2. **Pre-PoW non-computational intent:** pin every discretionary funding/role/
   reference requirement in an actual origin-template transaction or native
   prior-state intent before deriving the anchor. Changing it then changes
   pre-PoW hash/job and requires another inference, as ordinary template
   grinding already does. The pre-intent cannot contain final ResultId,
   ServiceId, funding scope/consent derived from the final Result, record roots,
   origin hash or actual later lease ID: those reintroduce a cycle. Final
   Result/service/consent must be uniquely checked against the pinned intent
   and deterministic computation. An unhashed local draft or a caller-provided
   precommit token is insufficient. Adopting and authoritatively resolving
   this new intent operation is additional implementation work, not existing
   mining eligibility or new issuance.

A Panel must also build its record before the relevant ticket/PoW decision;
letting it construct the priced roots only after mining changes own-template
work semantics. For a pre-mining Panel service, native admission/reservation
must authenticate a draft template preimage against the real native parent,
but must not treat an unmined template as an accepted origin or rewardable
mining claim. At final admission it must compare the actual origin header,
accepted prefix and all derived facts and consume the real leases atomically.

The alternative is useful: it removes a need to invent a new computation-only
lottery formula for ordinary PAV2. It does not yet remove the need to specify
the exact header identity/attachment contract, pre-mining Panel reservation
contract and PAV3 compatibility boundary. These differences remain for formal
review; the current payload codec stays non-permitting and no new native
identifier is allocated here.
