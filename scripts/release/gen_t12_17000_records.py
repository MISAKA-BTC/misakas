#!/usr/bin/env python3
"""Generate `consensus/core/src/palw_t12_17000_records_v1.rs` — the testnet-12 DAA 17,000 arm-readiness records (lane ACT2).

User decision 2026-10-27: arm the 17,000 list with the drill skipped. A requirement has a slot filled by a passing run output (the
pin script's JSON form) OR by a typed waiver. No run output in that form exists for any requirement of this list (lanes ran tests and
E2E and reported them in prose; none kept a `passed: true` JSON tied to one source commit), so every slot below is a waiver. A waiver
says the requirement was NOT run as pinned evidence, or that it is a user decision. It never says "passed", and it lifts no other
refusal (ordering, values, measurement derivation, wiring, open blockers).

Run: python3 scripts/release/gen_t12_17000_records.py > consensus/core/src/palw_t12_17000_records_v1.rs
"""
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import pin_arming_record as P  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parents[2]
AT = 16_600  # the G14 flag day (moved from 17,000 by the user 2026-10-27)
SOURCE = 'd181e815ea6b662b84ae409f696e92eb9a5e057d'  # the integration tip the lanes' runs were reported against
D = 'user decision 2026-10-27'
NR = 'not run as pinned evidence; ' + D + ' (armed at DAA 17,000 without it)'
DRILL = 'drill not run; ' + D + ' (drill skipped)'
E2E = 'node E2E not run on testnet-12 as pinned evidence; ' + D + ' (E2E not executed)'
MEAS = 'not measured; ' + D + ' (unmeasured; the lane VAL estimate is what is pinned)'
REPORTED = 'run reported in {} but no run-output file in the pin form was kept; counted as not run; ' + D
REVIEW = ('no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) '
          'are documents, not this record\'s review; ' + D)


def genesis_hex():
    src = (ROOT / 'consensus/core/src/config/genesis.rs').read_text()
    block = src[src.index('pub const PALW_T12_GENESIS'):]
    body = block[block.index('Hash64::from_bytes(['):block.index('])')]
    hx = ''.join('%02x' % int(b, 16) for b in re.findall(r'0x([0-9a-fA-F]{2})', body))
    assert len(hx) == 128, len(hx)
    return hx


V17 = [  # PALW_G14_VALIDATION_STEPS_V1
    NR + ' (V1 build / regressions / boundary decode)',
    E2E + ' (V2 one correct seat)',
    E2E + ' (V3 full collusion, one outside bond)',
    NR + ' (V4 every violation family and honest twin)',
    MEAS + ' (V5 maximum profile and delay under contention)',
    E2E + ' (V6 real network and economic accounting)',
    E2E + ' (V7 restart / IBD / pruned / reorg)',
]

# (family, enum variant, Params field, [(kind, reason)] per requirement in order)
TABLE = [
    ('g14', 'LegacyPublicFilerV1', 'palw_legacy_public_filer_v1', [
        ('not-run', REPORTED.format('arm-legacy-report.md (g14_close_legacy 9/9)') + ' (V1)'),
        ('not-run', REPORTED.format('arm-legacy-report.md (node E2E (a), outsider conviction)') + ' (V2/V3)'),
        ('not-run', REPORTED.format('arm-legacy-report.md (node E2E (a)-(d))') + ' (V4)'),
        ('not-run', MEAS + ' (V5 hard-deadline envelope and Valid-lock capacity)'),
        ('not-run', E2E + ' (V6 shipped binary)'),
        ('not-run', REPORTED.format('arm-legacy-report.md (g14_arm_legacy_d restart/reorg/IBD/pruned)') + ' (V7)'),
        ('not-run', DRILL + ' (D-02 / D-13 / D-16)'),
    ]),
    ('g14', 'LegacyHeldDaV2', 'palw_legacy_held_da_v2', [
        ('not-run', NR + ' (V1 tags 157-159 boundary)'),
        ('not-run', REPORTED.format('arm-legacy-report.md (node E2E (b) held DA default)') + ' (V2/V3)'),
        ('not-run', NR + ' (V4 lg14b suite)'),
        ('not-run', MEAS + ' (V5 descent rounds, 34-session budget)'),
        ('not-run', E2E + ' (V6 shipped binary)'),
        ('not-run', REPORTED.format('arm-legacy-report.md (g14_arm_legacy_d)') + ' (V7)'),
        ('not-run', DRILL + ' (D-02 / D-11)'),
    ]),
    ('g14', 'StagedPublicCourtV1', 'palw_staged_public_court_v1', [('not-run', r) for r in V17]),
    ('g14', 'StagedTirRouteV1', 'palw_staged_tir_route_v1', [('not-run', r) for r in V17]),
    ('g14', 'TirPublicConvictionV1', 'palw_tir_public_conviction_v1', [('not-run', r) for r in V17]),
    ('g14', 'CourtCapacityV1', 'palw_court_capacity_v1', [
        ('not-run', REPORTED.format('arm-legacy-report.md (g14_court_capacity 15/15)') + ' (V1)'),
        # Slot 1 (measured values): a UserDecision here lets ONLY the exact approved estimate policy stand without a measurement.
        ('user-decision', 'V11 court capacity uses the user-approved estimate PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1 (cap 42, '
                          'session weight 13, rho 3,125; user approval 2026-10-26 of the V1-V5/V11 estimates as values). Not a '
                          'measurement: only the one-move class was measured, under load ~11 (M-CAP arm-legacy-2026-10-27); the match '
                          'against all-op-class measurements and the no-load re-measurement are waived; ' + D),
        ('not-run', MEAS + ' (rho_court; the V11 estimate 3,125 is pinned)'),
        ('not-run', DRILL + ' (V3 saturating coalition)'),
        ('not-run', DRILL + ' (V3 refilling coalition)'),
        ('not-run', DRILL + ' (V5 maximum load)'),
        ('not-run', E2E + ' (V7 restart / IBD / pruned / reorg mid-queue)'),
    ]),
    ('g14', 'ClaimHardDeadlineV1', 'palw_claim_hard_deadline_v1', [
        ('not-run', NR + ' (V1 X3 suite)'),
        ('not-run', MEAS + ' (B_* terms and c_retain)'),
        ('not-run', E2E),
        ('not-run', DRILL),
    ]),
    ('g14', 'G14RouteRequiredV1', 'palw_g14_route_required_v1', [
        ('not-run', REPORTED.format('family-fptok-report.md (g14_close_gates 12/12)') + ' (T-N8)'),
        ('not-run', REPORTED.format('family-fptok-report.md (g14_close_gates 12/12)') + ' (T-N10)'),
        ('not-run', E2E + ' (TNODE run blocked by the slice ruleset)'),
        ('not-run', DRILL),
    ]),
    ('kernel-route', 'ProbabilisticConstraintsV1', 'palw_probabilistic_constraints_v1', [
        ('not-run', NR + ' (V1 kernel route fold suite)'),
        ('not-run', E2E + ' (g14_kernel_route_e2e; TNODE blocked)'),
        ('not-run', NR + ' (adversarial suite)'),
        ('not-run', REVIEW + ' (soundness / beacon review)'),
        ('not-run', 'shadow comparison not measured; user decision 2026-10-26 treated it as complete TEMPORARILY; ' + D),
        ('not-run', MEAS + ' (9B-8k / 2M reports)'),
        ('not-run', MEAS + ' (court resource bounds)'),
        ('not-run', DRILL),
    ]),
    ('kernel-route', 'ProviderCourtV1', 'palw_provider_court_v1', [
        ('not-run', NR + ' (V1 provider court suite)'),
        ('not-run', E2E),
        ('not-run', MEAS + ' (lease reservation, bond and fee)'),
        ('not-run', DRILL),
    ]),
    ('rfc0018', 'IndependentlyWorkCertifiedV1', 'palw_independently_work_certified_v1', [
        ('user-decision', 'U-1: Gate C activates at testnet-12 DAA 17,000 with the G14 list (' + D + ')'),
        ('not-run', NR + ' (T-0 suite)'),
        ('user-decision', 'U-5: certification m/k = 75/41 and the drawn verifiers bear the re-executions (user approval 2026-10-26)'),
        ('not-run', 'U-6 veto: not separately ruled; armed without that ruling by ' + D),
        ('not-run', 'U-2/U-3: not separately ruled; armed without that ruling by ' + D),
        ('not-run', DRILL + ' (Gate C crossing, restart/IBD/reorg)'),
    ]),
]


def main():
    g = genesis_hex()
    waivers, records = [], {'g14': [], 'kernel-route': [], 'rfc0018': []}
    for family, variant, field, slots in TABLE:
        rec = P.build_record_with_waivers(family, variant, field, g, AT, SOURCE, len(slots), [],
                                          [(i, k, r) for i, (k, r) in enumerate(slots)], review_waiver=REVIEW)
        waivers += rec['waivers']
        records[family].append(rec)
    # FpSelection's frame cost record: the lane VAL estimate's cost, its measurement and review slots waived.
    fc = [P.make_waiver('palw_tir_frame_cost_v1', g, AT, 0, 'not-run',
                        'frame cost not measured (slowest node, release build, genesis-size classes); the lane VAL estimate '
                        'PALW_G14_TIR_FRAME_COST_ESTIMATE_V1 is pinned; ' + D, 'user 2026-10-27'),
          P.make_waiver('palw_tir_frame_cost_v1', g, AT, P.WAIVER_REVIEW, 'not-run', REVIEW, 'user 2026-10-27')]
    waivers += fc
    # Model tier (user decision 2026-10-27): c 300, κ 1000, catalog edition 0 empty. Its own record shape (names c, κ, catalog tip).
    tier_slots = [
        ('user-decision', 'c = 300 permille (frozen premise, PRINCIPLES 9) and kappa = 1000 permille from DAA 17,000 (user decision '
                          '2026-10-27); genesis-registered classes are 1.0 as a chain fact, catalog edition 0 is empty'),
        ('not-run', 'Gate C non-shared-model test in the legacy/FP families not run; ' + D),
        ('not-run', NR + ' (RFC-0019 section 11 unit tests A-E)'),
        ('not-run', DRILL + ' (RFC-0019 R-1..R-3, R-5, R-6 with the shipped binary)'),
        ('user-decision', 'R-4: no edition lists a model at 1.0 after the fence (catalog edition 0 only); ' + D),
        ('user-decision', 'R-7 / R-8: no edition adds a local LLM after the fence (catalog edition 0 only); ' + D),
        ('not-run', 'no annual catalog edition release manifest (edition 0 only, empty); ' + D),
        ('not-run', 'RFC-0019 readiness M1-M5 not reported as a run; ' + D),
    ]
    tier_w = [P.make_waiver('palw_model_tier_v1', g, AT, i, k, r, 'user 2026-10-27') for i, (k, r) in enumerate(tier_slots)]
    tier_w.append(P.make_waiver('palw_model_tier_v1', g, AT, P.WAIVER_REVIEW, 'not-run', REVIEW, 'user 2026-10-27'))
    waivers += tier_w
    ge = 'crate::config::genesis::PALW_T12_GENESIS.hash'
    out = ['//! **GENERATED** by `scripts/release/gen_t12_17000_records.py` (lane ACT2) — do not edit by hand.\n',
           '//! testnet-12 DAA 17,000 arm-readiness records. EVERY slot is a typed waiver (`crate::palw_arming_waiver_v1`): no\n',
           '//! requirement here was run as pinned evidence. User decision 2026-10-27. A waiver lifts no ordering, value, measurement,\n',
           '//! wiring or open-blocker refusal.\n',
           'use crate::palw_arming_waiver_v1::{PALW_ARMING_WAIVER_REVIEW_V1, PalwArmingWaiverKindV1, PalwArmingWaiverV1};\n\n',
           '/// Every waiver the records below carry.\n',
           'pub const PALW_T12_17000_WAIVERS_V1: &[PalwArmingWaiverV1] = &[\n']
    out += [P.render_waiver_rust(w, ge) for w in waivers]
    out.append('];\n\n')
    names = {'g14': ('crate::palw_g14_gates_v1', 'PalwG14ArmingRecordV1', 'PalwG14GatedFenceV1', 'PALW_G14_ARMING_RECORD_VERSION_V1',
                     'PALW_T12_17000_G14_RECORDS_V1'),
             'kernel-route': ('crate::palw_kernel_route_arming_v1', 'PalwKernelRouteArmingRecordV1', 'PalwKernelRouteGatedFenceV1',
                              'PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1', 'PALW_T12_17000_KERNEL_ROUTE_RECORDS_V1'),
             'rfc0018': ('crate::palw_rfc0018_v1', 'PalwRfc0018ArmingRecordV1', 'PalwRfc0018GatedFenceV1',
                         'PALW_RFC0018_ARMING_RECORD_VERSION_V1', 'PALW_T12_17000_RFC0018_RECORDS_V1')}
    for family, recs in records.items():
        mod, struct, fty, ver, const = names[family]
        out.append('/// The %s records.\npub const %s: &[%s::%s] = &[\n' % (family, const, mod, struct))
        for rec in recs:
            ev = ''.join('            %s, // slot %s\n' % (P._bytes(d), lab) for lab, d in rec['evidence'])
            out.append('    %s::%s {\n        version: %s::%s,\n        fence: %s::%s::%s,\n        genesis: %s,\n'
                       '        activation_daa: %d,\n        source_commit: %s,\n        evidence: &[\n%s        ],\n'
                       '        review: %s,\n    },\n' % (mod, struct, mod, ver, mod, fty, rec['fence'], ge, AT,
                                                         P._bytes(rec['sourceCommit']), ev, P._bytes(rec['review'])))
        out.append('];\n\n')
    out.append('/// The frame cost record for FpSelection: the lane VAL estimate, measurement and review waived.\n')
    out.append('pub const PALW_T12_17000_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] = &[\n'
               '    crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1 {\n'
               '        version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORD_VERSION_V1,\n'
               '        genesis: %s,\n        activation_daa: %d,\n        source_commit: %s,\n        measurement: %s,\n'
               '        review: %s,\n        cost: crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1,\n    },\n];\n'
               % (ge, AT, P._bytes(SOURCE), P._bytes(fc[0]['digest']), P._bytes(fc[1]['digest'])))
    tip = empty_genesis_catalog_root()
    ev = ''.join('            %s, // slot %d\n' % (P._bytes(w['digest']), w['slot']) for w in tier_w[:-1])
    out.append('\n/// The model-tier record: c 300, κ 1000 (user decision 2026-10-27), catalog edition 0 (empty) as the tip.\n'
               'pub const PALW_T12_17000_MODEL_TIER_RECORDS_V1: &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1] = &[\n'
               '    crate::palw_model_tier_v1::PalwModelTierArmingRecordV1 {\n'
               '        version: crate::palw_model_tier_v1::PALW_MODEL_TIER_ARMING_RECORD_VERSION_V1,\n'
               '        genesis: %s,\n        activation_daa: %d,\n        registered_permille: 300,\n'
               '        registered_cap_permille: 1000,\n        catalog_tip_edition: 0,\n'
               '        catalog_tip_root: crate::Hash64::from_bytes(%s),\n        source_commit: %s,\n        evidence: &[\n%s        ],\n'
               '        review: %s,\n    },\n];\n' % (ge, AT, P._bytes(tip), P._bytes(SOURCE), ev, P._bytes(tier_w[-1]['digest'])))
    sys.stdout.write(''.join(out))


def empty_genesis_catalog_root():
    """`AnnualModelCatalogV1::genesis(vec![]).root()` — blake2b-512 keyed `misaka-palw/annual-model-catalog/v1` over
    catalog_bytes_v1: u16 version 1, u32 edition 0, u64 activation 0, 64 zero bytes of previous root, u32 model count 0."""
    import hashlib
    body = (1).to_bytes(2, 'little') + (0).to_bytes(4, 'little') + (0).to_bytes(8, 'little') + bytes(64) + (0).to_bytes(4, 'little')
    return hashlib.blake2b(body, digest_size=64, key=b'misaka-palw/annual-model-catalog/v1').hexdigest()


if __name__ == '__main__':
    main()
