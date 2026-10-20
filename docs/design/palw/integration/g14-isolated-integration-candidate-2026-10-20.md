# G14 isolated integration candidate — 2026-10-20

Status: implementation and verification in progress; no activation or production adoption.

The user approved ADR-0194 A (「A：隔離候補で実装・検証（推奨）」). Candidate branch `codex/rfc14-g14-integration-candidate` starts at X3 `0c3cb9f93`; selected main changes come from `0f808b81d`. Neither source branch is merged or rewritten. `pre`, live presets and live t12 remain unchanged.

## Schema and source boundaries

X3 key tags 0..11 and row tags 0..17 retain their existing order and meaning, including container key 11 and row 17. The new service key is 12, row 18. Its type/module is `CourtServiceStateV2` / `palw_court_service_state_v2`, with strict payload version 2. Its profile identity is a BLAKE2b-512 derivation under `misaka-palw/court-service/v2` from the explicit source profile ID; validation checks the derivation. This namespace is an isolated candidate, not an adopted record. Old main v1 service snapshots are not imported or converted. Tests check old tag confusion, complete byte consumption and version/profile substitution refusal.

The service reservation arithmetic remains the v1 component; its enclosing state/profile has the new version. Existing state accessor/hold helper suffixes remain internal API names, not wire version claims. X3 batched clocks, owned/carrier processing, transport costs and Proposed filing clock V2 are retained. Main legacy clock is not substituted for them. Lifecycle tag 230 is unchanged.

Service default remains denial-only, even after quota is supplied. Full duty graph, actual admission and first-accepting-block reserved supply, physical capacity and owner-independent public ingress remain incomplete. No empty record list is filled with synthetic values. Rooted custody/Final/bond/evidence holds and recovery are ported for joint verification. The imported funded relay test remains a legacy disclosure route; its node/UTXO/full-block-replay scope does not establish TIR-original T08 or T05.

## Documentation and evidence

RFC-0024/0025/0026 and user RFC-0027/Account Bond material are imported with original asset bytes preserved. [Byte comparison](../../../evidence/palw/g14-integration-candidate-2026-10-20/imported-assets-comparison.json) compares source assets, not network measurements. ADR-0193 Account Bond and ADR-0193 filing clock have different full paths; both are preserved and registered as a number collision. Next local ADR is 0195, RFC 0028. Historical evidence under `g14-completion-2026-10-20` remains source-branch evidence and is not summed into a joint-build pass count.

Account Bond implementation and real operating profit/heartbeat measurements are absent; capital results remain conditional simulation/reaggregation. The user devnet measurement job has not been declared complete or failed by this work.

Joint-build outcomes will be recorded separately. Existing imported reference defects are not baseline-suppressed. All cargo and release executable runs use the shared heavy-build lock. No upper limit or identity expectation is repinned.

## Joint native test integration finding

The first joint native run had 25 passes and one failure in the transplanted recovery test. It incorrectly assumed the old main fixture's owner interval must exceed original acceptance +10,000. X3's actual fixture derives acceptance 100, preparation end 4,493 and owner interval 4,496..4,506 before hard 10,100. This is synthetic fixture geometry, not measured latency, throughput or all-branch feasibility. The test is corrected to reserve and exercise journal recovery in the X3 owner interval, and separately rejects an interval beyond the unchanged original hard bound. The main-source historical negative result remains accurate for that older source. No production clock or limit was changed to correct the test.

## Remaining completion work

Joint component success does not discharge the original handover. Remaining work includes original-admission authentication and all legal response duties/turns/fragments; actual reserved supply and public outsider ingress under colluding owners; physical/network cost and capacity calibration; atomic deadline changes and complete dependency horizons; terminal reason-specific holds/releases and signed fresh adaptive relay availability; T01–T15 and R-D2-NODE campaign, tag 230 decision, and measured real-model door ≤3,120. Current service rows retain unresolved holds and have no adopted discharge rule. The minimum signed-custody guard covers original hard, not every follow-up/retention dependency. Shared request enumeration is not the complete interactive duty graph.

## Verification checkpoint

Final selected joint-build tests: **93 passed / 0 failed / 0 ignored**. See the separate [candidate evidence](../../../evidence/palw/g14-integration-candidate-2026-10-20/README.md) for exact commands, executable/log hashes, intermediate failures, synthetic scopes and unrun R-D2-NODE cases. This completes the first selective-port regression checkpoint; it does not complete G14, service admission/supply or the all-branch node campaign.

## Native response derivation checkpoint

Existing source ID chunks are distinguished from the owned route’s one all-ID logical request. Body/RAM bounds come from the native validator; declared-body branches yield Begin, canonical Parts and Finish. The native fixture helper now uses generated actions, tested through existing dispatcher/body authentication. Selected checks 57 passed, non-test consensus check succeeded; scope and unresolved full graph are recorded in [the response derivation](rfc26-native-response-graph-2026-10-20.md). Original query IDs and all protocol caps/clocks/tags are retained.

## Devnet source update received — 2026-10-20

The operator supplied `release/t12-16000` / `799a058fa` with the previously missing [report](../../../measurements/2026-10-19-devnet-payload-carrier.md) and raw 1,180-line log. They are imported byte-identically; earlier “no result file yet” statements describe the state before this update. The operator reports that the recording agent stopped before producing the summary and that the devnet has since been shut down and temporary keys/directories removed. This does not authorize a new deployment.

[Reaggregation audit and corrections](../../../measurements/2026-10-19-devnet-payload-carrier-audit.md): seven first-mempool observations pair with seven included records; intended load/saturation was not achieved. There are 35 block-observation rows / 33 distinct recorded block prefixes. Both maximum 111,927-byte rows include a measured tx, so a floor-only 111,927-byte maximum is not established; the largest whole-block observation with zero measured txs is 87,095 bytes. Sender timestamps, per-tx bytes/mass, selected-chain acceptance time and reserved court supply are absent. No observed or inferred rate is adopted. Full F02/F03 and real-model door/capacity work remains open.

## Native current-duty binding checkpoint

The service-bound acquisition default guard now verifies each unpaid actor against its actual retained original frame, Ready state, current native demand/turn and body cursor before the continuing service default denial. Actor/row duties remain visible as expired metadata without extending their signed clock or releasing claim holds. Selected checks **58 passed**, consensus non-test release check succeeded. [Derivation, limits and extra cost to calibrate](rfc26-native-response-duty-2026-10-20.md). Full packet-to-duty admission and actual supply/inclusion remain incomplete; this checkpoint does not establish F02/F03 or node T05/T08.
