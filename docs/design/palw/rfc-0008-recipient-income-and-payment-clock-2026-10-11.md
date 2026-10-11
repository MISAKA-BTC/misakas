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
