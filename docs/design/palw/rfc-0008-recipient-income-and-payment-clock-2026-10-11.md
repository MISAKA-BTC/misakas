# RFC-0008 recipient income and native payment clocks

Status: dormant implementation and focused acceptance candidates. This record is
not approval to activate RFC-0008, a measured economic PASS, or an all-model
execution soundness result. Every public preset remains unarmed.

## Confirmed monetary policy

Fee-only Rounds occupy the separate execution-right cap, T. Reward-bearing
blocks occupy B. Actual recipient R includes issuance and market fees together;
monetary income reserves the full collectible maximum before acceptance.

## Native holds

`NativeIncome` (budget origin 9) and `NativeIncomeQueued` (10) use only R.
They manufacture no Q, B, F or T. Q/B/F stay in the original computational
reservation. An issuer's original R moves into a recipient hold instead of
being charged a second time at Final. Domain-separated root/bond/role identities
are issuer 0, Panel 1, extra EXEC 2 and standalone kernel 3.

Panel binding reserves actual seat recipients atomically. Flat Panels reserve
one fixed share per distinct bond; sharded Panels conservatively reserve the
whole pool per distinct bond until fixed shares are available. Signed extra
slices reserve the recipient's full work share under the immutable issuer
allocation ceiling. Unpaid same-bond standalone kernel money can move into that
EXEC hold without a second standalone payment entitlement or Q charge. Prefix
binding cancels its separate standalone entitlement without enlarging the
issuer allocation, and keeps already occupied quota through its window.

Positive native funds consume the hold when they become queue credits. Several
funds for the same recipient can mature separately. Each positive partial fund
immediately splits off its own exact queue-key/bond receipt, preserving the
aggregate R. The unpaid remainder uses origin 11 and cannot transfer into a new
work entitlement or revive after cancellation. The last fund carries the
remaining maximum; receipts can coalesce only under the same exact native key.
One queue draining cannot start another unpaid queue's W clock. The pre-object fold's actual Payout removal confirms the coinbase
payment. Its reservation then lasts through actual payment DAA + W. A sparse
DAA jump cannot release an unpaid or still queued hold. Pending money keeps
`latest_release_daa = u64::MAX` as a withdrawal sentinel; ordinary release
removes the final zero bond row, so the sentinel creates no permanent hold.
Void, redraw, public conviction, dependency failure and zero-funded Final
cancel only unpaid rights. Cancellation never returns quota early.

## Collected model market fees

Carrier and EVM fills share the same owner/contributor consumer. It pre-reserves
both recipients' full collectible shares atomically, including the owner's phi
pool diversion. A positive cash leg uses the exact native model payout key and
reclassifies the same R into `NativeIncomeQueued`, without a second charge.
Queue backlog and sparse DAA cannot free it before the actual drain plus W.
Pool-only income is already owned at collection, has no unpaid cash leg and
uses collection plus W. Returned market-position principal is excluded.

Focused mechanics candidates cover the shared buy/sell consumers, exact
journals, second-recipient refusal rollback, EVM full refund, native backlog
across DAA 1,000/2,000, and partial/all-pool phi routing. Their synthetic admitted
class/backlog seams are explicit; they are not native model soundness or a
measured profitable market. Runtime results are still required.

## Paid source fees

A SourceFee promise (origin 8) reserves its immutable signed subject before
opt-in. Strict H and exact fixed fee checks precede every transition. Kernel
`Outcome::Paid` first creates an irrevocable native kernel queue credit. The
same R reservation moves to `NativeIncomeQueued`; it remains live until the
common actual coinbase drain and then uses actual payment + W. The fee promise
and its immutable Paid receipt retain their original identity and clock.
Destination collisions, a different fee, an early H or a second payment are
refused before any budget journal mutation.

Initial migration preserves all pending promises even beyond accepted+W or
H+W. A Paid promise at the last kernel fold clock with its exact native queue
still present (or drained in the activation block) is seeded as queued and
uses the actual drain clock. Other historical Paid promises have no saved
physical coinbase DAA. All existing mapped historical income is therefore
counted once through first fence + W as a conservative boundary reservation.
This boundary is not a statement that payment occurred at the fence. A first
window can exceed today's R cap; immutable old money still settles while new
income is refused until room returns. A historical paid producer whose
immutable bond mapping is gone cannot accept new R under that retired key.
No missing pending reservation is grandfathered into an unbounded payment.

## Evidence and remaining scope

Central verification reported 7791238dd's three native-income engine cases and
14 bond-budget fold cases passing. Later standalone-kernel, withdrawal-sentinel
and source queue candidates require their own compile/runtime results. Their
focused tests cover atomic refusal, sparse payment, duplicate prevention and
exact journal redo/undo. These tests use existing arithmetic/admission fixture
seams where named; they are not native execution correctness proofs.

The earlier actual native root Final case passed on integration dc4bc671c:
allocation 7,047,767, three positive kernel fee legs 3,442,591 / 2,403,451 /
1,201,725, native issuer zero, allocation conserved. Real signed readiness,
Panel draw/receipt assembler, DB reopen, post-Final public conviction and replay
were exercised. Its REAL admission/trace lottery seam remains explicit.
Follow-up actual reorg and Round boundary fixes need their own runtime results.

This milestone does not finish the full RFC-0008 inventory. Remaining work
includes authenticated arbitrary H160 EVM fee recipient binding and both
priority tip / deposit claim-tip consumers; reporter, improvement-pool and
base/inclusion issuance consumers; complete old native queued/recent-paid
history migration; all-path B/R/F/Q validation; W/material/pruned join evidence;
actual all-model court correctness; and fixed-criteria measured small/large
profitability and neutrality evidence. Dormant activation is not substitute
acceptance evidence for those conditions.

## Round boundary preparation

Candidate ticket counts and reservations both check the full migrated cap
vector. An overfull accepted legacy window issues no new ticket. A reservation
failure rolls back the whole draw and clears its tickets; it cannot panic the
virtual processor or keep a partially reserved permit.

The native fee boundary fixture first prepares the same genuine signed floor
Final/anchor with a generous, unapproved R cap and no tested fee body. It records
every actual recipient migration row and their common R total. Baseline plus
the fixed 500,000 fee limit is then frozen in genesis Params before both actual
fee runs. Both must match the same baseline before carrying either fee; the
criterion never changes after a tested fee outcome. This is mechanics evidence,
not an economic profitability or soundness approval.

## Positive all-fund native settlement fixture

The private happy-path candidate fixes W=436, settled depth=1, and the existing
work-pricing fence at DAA 10,000 before genesis. Before that pricing fence the
actual accepted escrow supplies positive issuer, Panel, NativeSlice and
KernelFee legs in the toy class. This counterfactual mechanics profile is not
shipped pricing, D=30 evidence, economic approval or model soundness evidence.
It retains the existing REAL trace admission seam, real signed readiness,
Panel/assembler, genuine native Final, actual funding and all cryptographic
acceptance gates. A second genuine native Final supplies the configured depth.

The candidate checks actual poster debits and allocation conservation, every
positive queue's coinbase payment, separate physical R clocks, DB reopen,
duplicate licence, each actual payment+W boundary, and full chain/lane replay.
Its runtime result remains pending. Central integration f22a37d46 meanwhile
passed 89 focused core tests and 5 wallet tests, including the partial FD/FE
clock, collected market queue/phi and frozen Panel collector candidates.

## Funded kernel reporter debt and exact tranches (candidate, 2026-10-11)

The dormant budget's distinct origin 12 records the exact funded reporter award:
`reserved.R` is its immutable total, `consumed.R` is irrevocably assigned native
cash, and the remaining difference is debt. This row is outside the rolling R
window and grants no Q/B/F/T. Objective `AccuserReward` and `DemanderShare`
settlements first verify their amount against real `SlashFraud` / `SlashDefault`
collateral debits in the same authenticated kernel batch. They preserve the full
award without testing recipient R, so full R cannot suppress a conviction or
burn a legitimate collected entitlement. The actual native bond comes from the
kernel's authenticated bond mapping; a matching payout payload is never treated
as ownership. The old ledger root, ordered settlement batch, DAA and event ordinal
bind the immutable award identity.

Pre-object budget ticks redeem up to the available recipient R into a unique
`(award, consumed progress)` native payout row, provided the bounded native queue
has room. Larger-than-R awards therefore require successive windows; partial
tranches preserve total plus remaining debt exactly. Each tranche has its own
native queue receipt and physical coinbase + W clock. Sparse ticks cannot expire
an unpaid queued tranche. Actual payment changes neither the immutable award total
nor past cash; reorg restores only the reverted tranche's exact rows. Completed
award receipts remain rooted to prevent reclassification. Derived unpaid-award and
per-bond debt indexes are rebuilt on Borsh/import and row delta application; they
are absent from serialization and roots. Redemption scans unpaid debt rather than
completed award history, and unpaid debt retains the actual recipient's withdrawal
binding even when R is otherwise empty. No real preset or activation is enabled.

Candidate verification comprises Rust parse acceptance and `git diff --check`.
The new engine case tests a 125-unit award against R=50 with W=280 and W=436,
full-R deferral, queue-constrained partial tranches, sparse physical clocks,
Borsh restart, precise reorg/replay and exact final total. The common fold case
uses real registered bond debits and both settlement consumer kinds, then actual
production payout drain / carriage / delta APIs over three windows. Its emitted
settlement batch is explicitly a mechanics seam; it is not cryptographic FileProof
or native coinbase UTXO evidence. Central compilation/runtime are pending.

Native named/reveal reporter maxima, authenticated legacy pending/awarded rights
migration, whole material/producer liability retention and bounded completed-award
history compaction still need closure. This candidate is not full RFC08 completion,
all-model soundness, economic approval or production activation evidence.


### Dormant reporter monetary schema v2 (implementation candidate)

`palw_reporter_income_v2` is a distinct Some-only activation field. All shipped
presets leave it `None`. Its params/schedule identity names it, identity
normalization collapses `never()`, validation requires the bond budget at or
below it and then refuses all public arming. The private state mirror carries
its height independently of the budget. State v1 retains exactly its original
six-field Borsh wire and five table roots until this fence. State v2 appends the
new typed table6 reporter book; its digest is committed through the header
root, and every table6/header write is verified by exact forward/reverse delta.

A named native court/DA winner or valid earlier commit-reveal winner freezes
its authenticated bond, payout payload, full funded maximum and original
acceptance DAA before award. A pending winner keeps that bond collectible.
Classification after the reveal horizon records the whole funded debt, even
when recipient R is full or the total exceeds one R window. Redemption uses
the frozen payload, never a later bond payout lookup, and creates exact unique
R-limited native queue tranches. No reward, Q/B/F/T right or second award is
created by redemption. Each actual drain starts its own payment+W clock.
The native consumed-verdict test uses an explicit court-outcome seam with a
real collateral debit; it is not evidence that a model court is sound.

At the new fence, old pending winners whose rooted consumed-offence and best
record authenticate the actual bond retain their exact already-accepted cash,
even if that recipient is retired or today's R cap is overfull. Old awarded
cash whose carriage has only a payout payload is not attributed by matching
an address to a current bond. Its exact queue, amount and payload become an
explicit unattributed liability. Such accepted cash remains payable. New R
admission is conservatively quarantined globally until every such queue has
actually drained and its W elapsed. The old cumulative awarded counter cannot
prove a historical physical payment DAA: a separate first-fence+W uncertainty
boundary is committed without inventing a payment timestamp. An unpaid exact
queue outlives that boundary and cannot expire on a logical award clock.
This migration trades temporary global monetary admission delay for preserving
old rights without false ownership. Zero-R rights and objective conviction/
funded debt recording remain possible; positive new R admission refuses
without a partial monetary mutation.

New tests cover table6 wire/root, winner immutability, W280/436 legacy quarantine,
physical sparse drain, restart and exact reorg; native named debt above one cap;
and independent late reporter-fence migration over a live v1 engine. These are
parser-reviewed candidates pending the parent's central compile/runtime.
Remaining RFC08 work includes complete native all-positive Final validation,
all reporter pre-work maxima and material/liability lifecycle, bounded completed
award history, EVM H160 ownership and gas-income reservation, and full external
pruned-join/profitability evidence. No economic/security approval is implied.

## Material pruning while an accepted obligation remains

The node janitor previously deleted own Attempt captures by age regardless of an accepted DA duty,
and could delete owed foreign copies under its disk-space rule. The janitor now pins every capture
whose chain read reports `da_owed`, under both age and space rules, and returns the remaining space
deficit. It cannot treat a possible later replay as discharge of the accepted retention duty.
The regression covers own old Attempt captures, foreign Attempt and free-prompt copies, and release
after the obligation ends. This local repair does not extend a signed provider TTL, prove material
availability, or yet connect root-exact physical monetary payment+W to that chain read.

Verification at commit: Rust parser and `git diff --check` passed; runtime tests await the parent's
central build.
