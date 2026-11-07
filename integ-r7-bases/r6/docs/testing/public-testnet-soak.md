# Public testnet soak — plan

The last gate before a mainnet GO decision, and the first test of this work on a network nobody
controls. Everything before it ran against fixtures whose shape was chosen to expose the bug. This
one runs against whatever the network actually does.

**Nothing here has been deployed.** The plan and its tooling exist; putting a release candidate on
public testnet nodes is an outward-facing, hard-to-reverse action on a live network and needs an
explicit decision. See *Before anything is deployed* below.

## What it is trying to falsify

Not "does it converge" — that has been measured. This is looking for the things a fixture cannot
produce: real peer churn, real clock skew, nodes on older binaries, partitions with shapes nobody
designed, and the long tail of a system running for days rather than minutes.

The soak passes only if, across the whole window:

| Signal | Threshold | Why it is the one that matters |
|---|---|---|
| Wrong-chain commit | **0** | A node acting on a branch the network did not take is the incident being fixed |
| Signer leakage | **0** | Attesting or mining while the gate was holding means the gate is decorative |
| Panic | **0** | One was found by a single VPS round; days of real traffic is where the rest live |
| Permanent quarantine | **0** | Fail-closed is correct; fail-closed-forever without cause is an outage |
| Permit reuse | **0** | A permit redeemed twice means the binding is not binding |
| Manual datadir deletion | **0** | If recovery needs an operator with a shovel, it is not recovery |

Zero is the threshold for every one of them. These are not rates to be minimised; each is a
statement that something impossible happened.

## Shape

Ten to twenty nodes, seven days minimum, fourteen preferred. Fewer than seven days does not reach
the second and third pruning-point crossings, which is where the review interacts with real depth.

| Role | Count | Purpose |
|---|---|---|
| Baseline | 4–6 | Current release, unmodified. The control: their behaviour is what "normal" means this week |
| Candidate | 4–6 | The release candidate. Mixed hardware and regions |
| Validator | 2–3 | Candidate build with a bond, actually attesting. Where signer leakage would show |
| Churner | 2–3 | Candidate build, restarted and partitioned on a schedule |
| Straggler | 1–2 | **Older binary, older consensus params.** Their handshake must be refused, not tolerated |

The straggler is not padding. `consensus_params_id` in the handshake is meant to reject a peer that
cannot state which rules it runs, and that rejection has only ever been tested against a fixture.

## Faults, on a schedule rather than by hand

Applied to churners and validators, never to baselines — the baselines have to stay clean or there
is nothing to compare against.

| Fault | Cadence | What it is probing |
|---|---|---|
| Restart (SIGTERM) | 2/day/churner | Ordinary operator action |
| SIGKILL | 1/day/churner | Torn shutdown; the gate must survive without a graceful write |
| OOM (cgroup limit) | 1/2 days | Death partway through an IBD, which quarantines by design |
| Partition (30–600s) | 2/day | Heal must not need an operator |
| Proof timeout (drop `PruningPointProof` on one peer) | continuous on 1 node | The lease and rotation, on a network |
| Peer churn | continuous | Sources appearing and vanishing under a nomination |
| Candidate conflict | as it occurs | Two chains offered at once — the actual scenario |

The last is the one that cannot be scheduled. If the network never forks during the window, the
soak has not tested convergence at all, only that nothing else broke — and that has to be stated in
the result rather than glossed. A fork can be induced deliberately at the end of the window if none
occurred naturally, on the churner subset only.

## What is collected, and how

`testing/scripts/soak_collect.sh`, every 60s per node:

- chain identity: pruning point, sink, blue work, DAA score
- participation: the gate's own log line, and `is_synced` from the RPC
- health: panics, quarantine entries, permit grants, IBD outcomes
- the binary each node is running, so a result is attributable

Disagreement is computed across nodes rather than per node: the wrong-chain signal is a node whose
pruning point differs from the majority for longer than a pruning period, which is the only
definition that does not require knowing in advance which chain is right.

**Known gap.** The participation gate is observable only through its log line — there is no RPC
field for it. The collector therefore reads logs over ssh, which works but is fragile and needs
shell access to every node. Adding the state to `GetServerInfo` is the right fix and is
deliberately **not** being done now: it is a wire-format change that would void RC7's 240 rounds
and 20 VPS rounds. It should be done before mainnet, and the soak result should be read knowing
that an operator today cannot ask their node why it is not mining without reading its log.

## Before anything is deployed

1. RC7's outstanding series finish clean (500 fresh scenarios across three hosts).
2. The candidate binary is built once and its SHA-256 recorded; every node runs that exact file.
3. Baseline nodes are confirmed on the current release, and their chain identity recorded, so
   "the network moved" can be told from "the candidate moved".
4. A rollback is written down and rehearsed on one node: stop, restore previous binary, start,
   confirm it rejoins. Untested rollback is not rollback.
5. **Explicit go-ahead from the operator.** These are live testnet nodes serving real peers.

## What the result can and cannot say

A clean fourteen-day soak across twenty nodes says the candidate did not misbehave under the
conditions that occurred. It does not establish a failure rate: the events being counted are
supposed to be impossible, so seeing none of them constrains the rate only as far as the window
allows.

It also says nothing about UTXO, overlay, or registry determinism. This work converges nodes onto
the same *chain*; two nodes that disagree about the *state* of that chain will still disagree, and
this soak is not built to notice.
