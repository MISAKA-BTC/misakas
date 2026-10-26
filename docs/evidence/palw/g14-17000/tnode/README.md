# G14 17,000 slice — lane TNODE evidence (run at c335f4e4e)

Run: `python3 scripts/drills/g14_tnode_slice.py --salt <openssl rand -hex 32> --at 60 --work <scratch>/work --out-dir <scratch>/out`
(release build `cargo build --release -p kaspad -p misaka-cli --features kaspad/private-integration-config`, base d8aded6de + lane commits).

Binaries (sha256): misaka-rfc12-private-node `143b7f2a72533e3a656320402afb6b3ea64a92baff03ed49370edbfb945fc867`,
kaspad `d4f6d9c5d62924892ba37db0135fb75409d2a0a45ab53c667c170766ab9abed8`, misaka `6be1a4fa234835cd2bdd8f1c8ef13e34492e50ea27f551c25d4088856edba343`.

Result: **no node was started** — the kernel-slice ruleset is refused by the whole V2 validation:
`palw_settlement_once_v1 requires the ADR-0176 bond budget fence at or below it`. `PALW_T12_KERNEL_SLICE_FENCES_V1`
(`consensus/core/src/config/params/full_activation_v1.rs`) arms settlement-once without `palw_bond_budget_v1`. This is slice fence
composition (lane CLOCK); TNODE does not change the list. All requirement files are `passed: false`.

Per requirement (`run-c335f4e4e/req-*.json`, the pin script's form; it refuses files under `docs/`, so copy out of docs before pinning a
later passing run):

| requirement | rests on | status |
|---|---|---|
| kernel-route `palw_probabilistic_constraints_v1` #1 | T01 T03 T05 T14 | not passed (T01/T03/T05 not implemented on node, T14 blocked) |
| kernel-route `palw_probabilistic_constraints_v1` #4 | — | WAIVER (user 2026-10-26: RFC-0011 §15.7 shadow comparison treated as complete temporarily, not measured) |
| kernel-route `palw_probabilistic_constraints_v1` #7 | T14 T15 | blocked (slice ruleset) |
| g14 `palw_g14_route_required_v1` #2 | T15 | blocked |
| g14 `palw_g14_route_required_v1` #3 | T14 T15 | blocked |

Test-only values: drill salt, drill records (labelled, never pinned), drill keys of one host (not independence / drawn-approval evidence).
No cryptographic computation proof is used (PRINCIPLES §8).
