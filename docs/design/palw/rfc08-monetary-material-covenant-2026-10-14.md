# RFC08 monetary material covenant — code checkpoint, 2026-10-14

This record describes source prepared after the user changed the order to complete
code before batch verification. None of the new covenant, capacity-provider or
evidence tests in this record has been compiled or executed. Earlier EXEC/budget
passes do not validate this version. Public presets retain `None` for the new gate.

## Contract and monetary consumer

Outer kinds 240/241/242 and kernel auxiliary table 120 introduce a separate signed
contract. Every provider and the sponsor signs the complete exact root/dependencies,
optional authenticated EXEC plan, capacity manifest, collateral, own-funded fee,
physical monetary window W, and the explicit payment-relative obligation. Existing
absolute ProviderLease carriers retain their previous meaning. There is no unsigned
lease extension, shortening of Final/court windows, or cancellation of earned cash.

Enrollment preflights all operators, capacity, free collateral and the recipient's
actual shared R before any write. Sponsor collateral backs provider SourceFee income
through the common native payout queue. One queue drain records its actual carrying
block, P, queue, exact fee portion, full coalesced coinbase amount, material receipt and
original entitlement. Queue provenance is derived from the canonical transfer receipt
preimage in the journal; amount equality alone never assigns a source entitlement.

The normal hold is MAX while any liability/queue remains. All real payments contribute
P+W; the maximum tail persists. Sparse DAA and queue backlog cannot expire the provider's
new signed obligation. Recovery requires fresh signatures and new sponsor-owned funds.
Charged old providers remain in the bounded row until their service fee is physically
paid, preventing replacement from erasing unpaid fee attribution. Count, total, paid
total and an entitlement-history commitment retain retired-generation monetary history
without an unbounded collection of old provider rows.

Release requires no live monetary hold or challenge and the exact tail finalized on
the carrying block's selected chain. Physical provider GC adds a stricter witness:
the accepted native auxiliary-row removal and the release block itself must be
finalized on the current selected chain. Missing deltas, old tips, incomplete imports,
generation changes and merely absent contracts cannot authorize deletion. Provider
GC therefore keeps the bytes during every admitted pre-finality release reorg.

## Actual codec capacity and provider

`palw_material_capacity_v1` uses signed descriptors for the original and each
continuation separately. A descriptor includes claim, execution/body commitment,
codec, exact canonical bundle length, unit count and content digest. The signed cap
counts every repeated binding, checkpoint, opening and leaf-evidence copy, the full
manifest and exact provider map framing. RAM admission estimates are not used as a
wire-material proof.

The NativeHeld complete codec authenticates the binding, all prompt tiles, every
checkpoint's state chunks, every step's leaf evidence and canonical range openings.
Distinct authenticated in-range units and exact category cardinalities prove complete
coverage. Leaf evidence runs the existing one-move output/operand/artifact/prompt
verifier; binding equality alone cannot certify availability. This service does not
reopen an expired computation verdict. Unsupported prompt/chunk/family codecs return
typed reasons and cannot produce a verified certificate. Numeric byte declarations
without a complete manifest do not authorize new native work.

Kernel position bundles cover every committed stage/position, authenticate the actual
responses against the ledger, and count their canonical bytes. Segmented bundles derive
the actual part layout for every position and authenticate each canonical part, with
hard limits on bytes/units/scopes. Original and continuation copies are counted
separately rather than sharing an assumed zero-cost reference.

`DurableMonetaryMaterialProviderV1` prepares and validates complete typed data before
provider signing, then publishes with write/fsync/rename/directory-fsync. A kernel
file lease prevents simultaneous writers and is released on process crash. Reopen
authenticates every stored codec again; stale owned pending snapshots never acquire
signature authority. Replacement prepares a distinct signed generation. Held answers
come from the retained typed bundle and include the exact dependency. Unix kernel
leases are implemented; unsupported platforms return an explicit refusal.

## Economic evidence and remaining acceptance

`assess_monetary_material_cost_v1` admits retrospective evidence only with the exact
manifest, complete original service-fee entitlement history, canonical native payment
verification callback, reviewed external measurement scope and finalized release
authority. It reports actual income, measured storage/network/verification/recovery/
court costs and measured surplus or deficit. It explicitly returns
`PaymentAndRecoveryTimeUnknown`; a profitable completed interval is not a theorem
bounding future sparse-chain staging, inclusion or recovery time. The public gate
continues to refuse activation without that approval. No fixed prepaid fee is labeled
as safe compensation for unknown unbounded time.

Prepared test sources cover funding refusal atomicity, signed replacement replay,
old unpaid fees, actual late P+W, delta reversal, capacity/framing boundaries, provider
GC witness rejection and corrupted restart state. The native signed enrollment test
uses actual ML-DSA carriers and native coinbase drains plus reopen/replay; its unused
root is a service plumbing control, not model execution or full copied-material proof.

Batch verification still must compile the shared source graph and execute those
controls, valid full typed model/provider fixtures, independent crash/reorg/pruning
consumers and actual economics. Unsupported codecs and missing economic/liveness
approval are failures to close acceptance, not PASS by refusal. This checkpoint does
not claim the original goal or all-model acceptance is complete.
