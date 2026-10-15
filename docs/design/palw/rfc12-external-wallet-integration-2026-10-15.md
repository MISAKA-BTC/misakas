# RFC12 external wallet integration (R12-OPS), 2026-10-15

Status: source prepared 2026-10-15 (round 3). Nothing here has been compiled, tested by
cargo or run against a node. No PASS is claimed; no preset, fence or release height changes.

This note is for third-party wallets, SDKs and EVM tooling that sign MISAKA transactions
after DNS retirement and the native pruning-import changes. The first-party CLI already
applies these rules (`misaka/apps/cli/src/wallet.rs`, `validator_reader.rs`). External code
uses one versioned surface for the same rules.

## The surface

The surface is in the public wallet SDK crate `kaspa-wallet-core`, module
`misaka_preflight` (`wallet/core/src/misaka_preflight.rs`), with WASM bindings in
`wallet/core/src/wasm/misaka_preflight.rs`:

| Item | Purpose |
| --- | --- |
| `MisakaNodeCapabilitiesV1` (schema `misaka.wallet.capability.v1`, version 1) | What the node reports: genesis, durable import status, `dnsRetiredAt`, native settlement availability, DNS authority availability, the resulting `finalityAuthority` (`native` / `dns` / `unknown`), the sink DAA, and which reads failed. |
| `MisakaWalletExpectationsV1::for_params(&Params)` | What the wallet knows from its own chain params: the expected genesis and the scheduled retirement DAA. These values never come from the node. |
| `fetch_capabilities_v1(&dyn RpcApi)` | Reads `getPalwNodeStatus`, `getPalwSettlement(0)` and `getDnsConfirmation`. |
| `preflight(&dyn RpcApi, &expectations, &[Transaction])` | **The single call to make before signing.** Brackets settlement/DNS reads with node identity and durable import epoch. |
| `evaluate_preflight_v1` / `evaluate_preflight_json_v1` | The same verdict on reads the caller already holds (native / JSON). |
| JS: `misakaCapabilitiesFromReadsV1(statusJson, settlementJson, dnsJson)`, `misakaPreflightV1(capabilitiesJson, expectationsJson, subnetworkIdsJson)` | The WASM form. Pass `""` for a read that failed. |

## Refusals

A refusal means the wallet does not choose funds and does not sign. These are the
refusal codes:

* `invalidCapabilities`: the native capability has another schema/version, or
  its displayed authority contradicts its underlying retirement/DNS facts.
  The native and JSON/WASM evaluators apply the same check. A serialized `dns`
  or `native` label cannot turn failed reads into authority.

1. `genesisMismatch`: the node runs a genesis other than the one in the wallet's params.
   A drill and the public network can share a network name.
2. `importStatusMissing`: the wallet's params schedule native retirement, or the node
   reports that retirement has happened, but the node does not report durable import status.
3. `importIncomplete`: the node is installing a pruning snapshot. The import is incomplete
   when it is not stable, when it has a session point, or when its phase is neither `idle`
   nor `legacy-stable`.
4. `finalityAuthorityUnknown`: both the settlement read and the DNS authority failed.
   The wallet must not show a fallback "DNS active" headline in this case.
5. `retiredKind`: a transaction on subnetwork `0x10` (new DNS bond), `0x11` (attestation
   shard) or `0x19` (precommit). This applies once retirement is active, or once it is
   scheduled within 1,000 DAA of the node's sink, because a transaction signed now may be
   included later. Bond unbond and exit stay available.

A pass is only a precondition for signing. It does not show that a payment is settled.
Settlement depth still comes from `getPalwSettlement(daa)`, and it counts settled anchors,
not blocks.

## Versioning

* A change to the capability schema adds `…V2` types and a new schema string. Version 1
  stays as specified here. `evaluate_preflight_json_v1` refuses an unknown schema or version.
* wRPC keeps the exact v1 response encoding when the optional native fields are absent
  (R12-OPS item 3). A missing field is reported as unavailable. It is never read as a pass.

## EVM tooling

EVM JSON-RPC tooling that signs native-lane deposits or withdrawals must run the same
preflight against the node that serves its native reads. The EVM endpoint does not carry
the import or finality facts. Until a tool does this, its native-lane signing should be
treated as unsupported under retirement.

## What remains

* No external wallet, SDK or EVM tool has been migrated or tested against this surface.
  Third-party migration is still EXTERNAL.
* The 1,000 DAA retirement signing margin is a proposal and needs USER REVIEW.
* The JSON and WASM forms have not been built for wasm32 here.

The follow-up consistency controls cover native callers with an unsupported
version and both native/JSON callers whose authority label was substituted after
failed reads. They are source only and have not been executed. The authority
check does not assert Safe/Final availability or payment settlement.

The asynchronous signing preflight now rejects an identity/import epoch change
between its two status reads, including stable → import → stable. Ordinary
producer counters may advance without invalidating the bracket. A legacy node
that lacks both status reads still goes through the existing genesis/import
expectations; this does not certify missing native import diagnostics. The
three-read advisory capability fetch and caller-supplied JSON reads do not by
themselves establish this bracket. Callers supplying reads own their coherence.

The server's settlement RPC separately brackets the sink, pruning boundary and
durable import state around all settlement/readiness queries. It derives the
retirement decision and reported DAA from the captured sink header, requires any
native snapshot to name that generation, and returns a retry error on movement.
A read crossing the retirement fence cannot publish old DNS authority with new
native heads. New source controls cover epoch, sink and pruning changes; these
are not executed concurrency or real-node acceptance results.
