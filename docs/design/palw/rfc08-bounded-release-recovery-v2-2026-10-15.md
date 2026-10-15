# RFC08 bounded release recovery V2 — source checkpoint, 2026-10-15

Status: implementation source only. No build, test, node run, benchmark or acceptance measurement has been executed for this checkpoint. All public presets keep this fence `None`; its validator refuses activation while cost and provider-ACK liveness approval are unavailable. This document does not approve economic terms or declare RFC08 complete.

## Failure addressed

A V1 provider which missed the entire accepted Release interval could not recover its deletion authority after the old native delta/header was pruned. An absent covenant correctly retained material at MAX, but that was not a complete recovery protocol. Keeping an unbounded release index or treating absence as a receipt would not solve it safely.

A second issue arises after the last provider ACK: the terminal receipt disappears. A provider offline across its ACK acceptance and historical pruning cannot retire its outbox from old carrier inclusion. Moreover inclusion by itself does not prove that the lifecycle object applied: `palw_accept.rs` validates and rehearses each object and may drop it while retaining the carrying block.

## Explicit new contract and bounded state

`Params.palw_material_release_recovery_v2` and its Borsh-skipped state mirror independently own `PalwMonetaryMaterialActionV1::RecoveryV2` (appended discriminant 7 beneath existing outer tag 242). Old V1 variant bytes, Release discriminant 6, PAV3 header/Ready meanings and private paid-v3 factory bytes are unchanged. A2 classification, native signature checking, transition checks, Params identity/scale/reflection, and restore consistency include the new fence.

Recovery enrollment requires a fresh, unused, sequence-zero monetary covenant, sponsor and all provider signatures over a distinct recovery domain. It cannot backfill unavailable historical generations for an old used covenant. New work after this gate requires explicit enrollment before the first consumer. Replacement preserves existing paid/earned rights and requires both ordinary V1 replacement signatures and the additional recovery signatures of all new providers.

Aux134 has two exact key grammars:

* Root64: `PalwMaterialRecoveryRowV2::Active/Terminal`, including bounded signed generation/provider history.
* `0xA1 || Borsh(PalwBondKeyV2)` (69 bytes): `PalwMaterialProviderAckCheckpointV2`, one current ACK stream checkpoint per consenting provider identity.

Policy names finite reserved-contract, generations, history-cell and provider-stream capacities. Enrollment reserves the whole history envelope, terminal slot, provider stream slots and ACK sequence credits before recovery admission. Replacement reserves new distinct-provider slots/credits before its funding writes. Capacity refusal preserves the prior state and earned cash. Release needs no new table slot: after unchanged native actual-tail/finality and no-live-right checks, it replaces the existing Active row with a compact Terminal receipt. All distinct providers from every accepted generation must ACK before this receipt is removed.

Each ACK binds exact stream, previous receipt, receipt id and checked next sequence. The fold updates the provider checkpoint only when that ACK actually applies and consumes a previously reserved credit. Current native-finalized presence of the exact checkpoint therefore proves application after the last terminal receipt and historical carrier/delta have disappeared. Stream retirement is separately provider-signed and refuses every accepted active or unacknowledged terminal obligation and nonzero reserved credit. Local outbox emptiness is not a consensus authority.

## Provider recovery and ACK consumer

The local Consensus API reconstructs the parent-state committed by a current native-finalized header, checks its committed root and strong consistency, and refuses incomplete/moving pruning imports or changed virtual tips. A terminal receipt present in this state authorizes only its exact root and recorded stored generation. It does not need the old Release header/delta. Missing/reconstruction-unknown facts retain material.

Op242 already authenticates the complete aux table. The proof pager now retains and structurally preflights aux134 rows as well as aux120; `finish_with_recovery_v2` alone constructs `ProvenMonetarySnapshotV2`. The V2 remote GC oracle requires this complete proof plus the separately configured native finality authority. Intermediate pages and RPC availability booleans are not release authority.

The provider GC janitor tries the current finalized terminal receipt even if it never observed the active covenant. Its deletion intent and exact native authority-index cleanup receipts remain fsynced/replayable across crashes. The provider signs an ACK only after every generation it served has a verified durable deletion receipt and authority metadata cleanup is fsynced. Its outbox uses an exclusive kernel lease, bounded structural preflight and atomic publication. Exactly one ACK per provider may be signed/in flight. The next ACK cannot be signed until exact current-finalized stream/sequence/receipt presence retires the preceding outbox entry durably. This prevents a later local ACK from overwriting the one checkpoint needed after a long offline interval.

`into_hook_with_service_v4` exposes the hosted service to an explicitly configured private fee-payer adapter. The parent integration owns the private v4 funding UTXO/key/fee cap, actual carrier signing, mempool submission/retry and checkpoint retirement worker. Submission itself is never retirement authority. Runtime creation and provider/sponsor signing of a new recovery enrollment before native consumer admission remains a separate source integration task; a fixture bootstrap alone does not satisfy it.

## Native allocation ownership

Provider snapshots now use a retained native owner for their full lifetime. Reopen preflights the outer provider store, all embedded material bundles and embedded kernel position/segmented response codecs before Borsh. The owner-specific mutation path derives logical requests from actual typed heap/memberships, canonical encoded byte counts, concrete BTree allocation envelopes and coexisting decode/response/publication copies. Raw signed material capacity is a separate storage limit and is not relabeled as native RAM or physical RSS approval. Strict bare-value extraction is refused; owned answer/publication methods retain their tickets. Physical allocator/RSS approval and all-model actual acceptance still require the later measurement phase.

## Prepared source controls; unexecuted

The source includes fold controls for fresh enrollment, reservation, unchanged state on duplicate/invalid ACK, old replacement rejection, terminal-to-ACK checkpoint persistence, explicit stream retirement and legacy Release wire pin. Outbox controls exercise one in-flight signature, durable reopen, wrong-checkpoint retention and retirement without historical carrier data. The provider decoder has malformed nested-codec controls before native authority callbacks. These fixtures are deliberately identified as storage/fold authority seams; fake signatures/finality callbacks do not demonstrate real native computation, positive payment or selected-chain finality.

Still required: complete runtime V4 enrollment/answer transport consumers, all-target positive money/provider deletion/restart/reorg controls on actual nodes, and economic/resource measurements plus independent review. A gate refusing unapproved values is not counted as completion of these items.
