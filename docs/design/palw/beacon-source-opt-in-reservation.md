# Per-attempt beacon source reservations — dormant implementation

This implements the ledger contract, signed prepaid source fees, and dormant **onboarding v4/v5** target/collector/SDK connections for ECON §4.7 S3/S4. V5 requires prepaid fees for every opted-in source. Security/economic closure still needs the acceptance conditions below. The existing v3 collector retains its policy and domains. No preset arms the new source fence. No approved source deposit, fee, concurrency or liability tuple is introduced.

## Accepted contract

Inner kernel tag **21**, `OptInBeaconSource`, signs `(producer, job, seal, context_id, terms_id)` through the existing signature-verified kernel route. It is 322 encoded bytes; its hard parser ceiling is 384. This does not alter tag 20 or a Round ticket. The native inner-kind table and admission gate assign tag 21 to **`Params::palw_beacon_source_opt_in_v1`**, separate from OPV. All presets hold `None`; any real height is refused by config validation. Its optional activation **and policy digest** are hashed Some-only in both fingerprints, and `never()` is removed by identity normalization. The ledger also requires an explicit, separate `activation_daa` from its consumer and the existing OPV fence. Default and row-restored admission inputs are empty, so tag 21 is refused until the consumer re-authenticates the target inventory.

The consumer supplies the **authenticated frozen target**; object bytes cannot create one. The context ID includes the complete `BeaconContextV1`, including subject, chain/ruleset/policy, epoch, accepted commitment position, candidate and both frozen profile sets. Source terms additionally bind all three window boundaries, a source Final deadline, its liability/material hold horizon, its separately reserved deposit and producer/global concurrent caps. Checked clock arithmetic refuses overflow. A zero deposit/cap, impossible horizon, or an unrepresentable release after `u64::MAX` is refused. The deposit values in tests are fixtures, not the derived 28.6 BILI policy.

A source needs its producer's own live salted-race seal, in the target's seal window, with enough ordinary seal TTL to survive its reveal window. The source profile must be frozen eligible OPV work, outside the candidate/dependency exclusion set; its job needs an authenticated retained payer. Only the node's currently exported ordinary/program and pipeline job forms are accepted. Segmented/typed jobs require their own complete authenticated source adapter.

Every target takes a **fresh source reservation**, in addition to the unchanged ordinary race deposit. Existing reservation receipts (`ReserveSealDeposit`, `ReleaseSealDeposit`, `ForfeitSealDeposit`) already have the required collateral/reservation effects; table 28 records which reservation is the source contract. A target/producer pair is used once forever. Failure, replacement or refund cannot replace its mixed seal. All in-horizon rows count toward concurrent caps, even after their deposits burn; changing caps cannot bypass a stricter still-live contract. For a single ordinary seal promised to several targets, the reveal windows must intersect, and its reveal is refused outside any of them.

## Money, clocks and recovery

Table **28**, keyed `(context, producer)`, retains the job/profile/payer, exact seal and accepted position, opt-in position, terms, held amount, revealed claim/position, and permanent forfeit/refund outcome. The optional source root extension is absent when this table is empty. Ledger roots and row-derived roots compose the extension in the same order, after EXEC holds. Row reconstruction validates bindings, revealed salted openings, source liability and held collateral; it clears the new consumer admission inputs. It does not clear existing contracts.

A successful salted claim admission returns **only the ordinary race deposit**. Source deposits remain reserved. Admission also extends that claim's material/collateral liability to every live signed source horizon. A claim Final retains both reservations through that horizon; Final is not a source-deposit refund. The tick refunds the source reservation once, only after the horizon and only for a standing source claim that finalized by its signed deadline. Returned principal is not source income.

Resealing or expiry burns every live source deposit attached to the replaced seal, independently of the ordinary seal deposit. A missed reveal, source conviction/default/timeout, or missing timely Final also burns the separate source reservation. Failed source metadata remains. In particular, several reseals at the same DAA cannot overwrite the per-target source row. This does **not** repair ordinary v3 table 26's older `(job, producer, sealed_daa)` collision: do not infer full v3 S1 closure from these new opt-in tests.

The row journal and scalar journal must both roll back with the consumer's block. The tests exercise exact row/scalar undo and rebuild against the old root, along with settlement-book conservation. The ledger tests alone are kernel consumer evidence; node/SDK evidence for the v4 connection is recorded separately in the design audit. P2P and pruned recovery are not established.

## Signed prepaid fees — separate monetary fence

Inner **22**, `FundBeaconSourceFee`, signs source terms, the authenticated attempt sponsor, fee per source and a maximum funded capacity. Inner **23**, `OptInPaidBeaconSource`, signs the producer/job/seal/context plus both source and fee term digests. Old tag 21 remains an unpaid source contract; funding does not retroactively award it income. The additional gate is **`palw_beacon_source_fee_v1`**, independent of the source and OPV gates. Every shipped preset keeps it None and config validation refuses every real activation. Both fingerprints bind activation, minimum fee and capacity. Probe values (50 sompi, capacity 64) test mechanics only; they are neither calibrated economic floors nor approved policy.

Each native load derives the sponsor from the actual signature-verified conformance commitment and requires its statement/position to match the current rooted attempt. A funding object cannot declare its own target inventory, minimum or authorized sponsor. It must match the frozen target, satisfy the authenticated minimum/cap, precede the seal-window end, and use free collateral of a non-exiting sponsor. The sponsor pays the **entire maximum fee capacity at funding**. This debit is not burned or paid out: it becomes separately owned escrow in rooted kernel table **29**, keyed by context. A reservation left in the same sponsor bond would be vulnerable to the native lanes' other slashes, which do not honor kernel reservations; prepayment removes that dependency. Cumulative debits equal routed payouts/burns plus held source escrow. Original settlement kinds retain their behavior; new kinds 22/23/24 transfer funding, pay a vested source fee, and refund unused/failed principal.

A paid source signs the immutable prepaid price/capacity before admission and adds a once-only promise in rooted table **30**, keyed `(context, producer)`. The promise is accepted only alongside the original fresh source-deposit contract and only if the prepaid pool backs every outstanding minimum receipt. Failure does not free capacity for another attempt; neither funding nor a paid promise can be replaced. The two books have a new optional root extension absent while empty. Rebuild validates their target bindings, exact capacity/money inventory, outcomes and clocks, and clears consumer admission inputs. Historical source table 28 and v3/v4 beacon domains remain unchanged.

At the seal-window end unused capacity is returned to the sponsor once. A failed source returns its unearned fee to the sponsor once and still forfeits its independent source deposit. A timely standing source Final vests its fee only **after the full signed hold horizon**. A different source vetoing the target does not take an honest source's fulfilled payment. Returning source deposit or sponsor principal is not income. The native consumer checks original prepaid context balances and promised recipients; it never pays source fees from the sponsor's later collateral, a new job escrow or new issuance. Row/scalar undo, replay and DB recovery must preserve both the escrow debit and the once-only rights.

V4 permits optional funded agreements. The distinct v5 policy below makes the funded agreement mandatory for its own onboarding sources. Measuring carrier/check/capital costs and a minimum **net** receipt, and proving class profitability and zero-volume capital coverage remain separate acceptance conditions. The current conditional fee provides no idle retainer. On 2026-10-11 the user selected a separate execution / fee Round cap and aggregate market-fee-plus-issuance `R_max`.

When the bond-budget fence is in force, paid admission reserves the actual source recipient's entire fixed fee in R before any accepted native source or monetary write. The immutable subject is a separate hash of the signed `(job, seal)`, because the source claim has not yet been revealed. Context and recipient are also bound into the income-row ID. A refused R reservation rolls back initial budget creation as well as all provisional rows. This source-income row creates no Q, B, F or execution ticket. Pending promises remain held through their signed liability horizon; payment consumes that same row once and extends reuse through the actual payment's W. Returning unearned sponsor principal cancels the income entitlement. The route tick includes source and funding books even when no computation claim can move. First-budget migration must also seed old pending promises, including ones whose acceptance-plus-W has already elapsed.

## Mandatory funded onboarding policy v5

`palw-work-beacon/paid-opt-in-source/v5` has separate initial/item/mix domains. It preserves historical v4 context/contract codecs and freezes its additional authenticated sponsor, fee floor and capacity in auxiliary **table 47** before source admission. Funding must cover at least the full k-source quorum; underfunded capacity is refused before a sponsor debit. Tag 21 cannot join a v5 target, even after someone has funded it. Tag 23 requires both signed digests and complete prepaid backing.

The v5 accumulator binds the whole context, source contract and monetary contract. Every same-context opt-in must have exactly one matching fee promise at the same acceptance position, with the fixed signed price and funding preceding admission. Missing, duplicate, orphaned or substituted fee promises are refused. A paid but withheld, forfeited or late-Final source still vetoes the target; payment cannot filter a failure out of its inventory.

The native collector derives all facts from the rooted source/funding/promise tables. The SDK requires `VerifiedPaidOptInSourcesV5`, built from the same root-verified op-211 snapshot as the target and attempt; v4 and legacy verification entry points refuse a v5 attempt. CLI fresh verification dispatches to that SDK API and reports the authenticated funding/promise figures. These checks authenticate against supplied roots; a connected RPC server's self-consistent roots still require an authenticated chain view. No new source-independence assumption or effective-bit approval is inferred: v5 remains at **0 effective bits** under the release gate.

### V5 gross fee and scoped net cost bounds

The monetary policy carries optional proposed `PalwBeaconSourceCostBoundsV1`.
V5 refuses an absent tuple, a zero component/report identifier, arithmetic
overflow, a source deposit above the tuple's bound or a liability span beyond its
scope. V4 keeps its earlier gross-fee contract. For V5, the admitted gross floor is

`max(policy.minimum_fee, execution + carrier + material + public_court + locked_capital + minimum_net_receipt)`.

The native commitment freezes the whole tuple in separate rooted aux table 48
alongside the immutable table-47 sponsor/floor/cap. Every later admission checks
the frozen terms against the network policy; collector and SDK require the
frozen cost tuple and refuse a gross floor below its sum. The SDK exposes that
same tuple, and CLI verification reports it and its conditional minimum net
receipt. Every monetary component and report identifier participates in the
fee-policy fingerprint. Nothing is appended to the old attempt/header schema.

This is a conditional accounting bound, **not measured profitability or price
approval**. The report identifier is provenance, not a proof of a cost estimate.
The probe's six one-sompi components are arithmetic fixtures only. Activation
still needs reproducible worst-work/court/DA/carrier and capital-cost upper
bounds for every eligible source profile, evidence covering the signed liability
and payment-delay horizon, conversion assumptions and actual receipts. The span
checked by code is `H - seal_start + 1` (H is inclusive); this alone does not bound
wall-clock stalls or sparse ticks after H. A zero-volume retainer, independent
participation and censorship/entropy guarantees remain separate requirements.

## Remaining connections and acceptance

The dormant native onboarding adapter freezes the actual accepted conformance context and terms in auxiliary **table 46**, keyed by the full context ID; it does not append fields to the historical route header or attempt row. Every ledger load rebuilds admission inputs only from open, post-fence v4 attempts in their seal windows, checking the stored context and terms against the attempt and typed network policy. Closed targets remain as immutable contracts for settlement and verification.

`PalwBeaconSourcePolicyV1` binds the separately reserved deposit, producer/global caps and finite source-Final slack. The target builder uses checked clocks: the reveal end plus that slack is the source Final deadline; the hold extends through source settlement depth, the latest evidence publication/window and binding liability. It refuses slack below the ledger's declared challenge/court/grace sum. This is a **bounded contract**, not proof of worst-case physical carrier/inclusion time or an approved economic deposit minimum. The private harness uses 100 sompi and caps 8/64; these are not the derived 28.6 BILI policy. Intersection of reveal windows alone does not prove inclusion in the intersection.

The source-only collector has its own randomness ID, `palw-work-beacon/opt-in-source/v4`, and initial/item/mix hash domains. Its initial accumulator binds the **whole context and contract digest/deadline**. Only table-28 reservations are projected into v4 facts, including permanent failures; ordinary table-26 race seals do not enter this projection. Withholding, forfeiture and missing/late source Final veto rather than disappearing or leaving settlement open indefinitely. Eligible distinct-source quorum rules still apply. v1/v2/v3 collectors refuse v4 and v4 refuses their policies; no v3 seed changes.

The SDK's v4 API requires `VerifiedOptInSourcesV4`, constructed only by rebuilding public op-211 rows against supplied ledger/aux roots and deriving target/reservation facts through the native adapter. The frozen target must equal the context of the **current rooted attempt**. The supplied attempt/evidence bytes and canonical program must also equal that same rooted snapshot, so a substituted lifecycle state cannot fabricate agreement. The legacy fresh-verifier entry point explicitly refuses v4 without those rows. Roots must come from the reader's authenticated chain view; self-consistent roots supplied by an adversary are not authentication. No caller-supplied seed/output or raw ordinary seal list can substitute for the SDK contract. Effective v4 soundness remains **0 bits**, with no inherited v3 participation claim or new approved tuple.

Onboarding is the only connected native target type. Panel assignment and claim verification require separate authenticated adapters; their old collectors refuse a v4 policy. S3/S4 closure still requires the remaining economics, timing, independence and scale conditions below.

Prepaid source fees implement signed funding and once-only Final/horizon vesting; mandatory funded participation and a measured minimum net receipt remain unclosed. Watcher/source capital costs and zero-volume solvency are still separate unresolved economics (§5f); no issuance or unfunded stipend is added here. S2 retry/lifetime soundness accounting, Sybil/source independence, censorship, composed soundness approval, and public class security approval remain open. The hard binary live-slot ceiling is a defensive implementation bound, not a demonstrated 100k-class resource/throughput gate. The book retains historical rows and currently scans them; indexes, bounded history/snapshot rules and measured scale remain required before activation.

Required node evidence includes signed admission/refusal, separate charges for N victims, cheap ordinary seals excluded by the new collector, withholding/reseal/abandonment/public conviction including after source Final, deadline and low-demand cases, once-only refund and actual node restart/replay/reorg, served-row SDK verification, pruned/P2P recovery and realistic concurrent load. The existing v3 many-victims regression is expected to remain negative until its actual consumers move to the new policy with their own complete contracts.
