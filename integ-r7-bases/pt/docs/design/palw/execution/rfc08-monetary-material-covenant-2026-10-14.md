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

## 2026-10-15 note: in-node PAV3 exchange and private loopback provider (unverified source)

Lead default B, pending user review. Nothing below has been compiled, tested by cargo or
run on a node; it is source only.

- **Producer side** (`kaspad/src/palw_native_custody_v3.rs`, `palw_producer.rs`). Past
  `palw_monetary_material_v1`, for every class but the BASE floor, the in-node producer
  reads the canonical prior session from its own coherent tip
  (`ConsensusApi::palw_native_session_v3`, the one `select_native_session_v3` rule, at the
  template's selected parent) and refuses a roster with fewer than two distinct provider
  operators or one containing the executor's operator. Both draws are priced on a
  provisional PAV3 envelope: `priced_execution` reads session, consent, roster and the
  immutable execution, never the manifest or a signature, so the typed bundle is built
  only for a won draw. The sealed carriage's priced execution is re-checked equal before
  publication. On a win: authority from `palw_native_material_provider_authority_v3`, the
  actual bundle from the producer's own capture and resolved backend
  (`PalwHeldMaterialBundleV1::from_capture`), `draft_native_ready_v3`, every roster
  signature through a `NativeReadyExchangeV3` transport, each verified against the
  registered roster key, then `producer_assemble_native_ready_v3` and the executor seal.
  Without a configured exchange (every ordinary kaspad; no Args flag sets one) the
  producer keeps `PALW_PRODUCER_NATIVE_CUSTODY_HOLD`. Riders still hold under the fence.
- **Loopback provider** (`kaspad/src/private_integration/native_provider.rs`,
  feature `private-integration-config`, paid-v3 profile only). Provider bonds are fixture
  seat seeds named `SEED@TXID:INDEX`; each must be the salt's card key for that bond, at
  least two distinct card operators are required, and on a producing node none may be its
  producer's bond, key or operator. Each bond has a `DurableMonetaryMaterialProviderV1`
  store (plus an authority index for reopen) under `<appdir>/<network>/palw-native-provider-v3`.
  Per request the service obtains authority from its own Consensus API, re-derives the
  prompt from the anchor, rebuilds the bundle itself, refuses a differing manifest,
  checks the registered key, publishes durably and only then signs `Ready::id`.
  Transport: in-process, or one framed borsh request per loopback TCP connection
  (`--private-probe-native-provider-keys/-listen`, `--private-probe-native-exchange`).
- **Paid-v3 factory** additionally arms `palw_panel_lifetime_v2` (structural probe values)
  and `palw_paid_consumer_lifecycle_v1` (1,000/3,000/2,000 DAA mechanics-only spans) from
  DAA 0, with mirrors checked and both made dormant in the validation copy; the public
  validator refuses both whenever armed.

Remaining: a non-loopback, authenticated provider transport; native session enrollment
in the drill (the bootstrap's floor producer is exempt, so `--native-loopback-provider`
measures nothing about custody by itself); provider GC wiring; compile, unit and drill
execution of all of the above.

## 2026-10-15 note (round 3): authenticated provider transport, provider GC janitor, drill session enrollment (unverified source)

Lead default, pending user review. Nothing below has been compiled, tested by cargo or run
on a node. It is source only, and no PASS is claimed. Public presets, fences and release
heights are unchanged. Ordinary kaspad Args select none of it.

- **Authenticated provider transport** (`misaka/palw/remote/src/provider_transport_v1.rs`,
  wired in `kaspad/src/private_integration/native_provider.rs`).
  - **Requests.** Each request is signed under its own ML-DSA-87 context by the
    executor's bond key. The signature covers: the chain/network domain
    (`palw_network_domain_v2_for(net, genesis)`), the purpose, the sender bond and key,
    the sorted recipient provider bonds, a 32-byte nonce, the wall-clock issue time, the
    request's DAA score, and the body length and digest.
  - **Provider checks before any work.** The provider verifies the request first: domain,
    ±120 s clock window, ±600 DAA against its own virtual DAA (an unknown DAA refuses),
    signature, and then a bounded `(sender, nonce)` replay guard that refuses when full
    and never evicts a live entry. It then binds the sender to the body: the bond and key
    must be the attempt's `executor_bond` / `executor_pubkey`. The Consensus API authority
    read refuses a key that is not the bond's registered key.
  - **Responses.** Every answering provider bond signs the response, which binds the
    request id, the responders, the time and the body digest. The producer verifies each
    signature with the roster keys from its own authority read, never with a key named in
    the response. The trait method that carries those keys is
    `NativeReadyExchangeV3::request_signatures_authenticated`.
  - **Failures.** An unauthenticated request gets no answer. An authenticated refusal is
    signed.
  - **Limits.** Frames use the existing bounded length-prefixed codec. Timeouts are 10 s
    connect, 60 s per read on the provider, and a 600 s exchange. The service runs at most
    4 concurrent connections.
  - **No encryption.** The transport is TLS-free on purpose. The material is public, so
    the guarantees are integrity and authentication, not confidentiality.
  - **Loopback and remote.** The loopback socket is one instance of this transport.
    `--private-probe-native-network=remote` admits separate hosts and remains
    private-integration only.
- **Provider GC janitor** (`misaka/palw/remote/src/monetary_material_gc.rs`; one thread
  per provider service, 60 s period).
  - **Observing rows.** While a covenant row is present in a coherent read (sink
    unchanged across `palw_monetary_material_covenants_v1`), the janitor records the
    `(consent, tail_block)` pairs and the point at which it saw them.
  - **Finding the release.** Once the row is absent, it walks the selected-chain blocks
    after that point (`get_virtual_chain_from_block`), at most 256 witness calls per
    tick. For each candidate it asks `palw_monetary_material_release_finalized_v1`
    (accepted release delta of exactly `(root, consent, tail)`, with tail and release
    block both finalized on the current selected chain). It deletes only through
    `DurableMonetaryMaterialProviderV1::gc_finalized`, which asks the same witness again.
  - **Retention.** The janitor pins and keeps the material when a contract was never
    observed accepted, when there is no observed tail, and when any read fails.
  - **Crash safety.** A write-ahead intent marker (write, fsync, rename, directory fsync)
    names the exact witness tuple. On restart the marker replays the same witnessed
    deletion, and the replay is idempotent. The journal is rewritten atomically.
  - **Remote providers.** `RemoteProofGcOracleV1` uses op-242 rows authenticated by
    `MonetaryMaterialProofPagerV1` together with an operator-configured finality
    authority. It is weaker than the local delta witness and says so. With no authority,
    it deletes nothing.
- **Drill session enrollment.**
  - **CLI.** `misaka palw material-session draft|sign|seal` builds
    `MonetaryMaterialEnrolledV1` with `native_session` for a non-floor class. Each party
    signs offline under the tag-240 provider-court message, and the result is filed with
    `palw submit-object`.
  - **Bootstrap manifest.** The paid-v3 bootstrap manifest now carries the factory's
    unapproved `monetaryMaterialPolicy` terms and each seat's `operatorId`.
  - **Drill flag.** The `--native-session` drill flag requires
    `--native-loopback-provider` and paid-v3, and excludes `--complete-check`. With it,
    the drill:
    1. Enrolls the session for `class`: executor and sponsor seat 0, providers seats 1
       and 2.
    2. Waits until every node observes the row through op-242 pages. This is an
       observation, not an authenticated proof.
    3. Switches A's producer to that class.
    4. Waits for a non-empty provider store on B.
  - **Scope of the evidence.** A non-empty store is evidence that the exchange ran. It is
    not a Final, a court outcome or a GC observation. Python control tests are in
    `scripts/drills/tests/test_rfc12_native_session_plan.py`.

Remaining:
- Compile and run the unit tests of all of the above.
- Run the drill itself. Whether the producer will produce the registered non-floor class
  before binding maturity is unobserved. If it does not, the drill's custody wait times out
  and the run fails with that reason.
- A GC deletion of real typed material: only the empty-store and refusal paths have
  prepared tests.
- An op-243/delta proof that would make the remote GC witness equal to the local one.
- A multi-host private drill: the private probe still pins P2P to loopback.
- Unused-offer GC wiring (`discard_expired_offer` needs the original enrollment, which the
  janitor does not keep).
