# RFC-0012 campaign evidence identity guard (2026-10-26)

The 15-gate campaign remains **unexecuted**. No gate is promoted from UNKNOWN to PASS by this change.

Before this guard, `gates_report` collected observed facts with a dictionary comprehension. If the same fact ID appeared twice, a later PASS replaced an earlier FAIL when the gate was assembled. The campaign plan requires unique fact IDs, but the runtime aggregate did not enforce uniqueness or recheck the observed fact's scenario, kind, role and claim against the frozen plan. The report also rechecked `campaign-freeze.json` but did not recheck the durable `campaign-once.json` marker before writing its final result.

Now any duplicate, unplanned, reserved or reassigned observed fact, duplicate or mismatched scenario, or absent planned scenario fails every gate with an explicit `integrityErrors` entry. A fact ID cannot take the reserved `base-drill-mechanics` identity in the plan. Final aggregation requires both durable markers to retain their original exact bytes, and refuses a symlinked campaign root. These checks preserve failed and incomplete observations instead of allowing a later row or marker replacement to claim a different campaign.

Validation: `python3 -m unittest scripts.drills.tests.test_rfc_integration_campaign` → **18 passed**. The controls include a complete synthetic plan that still aggregates to PASS, FAIL→PASS duplicate ID and altered role that aggregate to FAIL, a malformed fact identity, and mutations of either durable marker that are rejected. These are unit controls over the reporting logic; they do not execute multiple processes, 100k work, pruned IBD, OS crash, EVM Latest/Safe/Finalized, bridge/market fill or historical signatures.
