# Independent read review — retained semantic borrow connection

Reviewed the authoritative cumulative `semantic-retained-borrow.patch`, NOTES, and actual existing Constraint/Tokenizer builder implementations. Read-only review; no repository modification, compilation or tests. No concrete blocking authentication/lifetime/quote defect found.

## Contracts and provenance

The table-only owner is a distinct opaque type, not a full raw owner with invented EOF/zero inventory. The common context path still authenticates current shared service/frame/source, matches the actual Store source ID AND header, validates immutable original semantic context/class/artifact/table and original acceptance time. Constraint's legitimate raw-length=None/no-EOF case therefore remains possible. The current original Constraint builder independently requires exact semantics/table-root equality with its actual job; the new table helper's broader data-only applicability cannot bypass that later check.

Full material still checks actual rooted EOF certificate, immutable tokenizer hash, accepted timing and exact raw byte count, then checks the borrowed raw flat hash and copies raw under its retained ticket. Factoring removes no EOF/hash check. Both owners share all-ID cardinality/order/root verification and the existing EOG stream's complete order/range/digest check; no caller count or Complete flag becomes authority.

## Ownership and allocation

`table_requests<T>` substitutes only the actual output inline type. Full `output_requests` still adds the same raw byte owner allowance, rendering outer Vec and schema-derived decoded payloads. Common table decode moves its Vec/EOG/count tuple into the full result; it does not clone decoded bodies. Full raw copy occurs after table decode and retains the same lease. The current Source reconstruction temporary still drops before output growth/material decode. Table-only scope is fixed-size and adds no hidden heap. The context prelude and its same frame/Source/hash writer quote remain held across output growth and result lifetime.

Adapters borrow `&'a NativeRetained<...>` and derive actual rendering/raw/EOG references from that borrow. Returned existing evidence carries those references with lifetime `'a`; it cannot be used after dropping the material owner/ticket. No `.to_vec`, clone of the raw/table payload, or unleased payload extraction was introduced. Tokenizer compact proof owners are separately borrowed for the same lifetime and remain independently admitted. The evidence's existing signed-intent clone and builder reconstruction/parser/signature/recheck scratch remain separate caller obligations, explicitly documented; this patch does not claim its material lease admits those costs.

## Real builder verification remains

Both adapters call the existing actual Store builders unchanged. These reconstruct current source/frame, verify current policy/DAA/rules scope, immutable semantics, literal signed material identity, current active/collateral-qualified filer and MLDSA signature, and then replay and recheck evidence. Tokenizer also rechecks actual accepted EOF identity/time/length and target token ID; Constraint verifies table/openings and real row pin. New adapter preliminary scope checks do not substitute for those checks or let late supplied fields establish original provenance.

## Tests and limitations

New actual native fixture blocks compare the SAME signed intent's existing builder result with the retained-data adapter result. They route to the actual primitive and independent fresh evidence recheck, rather than asserting an arbitrary declared outcome. Constraint explicitly asserts no raw semantic length and no EOF; Tokenizer rejects separately signed wrong EOF/scope, out-of-vocabulary target and missing proof. Missing table ID rejects at material retention. Existing full raw owner negatives remain through the shared decode path.

The local table quota test exercises exact held lease/second reservation/drop without pretending its synthetic Scope is current native state; native integration supplies that provenance. No compile/positive runtime result is inferred from this review. No all-parser closure, remote acquisition/availability, adopted retention duty, new paid fee, native original admission, slash or Complete authority is established.
