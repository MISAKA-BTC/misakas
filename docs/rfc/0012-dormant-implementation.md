# RFC-0012 dormant implementation

Status: implemented behind an **unassigned** `Params::palw_dns_retirement` fence.
All shipped network presets use `None`. No network, activation height, D/W threshold,
operator/class concentration cap, or legacy evidence horizon is assigned by this implementation.
There is no CLI override for this policy. Test fixtures do not constitute deployment settings.
The [RFC's release gates](0012-palw-only-consensus-and-native-evm-settlement.md#8-implementation-sequence-and-release-gates)
remain mandatory before activation.

## Coordinated transition

`PalwDnsRetirementV1` commits its activation, policy version and all policy values,
legacy evidence horizon, subsidy/fee schedule, retired transaction kinds, and native
snapshot version into both the consensus-parameter and schedule identities. `None`
contributes no new bytes; existing preset fingerprint vectors remain the reference.

Fork choice resolves retirement at the incumbent sink. After retirement, the DNS
BFT veto and stale-stake preferred-tip selection abstain; the existing full PALW
state comparator, frontier/maturity rules, provenance checks, strict wins, shallow
tie rules and missing-data refusal remain in force. DNS confirmation updates stop.
Peer-discovery DNS and header ordering as a download hint are unaffected.

Transaction admission resolves the fence at the accepting DAA score. New DNS bonds
(`0x10`), attestation shards (`0x11`) and precommits (`0x19`) are rejected in header
context, populated UTXO context and template selection. Historical equivocation is
allowed only through the configured finite accepting-DAA horizon, with both signed
targets strictly before retirement; normal evidence signature checks still apply.
Bond unbond/exit validation, historical snapshots, codecs, PALW producer/seat bonds,
panel/court/slashing, payout types and shared signing-key loaders remain available.

## Native EVM evidence and heads

The existing native lock/claim/refund/withdrawal executor remains authoritative:
claim before timeout, refund at/after timeout, same-block claim/refund exclusion,
scaling, tips, single credit/debit and parent-payload/child-result ordering remain
unchanged. Retirement releases the optional DNS Pause admission gate. Acceptance
and settlement confidence remain separate facts.

The new native evidence row uses the canonical L1 sink hash as its generation.
It records policy/ruleset ids, PALW frontier, optional latest/safe/finalized heads,
eligible depth, decimal unique work and a stop reason. It is written in the same
virtual-state RocksDB batch as the legacy three-hash EVM-head row and UTXO/index
changes; the old singleton's Borsh layout remains unchanged.

* `latest` is the newest persisted canonical EVM execution result. Readers verify its committed execution root and selected-chain
  membership. It is a fact about execution and is reported even when a certificate cannot be built (and in `FinalizedConflict`).
* `safe` is the deepest **contiguous executed prefix** covered by the PALW frontier,
  closed relevant claim/court/DA lifecycle and the configured D/W/concentration
  requirements. Work before the effect's accepting DAA/blue position does not
  count as subsequent evidence, including equal-DAA but earlier-blue work.
* `finalized` additionally requires the validated pruning point to be an executed
  ancestor of that certified safe prefix. Pruning alone supplies no certificate.

Evidence consists of REAL (non-floor) attempt claims read at their `Final` transition and matured free-prompt slices read at the spend
that consumed them, **both taken from the PALW delta of the chain block that carried them** — not from the claims the sink still holds, which
are retired at `terminal + claim_retirement` before their trace retention lapses (see the [policy proposal](../design/palw/rfc-0012-policy-proposal.md)
§0). A `Final → Voided` conviction anywhere on the chain retracts what the `Final` counted. A fact is mature at
`max(trace_retention, Final + claim_retirement)` (and, for a slice, no earlier than the spend plus the committed execution-quantum maturity
window); an unspent FP grant, heartbeat, floor or execution carrier alone supplies no W. A slice is identified by
`(canonical work id, quantum index)`; carrier copies cannot supply duplicate credit. FP pricing must be in the compute regime. D counts
distinct qualifying grant/attempt anchors, so multiple slices from one grant do not manufacture depth. W concentration is measured by
registered operator and class. The per-block extraction is cached in memory by block hash (rebuilt from the retained deltas after a restart)
and certified by an O(N + F) sweep. These definitions still need economic and adversarial justification before production parameters are
assigned; the proposal carries the measurements.

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
carrier (100% worker, zero DNS/service). Allocation uses the lower earning/paying
DAA for claim escrow. Old claims retain their original recorded entitlement; a later
payer does not retroactively increase it. Recovery floor retains its prior bounded
carve. The coinbase withholds the full new worker base; unallocated remainder is
never minted, preventing the retired 20% from becoming an immediate miner windfall.
Inclusion eligibility follows the existing entitlement/dedup checks and excludes
heartbeat, execution Round and floor useful-work credit. Retained quality obligations
continue through the old deferred payout path; no new DNS reward keys are generated.

## Readers and services

In-process and standalone DNS validator workers stop signing/funding after retirement.
Status reports the retired role; historical bond exits remain accessible. CLI,
TypeScript, gRPC and wRPC expose optional native evidence/retirement fields. wRPC
keeps the exact v1 encoding when those fields are absent and uses response v2 when
present. Old JSON without these fields remains readable. The wallet clears an
explicitly unavailable DNS shortcut and retains its long coinbase-maturity fallback.
Explorer overlay/finality pages become native settlement views after retirement;
bridge notices report native evidence availability instead of DNS funding/quorum waits.

## Activation work still required

This is dormant engineering support, **not a completed production migration or a PALW common-prefix security proof**. Status after lane X12
([record](../design/palw/rfc-0012-implementation-record.md), [policy proposal](../design/palw/rfc-0012-policy-proposal.md)):

* **Policy.** D, W, concentration caps, evidence horizon and trusted-sync assumptions are *proposed with measurements*, not assigned. Open:
  the maturity-rule decision (≥ 7.5 days to the first `safe`), a live census of operators / classes / `pwu` per window, release builds.
* **Private-network matrix.** Run in-process (`rfc12_zero_dns_matrix`: healthy zero-DNS deposits, withdrawals and market orders across the
  fence; opposed DNS state; sibling / deep forks and partitions; fee / subsidy / retained-liability reconciliation; restart; old-version node;
  finalized conflict; lost history; historical exit; hostile block). Open: a real multi-node drill with real `Final` claims through the shipped
  binary, a pruned join with EVM state, and market fills (release gate 4).
* **Evidence loss and cost.** Done: evidence survives claim retirement (read from deltas), a missing delta stops certification, the walk is cached
  and the sweep is O(N + F). Open: a >100,000-block processor run and release-build timings.
* **Clients and operations.** Done for the wallet, CLI, wRPC / gRPC and the first-party explorer; the `FinalizedConflict` alarm and resync are
  implemented, the pager rule and runbook are OPS. Open: third-party wallets / SDKs / EVM tooling (EXTERNAL).

Focused regression tests cover committed policy values, unset presets, duplicate
work, concentration/maturity/order/lifecycle stops, evidence windows, integer subsidy
conservation, opposed DNS state at the retirement fence, transaction/template refusal,
old entitlement preservation, native generation mismatch and atomic row persistence.
Legacy bridge/execution/reorg tests remain part of validation; they do not by themselves
satisfy the private-network activation matrix above.
