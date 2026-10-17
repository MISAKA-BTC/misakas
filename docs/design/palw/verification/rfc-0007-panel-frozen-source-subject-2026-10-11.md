# RFC07 Panel frozen source subject and v4/v5 verifier

This change is dormant under the existing Panel and opt-in source fences. All public activation guards and the empty approved Panel policy registry remain in force. It is not a completed paid Panel/claim adapter or source soundness/economics approval.

## Implemented

The Panel pre-object fold freezes one authenticated kernel auxiliary row (namespace 49) at the first actual selected-chain step reaching an epoch's scheduled release. It reads parent eligibility and all previously sealed assignments. The row binds the network, ruleset, scheme, epoch, scheduled carrying deadline, actual commitment block and DAA, parent checkpoint, eligible/excluded classes and the full immutable work exclusion set. A sparse DAA step cannot backdate this subject to the scheduled release.

Eligible profiles and assigned work identities used by the v4/v5 verifier come from that immutable row. Later eligibility changes or a claim leaving its pending phase cannot select new sources or permit the claim to source its own Panel. Existing rows are validated and never silently replaced. The aggregate retained profile/work references are capped at 65,536; exhaustion leaves a new epoch unavailable for its normal non-fraud termination instead of deleting prior facts.

The verifier dispatches opt-in v4 and prepaid v5 to their own collectors, with complete rooted source/fee inventories. It rejects an assigned work inside an opt-in inventory rather than filtering that mixed source out. Missing funding, orphaned fee promises, substituted context and an unavailable frozen subject refuse verification. Legacy v1/v2/v3 contexts retain their original schema and collectors.

## Still required

Panel target creation needs an authenticated sponsor derived from the accepted signed consumer and an exact full Panel/receipt/DA/court/material liability bound. The onboarding target's horizon is specific to onboarding and must not be reused as a Panel liability bound. The source and fee input builders currently authenticate onboarding attempts; they do not yet admit Panel targets. No deduction or Panel source funding is enabled by this snapshot/verifier change. Claim adapters remain open.

The actual epoch-freezing fold, its delta replay, carriage reconstruction and retention after non-fraud termination passed centrally on `5277aa33e` (`/tmp/rfc06-rfc07-retired-source-freeze-v13.log`). The fixture first creates the authenticated kernel route through its accepted object writer; this is core-fold evidence, not signed native inclusion or a funded Panel lifecycle. Synthetic contract verifier cases also passed in the earlier selected core run. Funded Panel lifecycle on real nodes, CLI/wRPC, source participation/independence/censorship, cumulative retries and external 128-bit review remain required.
