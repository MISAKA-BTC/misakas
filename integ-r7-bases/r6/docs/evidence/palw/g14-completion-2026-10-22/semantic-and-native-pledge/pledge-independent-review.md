# SourcePledge candidate read review

Reviewed `current-producer-pledge.patch` against current native diagnostic, existing TFP1 binding and custody signature APIs. No concrete apply blocker found. No repository edit, compilation, native test or execution performed.

The new check follows actual current producer existence, exact key equality with original executor and Active status. It reuses the existing independent SourcePledge signature context and actual signed pledge fields; payer or original FP consent cannot replace it. Literal TFP1 pairing checks bind pledge producer/claim/job/source/header before this step.

The negative changes only pledge signature, explicitly verifies unchanged payer and FP signatures, rebuilds/signs/finalizes actual transaction, passes real native Full UTXO validation, and requires the exact new SourcePledge Invalid branch. Thus stale transaction ID or malformed envelope refusal does not masquerade as the tested signature failure. Existing state root/fallback behavior and the ultimate Missing classification remain unchanged.

The positive generated fixture's actual pledge signature and all runtime results remain unverified until parent builds/tests. Signature validity does not authenticate pledge resource profile/horizon, all branch closure, original admission, escrow lock, or paid service.
