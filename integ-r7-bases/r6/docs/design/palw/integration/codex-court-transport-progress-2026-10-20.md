# Court transport accounting implementation record — 2026-10-20

Status: dormant implementation candidate on `codex/rfc14-x3-reorg-clock`; not integrated, adopted, armed or deployed. This is a prerequisite of RFC-0026's court submission capacity contract, not completion of its one-outsider capacity guarantee.

## Scope

The existing court capacity classifier counts four adjudication operation classes, but not `TirPublicControlV1` or `TirPublicGrantReadyV1`. Those carriers include acquisition response parts and conviction evidence. The candidate adds a separate `palw_court_transport_v1` fence and policy; it does not change the existing capacity-policy ID or silently reinterpret an existing operation class.

One shared function computes costs in the actual processor acceptance walk and native fold. Transport draws from the existing session budget; one-move accusations retain their separate budget. The byte counter serializes the canonical object into a counting writer and includes the lifecycle version, object tag, metadata, signatures and literal body/evidence. It excludes transaction framing, which remains subject to ordinary transaction mass. This is not a measurement of network capacity or a demonstrated partition of physical block capacity.

Work and memory use explicit positive base and per-payload-byte coefficients, with checked arithmetic. Their values must cover native signature verification, state-dependent frame processing, assembly and resident memory. The probe with coefficients equal to one exists only for fork identity and synthetic tests; it is not a production calibration or claimed upper bound. Overflow of either cost or cumulative block expenditure refuses the operation, including when the budget is `u64::MAX`.

All shipped presets leave the new fence absent. Before activation no new serialization/accounting occurs. Native frames retain their original acceptance clock, including retired claims; originals accepted before this fence keep their old transport rules. Active profiles without a capacity budget refuse. Policy bytes enter the configured params/schedule IDs only for a configured non-`Never` fence; the existing identity visitor normalizes future schedules. `Never` contributes nothing to those IDs. Real-network validation always refuses this candidate until transport measurements, the physical budget split and one-outsider capacity closure are reviewed. An isolated G14 genesis permits synthetic tests only.

## Verification boundaries

The codec fixture checks literal byte accounting and overflow. The native fold test sets up a synthetic preexisting demand, then signs incoming Part and Finish controls with real ML-DSA-87 keys and runs the actual object dispatch. It checks an exactly fitting Part and budget rejection of a following Finish without state mutation, plus dormant/old-original compatibility. This is not a live-node flood test or proof that an outsider can pursue every allowed branch within 3,120 / 10,000 DAA.

The processor uses the same cost function, drops an over-budget object and retains the block. A real processor-level budget exhaustion test remains required; compilation alone does not establish that behavior end to end.

Release checks passed: 4 transport-filtered native/codec tests, 28 identity/activation/release regression tests, and `cargo check --release -p kaspa-consensus`. The explicit-`Never` schedule regression failed initially and passed after excluding an already-disabled fence from visitation; no expected identity was repinned.

Release checks and raw logs are recorded in the [verification record](../../../evidence/palw/court-transport-2026-10-20/README.md).

## Remaining work

- Measure ingress parsing, rejected traffic, native work and state-dependent memory. Accepted-object accounting alone does not bound pre-authentication denial of service.
- Derive and review the court/ordinary physical capacity split and guarantee its actual supply.
- Connect deterministic reserved service and its supply journal to pending duties, response deadlines, default attribution, reorg undo and retention.
- Connect the owner-independent claim frontier, all permitted adversarial branches and a signed third-party relay route.
- Keep `OneOutsiderCapacityIncomplete` / `ServiceOpportunityIncomplete` closed until their complete acceptance criteria pass. No cap increases or invented production coefficients resolve these gaps.
