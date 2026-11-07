# Native funded candidate: actual policy → real native Source

Candidate `native-source-policy.patch` applies to current tree (apply-check passed). Repository untouched; no build/test. Three files: core existing candidate function, actual virtual-processor call site, existing Full UTXO candidate fixture. No new wrapper/DTO, Complete/Admitted authority, price, wire, preset or production diagnostics enablement.

The existing diagnostic now receives direct references to actual processor state params and the actual court resolved at accepting DAA plus its native leaf ladder. The processor calls its existing `palw_court_params_at` (bundle plus kary/held fences) and `palw_court_step_ladder_at`; it does not resolve any body from carrier profile IDs. Invalid native configuration/unavailable bodies remain Missing. Function documentation makes this trusted call-site boundary explicit; public core helper references themselves are not transferable policy credentials.

Within the same current state used by existing diagnostic checks:

- fetch actual class state and require registered_daa<=accepting DAA and artifact root equals literal original header;
- require actual acceptance DAA<=producer's signed SourcePledge acceptance expiry (without inventing any longer material or service obligation);
- derive demand from actual configured state fence mirror at the class's REGISTERED DAA, never accepting DAA;
- apply same rooted class-specific ladder uplift as existing original_plan_v1;
- call real `Source::new_native_v1` using actual registered program/layout/header, literal original execution/trace roots and native ladder; its actual constructor authenticates class/layout/job/canonical counts/roots under the native resource ticket;
- compare actual Source ID to producer-signed pledge Source ID, reducing previously unchecked data scope;
- retain Missing explicitly: numerical/semantic enumeration calibration, service/financial profile bodies and ordered atomic coordinator remain absent. Registered demand is resolved and reported, NOT falsely said to have executed numerical enumeration. Actual court body is resolved; only its legal arity and leaf-ladder part are consumed by this Source-only unit.

Native Source Resource errors remain `PalwFundedOriginalCandidateV1::Resource(NativeCoverageV1)`; no String classification or Invalid/conviction conversion. The enum loses Copy because coverage carries typed owned detail. Ordinary UTXO acceptance/output handling and funded extraction refusal remain unchanged. Parent should check all enum users for exhaustive handling; repo search currently finds only debug logging and diagnostic tests, no state writer.

## Actual fixture coverage proposal

The previous test used MAINNET_PARAMS with PALW Disabled, which cannot exercise native rules. Candidate switches only this test's processor to the existing `palw_v2_test_bundle_funded_for` helper and same test cadence setup used elsewhere. The standalone source state stays the core-generated, production-prepared fixture; this does not claim its history was mined into that processor's genesis chain.

Tests still use actual valid payer-signed transactions and Full native UTXO validation. Baseline now requires Missing beginning `native source checked`, proving it passed the new constructor rather than taking policy-unavailable early return. Additional mutations:

- correctly re-sign TEST producer pledge with wrong Source ID and re-sign matching payer consent: exact actual Source mismatch;
- validly signed expired SourcePledge acceptance clock: exact clock refusal;
- actual current class metadata future registration or foreign artifact: exact native class refusal;
- actual current registered layout changed, still structurally legal: actual native Source geometry refusal;
- existing TEST config with native TIR fence and second fence at1: identical current acceptance, registered DAA0 resolves Release2000 while registered DAA1 resolves H7. Still Missing; no numerical verdict or full closure claim;
- existing TEST court body replaced with valid two-leaf court: actual native ladder causes Source refusal, not a producer profile ID; asserts fixture has no held uplift instead of changing that legitimate policy;
- existing invalid payer/FP/pledge signatures, input/fee checks and ordinary output availability tests retained. No default diagnostics flag is set.

Native Resource branch structure is explicit; actual isolated Source quota tests separately exercise that constructor's refusal/lease behavior. This new Full UTXO test does not claim a global-resource-mode refusal test or mined funded escrow/undo success. Constructor output/geometry ID equality remains covered by existing actual Source tests.
