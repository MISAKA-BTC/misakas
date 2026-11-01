# T12 connected peers and handshake observations — 2026-11-01

This change repairs Explorer observation. It changes no fork-id decision, consensus rule,
node configuration, topology, hold, DB or claim. Deployment and commit remain parent-owned.
The unrelated metadata-feed publisher WIP is frozen, with all six file hashes unchanged.

## Observed source error and real partition evidence

The existing `misakascan-peer-census.service` ran
`/usr/local/bin/misakascan-peer-census.py` without a log override. That script read
`/root/.t11/misaka-testnet-11/logs/rusty-kaspa.log`. The public JSON had a fresh generated
time but null network/DAA, zero rejection rows and a historical repair suggestion. This
was not a verified zero from the current T12 log. The live b6 unit's appdir is
`/root/.t12r-b6`; its actual log is
`/root/.t12r-b6/misaka-testnet-12/logs/rusty-kaspa.log`.

A bounded read of that current log shows both directions: b6 at DAA18935, crossed18640,
rejects a session advertising next=`18446744073709551615` while it expects18640; a received
rejection reports a remote node at DAA19019, crossed18400, rejecting an unknown fired set.
The quoted DAA/crossed fence in a received rejection belongs to the remote author. These
are real handshake disagreements, rather than merely an omitted IP in the UI. They do
not establish which branch has majority support or the source revision of the operator.
The checked-in fixture preserves those messages with its IP replaced by a TEST-NET IP.

The [direct95/public identity proof](peers-2026-11-01-peer95-primary-identity-proof.json)
records the same P2P identity `52196ead-085a-408b-a1c4-5cda62c769fd`, EDF/4024 IDs and fence
schedule including18400 and18640 through direct95 localhost RPC and public `/kaspa` at
2026-11-01T14:00:27Z. The local forwarding process was verified as SSH to95 with
39314→localhost26314. The hypothesis that this public observer lacks18640 contradicts
those observations. A matching version label is not a git-revision attestation.

At the earlier13:32Z peer snapshot, `169.58.13.16` was an actual connected peer at both
public vantages, with the same `8ad0390d-8a1e-4835-90b6-f8403da65d8f` identity. Its role,
source revision and chain state remain unknown; being connected does not establish that
it is an official seed. Peer counts from these two vantages cannot establish majority.

## Replacement source and UI behavior

`contrib/misakascan-t12/peer-census.py` reads at most32MiB of the configured log. It checks
that the running unit's actual appdir resolves to that log and that the same process owns
the selected local RPC listener. The adjacent bounded Node20 helper reads public runtime
network, genesis, params, schedule, fences, DAA and P2P identity. Wrong/missing runtime,
wrong log binding, stale timestamps or unparsed rejection records yield `available=false`.
The [read-only probe](peers-2026-11-01-census-readonly-runtime-proof.json) at14:01:27Z
verified the live binding and found423 event groups in a bounded tail. It did not run the
writer CLI or publish a JSON file. Later malformed-runtime validation tightening was
covered locally; the saved actual runtime has the required field shapes.

The UI verifies source freshness/genesis and distinguishes unavailable observation from
an available tail with no events. Window coverage is explicit. Rejections are session
observations with IP, remote port, direction and mismatch, separate from connected peer
identities. It retains both successful and refused sessions behind one IP/NAT. Current RPC
network/DAA/params/schedule/fences/identity appear in their own table, including unavailable
values when an individual method cannot respond. Existing connected lists and deduplication
remain intact. No rejection is automatically labelled an outdated binary.

The port explanation now distinguishes inbound ephemeral remote ports from outbound
listening ports. Lookup by IP cannot identify one individual node behind shared NAT.

## Verification and deployment boundary

Python5/5; isolated UI20/20; independent live candidate20/20; original-main preserving
candidate16/16. The live set additionally verifies prior classification, arrival ordering,
home cold-fill and body hydration fixes. The main set excludes its independent older
classification implementation. Source/helper/candidate syntax checks passed. Both clean
three-way candidates preserve every byte outside the census/Peers section. These are
fixture/source checks, not a production browser-pixel check or Qwen acceptance evidence.

[Repair manifest](peers-2026-11-01-repair.json) records source/candidate hashes and exact
boundaries. The live preimage is SHA13176ce4; the new candidate is SHA862c49ff. Original
main preimage is SHA6c599167; its candidate is SHAf8e730ac. Neither was installed by this
agent. Parent must recheck exact preimages before replacing files; original user edits
and unrelated live fixes must remain.

`contrib/misakascan-t12/PEER-CENSUS.md` and `peer-census-t12.conf` contain the service plan:
back up the old script/service/drop-ins/public JSON; install adjacent helpers under
`/usr/local/libexec/`; validate with a temporary CENSUS_OUT; compare runtime identity; then
use the existing two-minute census timer. Roll back only these added files/drop-in and
backed-up public JSON. No node restart or hold release is part of this repair.
