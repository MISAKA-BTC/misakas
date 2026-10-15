# RFC-0012 PALW-only implementation

This source tree is for the new ruleset only. The old DNS BFT implementation and
validator workers have been removed, including `dns_bft.rs`, `dns_bft_v1.rs`,
`validator_service.rs`, the standalone `run`/`bond` commands, in-process validator
flags and DNS-dependent bridge pause policies. It cannot revalidate the old BFT
network history. Every preset has a new consensus identity and schedule marker;
use a fresh network/database rather than attaching this build to the old network.

Bonded-stake attestation/precommit quorum, DNS-final anchor advancement, stake
preference and the DNS-anchor reorg veto are deleted. The production virtual
processor no longer reads DNS state. Historical anchors cannot accelerate coinbase
maturity or certify EVM heads.

DNS participation transactions are rejected from genesis. The PALW comparator,
strict economic wins, shallow-race convergence, frontier provenance and missing
state refusal remain authoritative. DNS attestation/active-set RPCs return empty
results and precommit duty reports retirement at DAA 0. Reserved wire IDs and
historical payload layouts are retained to avoid reusing protocol identifiers.
PALW keys, producer/panel/court operations, wallet transactions and historical
bond exits remain available. DNS peer discovery is independent and remains available.

Native settlement certification still requires an explicit
`Params::palw_dns_retirement` policy. All shipped presets leave that policy `None`;
no production D/W thresholds, concentration caps or certification activation height
are assigned by this deletion. Without an active native policy, EVM `latest` names
an executed canonical block while `safe` and `finalized` are unavailable. Neither
an old DNS anchor nor an imported pruning point provides a native certificate.
The [RFC release gates](0012-palw-only-consensus-and-native-evm-settlement.md#8-implementation-sequence-and-release-gates)
still apply before deployment.

## Native EVM evidence and heads

The existing native lock/claim/refund/withdrawal executor remains authoritative:
claim before timeout, refund at/after timeout, same-block claim/refund exclusion,
scaling, tips, single credit/debit and parent-payload/child-result ordering remain
unchanged. The DNS Pause admission gate has been deleted. Acceptance
and settlement confidence remain separate facts.

The new native evidence row uses the canonical L1 sink hash as its generation.
It records policy/ruleset ids, PALW frontier, optional latest/safe/finalized heads,
eligible depth, decimal unique work and a stop reason. It is written in the same
virtual-state RocksDB batch as the legacy three-hash EVM-head row and UTXO/index
changes; the old singleton's Borsh layout remains unchanged.

* `latest` is the newest persisted canonical EVM execution result. Readers verify
  its committed execution root and selected-chain membership.
* `safe` is the deepest **contiguous executed prefix** covered by the PALW frontier,
  closed relevant claim/court/DA lifecycle and the configured D/W/concentration
  requirements. Work before the effect's accepting DAA/blue position does not
  count as subsequent evidence, including equal-DAA but earlier-blue work.
* `finalized` additionally requires the validated pruning point to be an executed
  ancestor of that certified safe prefix. Pruning alone supplies no certificate.

Evidence consists of retained Final non-floor REAL attempts with canonical work
identities/weights and eligible matured FP slices actually consumed in root-verified
canonical deltas. An unspent FP grant, heartbeat, floor or execution carrier alone
supplies no W. A slice is identified by `(canonical work id, quantum index)`; carrier
copies cannot supply duplicate credit. FP slice maturity is at least claim Final
and accepting DAA plus the existing committed execution-quantum maturity window.
FP pricing must be in the compute regime. D counts distinct qualifying grant/attempt
anchors, so multiple slices from one grant do not manufacture depth. W concentration
is measured by registered operator and class. These definitions need economic and
adversarial justification before production parameters are assigned.

PALW delta provenance follows the child header's commitment to its parent's state;
the sink is checked against its reconstructed candidate state. Missing/pruned
contributions are never guessed. Imported checkpoints clear native evidence and
safe/finalized pointers until reconstruction supplies proof. Readers reject mismatched
generations, policies, rulesets, roots and unordered head prefixes. A branch abandoning
a previously finalized head reports `FinalizedConflict` and logs a resync requirement;
the conflict remains sticky until a validated resync/import clears it. Incompatible or
unreadable persisted evidence is preserved for recovery and makes readers fail closed,
rather than being replaced with an empty certificate. Neither case reinstates DNS authority.

An absent ETH block tag returns `null`. State/call queries for unavailable executed
heads return an error, rather than reading tip state or inventing an empty account.
The evidence API also verifies endpoint roots and provenance. The legacy DAA-only
`settled` field requires a DAA strictly below the native safe head, because DAA alone
cannot identify ordering within an equal-DAA group. Exact head hashes are authoritative. Its guarded session
keeps retirement status, evidence and the legacy PALW status at one generation.

## Money and retained liabilities

New eligible non-floor attempts escrow 92% and useful-work inclusion earns up to 8%,
with zero new DNS subsidy. Normal and native-settlement transaction fees go to their
carrier (100% worker, zero DNS/service). This new ruleset applies its accounting from genesis; it does not replay old DNS-era earnings. Recovery floor retains its prior bounded
carve. The coinbase withholds the full new worker base; unallocated remainder is
never minted, preventing the retired 20% from becoming an immediate miner windfall.
Inclusion eligibility follows the existing entitlement/dedup checks and excludes
heartbeat, execution Round and floor useful-work credit. Retained quality obligations
continue through the old deferred payout path; no new DNS reward keys are generated.

## Readers and services

In-process and standalone DNS validator workers have been deleted.
Status reports the retired role; historical bond exits remain accessible. CLI,
TypeScript, gRPC and wRPC expose optional native evidence/retirement fields. wRPC
keeps the exact v1 encoding when those fields are absent and uses response v2 when
present. Old JSON without these fields remains readable. DNS-final coinbase acceleration
and wallet anchor polling are removed; the long DAA maturity remains authoritative.
Explorer overlay/finality pages become native settlement views after retirement;
bridge notices report native evidence availability instead of DNS funding/quorum waits.

## Activation work still required

The deletion is complete, but native settlement policy remains unassigned. This
change is not a production deployment or a PALW common-prefix security proof. Before assigning an activation:

* Justify D, W, concentration caps, evidence horizon and trusted-sync assumptions;
  measure private maturation, class/operator concentration and withholding attacks.
* Complete the RFC matrix on an activated private network: healthy zero-DNS REAL/FP
  work with deposits, withdrawals and market orders; sibling/deep forks and partitions;
  missing DA/open court; fee/subsidy and all retained-liability reconciliation;
  process crash, full restart and pruned IBD/reconstruction.
* Review conservative loss of retained work evidence after claim retirement/pruning,
  incremental certificate/index design and the cost of walking the retained chain.
  Current missing evidence stops certification; it is never an activation workaround.
* Complete external wallet/SDK/client compatibility and historical-exit acceptance,
  and operational alarm/resync handling for below-finalized conflicts.

Focused regression tests cover committed policy values, unset presets, duplicate
work, concentration/maturity/order/lifecycle stops, evidence windows, integer subsidy
conservation, opposed persisted DNS state, transaction/template refusal,
native generation mismatch and atomic row persistence.
PALW and native bridge/execution tests remain part of validation; they do not by themselves
satisfy the private-network activation matrix above.

## Validation of DNS authority removal

The related consensus, node, signer/key utility, CLI, gateway, extension and wallet
crates pass `cargo check --locked --tests` (nine selected packages). Focused tests
pass: 9 RFC-0012 core tests, 10 fingerprint/maturity tests, 14 signer tests,
32 key/exit tests and 8 consensus tests, for 73 tests total. The consensus tests
include opposed persisted DNS anchors, isolated participation rejection, unreadable
PALW state, EVM execution/claim templates and the long coinbase maturity fallback.
Shell scripts and both setup UI scripts also pass syntax checks.

The repository's repin enumeration checker still reports the same 107 pre-existing
unclassified occurrences as the base commit; this deletion adds no occurrences.
That checker is not a passing repository-wide gate. The activated private-network
matrix above remains outside this code-removal validation.
