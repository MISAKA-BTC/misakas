# RFC-0019: old and new gateway observations on both sides of H

The real-process R-8 driver now requires every configured old/new gateway to answer both before and after the annual switch. Previously `scenario_local_llm` filtered its pre-H observations to old class IDs, so a new gateway that was unavailable or refused inference before H could still pass after H. The RFC-0019 §11 R-8 requirement and the driver's own scenario description require both sides for both models.

The driver probes actual node DAA around registry snapshots and before/after each gateway observation. A pre-H label requires all upgraded roles to remain below H; a post-H observation requires all roles at or beyond H. Crossing H during a pre-H HTTP call, or rolling back below H during a post-H call, refuses the observation. Reports retain the actual per-role DAA samples for each answer. The driver's existing timeout/retry logic does not convert a late success into a pre-H success.

Both registry snapshots now include every upgraded role. The existing R-7 verdict checks each role's pinned old/new roots and tiers, append-only preservation and edition; the existing optional pinned catalog-root check also applies. Disagreement between roles refuses the scenario. The report is written only after every required inference and registry check succeeds. This changes no catalog selection, model tiers, signature authority, model review, public activation or consensus-credit rule.

## Verification

`python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc0019_*.py'` passed **11/11 in 0.018s**. Log: `/tmp/misaka-rfc19-local-llm-prepost-driver-tests-20261025.log`. `git diff --check` passed for the driver and test file.

The new orchestration controls execute the actual `scenario_local_llm` flow and existing gateway/registry verdict functions with explicitly scripted observations. They verify all four old/new-before/after calls; refusal of a missing, offline, empty or wrong-ID new pre-H response; per-role registry disagreement; wrong pinned roots, tiers, edition or catalog root; and actual phase checks when the chain crosses H during a pre-H response or rolls back during a post-H response. Existing artifact/dispatch/verdict controls also pass.

These tests launch no node, gateway or model. They establish driver behavior only. No real LLM R-8 campaign, annual upgrade interoperability, actual artifact execution, Gate C certification, claim acceptance or K3 result is claimed. Full real-process execution still requires the user's fixed old/new artifacts, binaries, gateway configuration and annual catalog plan; this change invents none of those inputs.
