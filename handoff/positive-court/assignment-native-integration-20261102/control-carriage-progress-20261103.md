# Original Assignment control carriage — 2026-11-03

Based on `3a2bf5bed`, within adopted RFC-0045/ADR-0213 and RFC-0046/ADR-0214.
No new operation, namespace, public value or activation is introduced.

## Concrete source change

The private VP archive loader now has an Assignment control-history entrypoint,
sharing the existing Case loader's actual accepted ordinal/body retrieval,
ordinary Full transaction validation, original populated UTXO reconstruction,
selected-chain ancestry/header checks and reconstruction of the pre-event
native state from the persisted original selected-parent state/delta.
Assignment and Case subjects retain distinct concrete boundary types; no fake
Case is manufactured for an unconsumed Assignment.

The shared fixed populated-input checker compares actual transaction identity,
ordered entry hash/count, fee and checked exact value conservation. Original
Case behavior delegates to the same helper without changing its identity recipe.
The new borrowed control-record adapter requires the real PC2ACTRL envelope,
exact header Assignment/origin and the corresponding op8–13 prefix. Open unwraps
PC2AOPEN to compare only its literal original op8 carrier body. Later intake
Params are not consulted by this history loader. A naked old DATA frame is not
silently accepted as a native boundary. MetadataReceipt remains separately
Unavailable until its genuine carriage owner is connected.

This is a concrete native history-loading seam, not a completed native session.
It must still be called from current rooted record selection and the complete
original-policy/source/Remaining/custody/funds replay factory. The caller's
finite limits are not service reservations, and the control adapter does not
perform full op semantics or signature validation by itself. Rooted summaries,
hashes or successful decoding are never computation or payment authority.

## Validation

The Core material target passed all six cases. Core and Consensus library
and all-test-target typechecking passed, along with strict library and focused
test Clippy. The four runs share the same source map. The artifact audit
records the executable hash. The
framing fixtures explicitly use DATA-only short bodies; they are not represented
as signed or accepted native operations. Ordinary Full execution of this new
Assignment entrypoint needs the still-missing real native A/C owners and is not
claimed by those framing fixtures.

## Unfinished RFC-0045 work

Actual A/C/R intake, shared pre-coinbase metadata fee allocation, finite real
custody/global service reservations, original-contract continuation routing,
Case/source replay factory, P negative/positive accounting and charge/release,
all-purpose actual payment, R2 continuation and native restart/reorg/IBD remain.
All shipped Positive/Assignment/relay selections remain None.
