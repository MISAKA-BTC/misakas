# Native Panel signed lifetime V2: source checkpoint (2026-10-14)

Status: implementation in progress, not compiled or accepted. The user requested
all code first and a consolidated validation phase afterwards. Existing shipped
presets keep the fence absent. Public activation remains refused pending the
scoped source, security, economics and consumer acceptance reviews.

## Non-circular consent and native admission

A producer cannot sign its future exact native claim hash in the carrying body:
the claim/work/trace statement depends on the native execution anchor and body.
Inner 66 therefore signs `PanelLifetimeAuthorizationV2` before admission. It
binds the producer's registered bond, class, network/ruleset, unique nonce,
absolute acceptance interval, absolute service and liability endpoints, retry
and total pause budgets, and maximum actual PWU/reserved/escrow/rights amounts.
No future claim or work hash is in that prospective consent.

Before any reservation write, the native Attempt and FreePrompt birth paths
select an unconsumed matching producer authorization in canonical key order.
They check actual execution and monetary quantities against the signed maxima.
After native birth, the authenticated native execution signature and the signed
prospective authorization compose consent to the exact admitted statement.
The fold captures actual claim/work/immutable fields before Panel entropy. It
installs the exact enrollment immediately, retaining the actual birth DAA/block;
there is no producer-selected delay, replacement admission or new epoch.

Inner 65 is an optional additional exact producer reaffirmation only while the
already admitted Panel claim is PendingSeal. It cannot replace any captured
field, extend a clock, reset a pause counter or change a seed. The SDK derives
this draft from authenticated native state, never from a forecast claim.

## Rooted rows and bounded history

Aux table 121 contains three disjoint key namespaces:

- 65-byte key `0x41 || authorization_id`: signed authorization, actual authorizing
  DAA/block, signer, compiled policy and optional consumed claim.
- 65-byte key `0x42 || claim_id`: exact native admission consent capture.
- 64-byte claim key: captured absolute lifetime, counters and authority kind
  (composed native signatures, or additional exact reaffirmation).

Each admitted authorization contributes at most one capture and one exact row.
The global and per-producer admission counters count retained authorizations;
terminal/restarted/reorged claims cannot renew this lifetime capacity. Restart
and IBD consistency checks reject unknown namespaces, substituted domains,
missing/duplicate consumption, future positions, policy changes, monetary cap
violations and disagreement with Panel's immutable admitted statement. The
existing journal reverses all writes together.

## Consumer connections

Panel assignment requires a complete receipt window within the absolute service
endpoint; retries share that endpoint. Unfinished service reaches neutral expiry
except while its authenticated court remains paused. Native direct/sharded,
vertex, batch and audit receipt consumers use actual inclusion DAA and charge an
open pause interval as well as rooted closed intervals. Expired batch leaves are
inert independently. Court/DA/held-session extension consumers must fit the
signed liability endpoint, including the actual model-specific court tail.
New funded Panel sources must cover the frozen liability deadline.

A finite Panel service contract does not extinguish earned cash. RFC08's monetary
material covenant retains its separate actual-payment and selected-chain-finality
requirements; an unpaid or unknown cash path still holds its material.

## SDK and CLI

`palw-class panel-lifetime-v2 authorize|reaffirm|sign|inspect` restores a native
carriage against a separately caller-authenticated selected-chain state root.
Authorization preparation resolves the actual registered producer/class and
model-specific court window. Signing requires the key to match the registered
active bond's public key and signs inner 65/66 with the native route context.
The CLI writes exclusive output files and does not alter compiled activation.
Public presets with an absent fence refuse preparation as the node does.

## Deferred consolidated validation

Required tests include signed 66 -> native useful Attempt and FreePrompt birth ->
pre-entropy capture -> bounded Panel service/court -> exact native payment;
wrong actor/class/network/caps, multiple authorizations and canonical selection;
expired admission, sparse DAA, same-block court close, repeated pause/retry,
immutable optional 65, duplicate/reorg/restart/IBD row substitution, and all receipt
forms. Code formatting is not evidence that these scenarios pass. No new build,
test, benchmark or real-node acceptance has been executed for this checkpoint.
