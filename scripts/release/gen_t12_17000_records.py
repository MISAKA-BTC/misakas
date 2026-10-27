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
AT = 17_400  # the G14 flag day (user: 17,000 -> 16,600 -> 17,400, 2026-10-27)
SOURCE = 'd181e815ea6b662b84ae409f696e92eb9a5e057d'  # the integration tip the lanes' runs were reported against
def genesis_hex():
    src = (ROOT / 'consensus/core/src/config/genesis.rs').read_text()
    block = src[src.index('pub const PALW_T12_GENESIS'):]
    body = block[block.index('Hash64::from_bytes(['):block.index('])')]
    hx = ''.join('%02x' % int(b, 16) for b in re.findall(r'0x([0-9a-fA-F]{2})', body))
    assert len(hx) == 128, len(hx)
    return hx


D = 'user decision 2026-10-27'
DRILL = ('drill-skipped', '', 'the user decided to skip the drill; ' + D)
E2E = ('e2e-not-run', '', 'the user decided to arm without this node E2E; ' + D)
REVIEW_REPORT = ('docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, '
                 'independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)')
REVIEW = ('agent-review', REVIEW_REPORT, 'independent review by a Claude agent (user-approved 2026-10-26)')
# A leftover slot: no permitted waiver kind fills it (F1). The record is NOT emitted while any slot is leftover.
LEFT = None

V17 = [  # PALW_G14_VALIDATION_STEPS_V1: V1 build/regressions, V2, V3, V4, V5, V6, V7 — none is a drill/E2E/measured/review slot
    LEFT, LEFT, LEFT, LEFT, LEFT, LEFT, LEFT,
]

X3_VALUE = ('PALW_X3_DECIDED_DELTA_TERMS_V1 (Delta 2,016 DAA) and c_retain 36,058 sompi/DAA '
            '(PALW_X3_RETENTION_COST_PER_DAA_SOMPI_V1)')
X3_BASIS = 'lane P3 V10 decided 2026-10-22; c_retain = ceil(2,250,000 B x 50 sompi/B / 3,120 DAA); estimates, user 2026-10-22'
V11_VALUE = ('PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1: cap 42, session weight 13, op_work [5,13,1,11], rho_court 3,125 '
             'sompi/DAA/unit, block budget work 13, one-move budget work 5 / 34,000 B')
V11_BASIS = ('user approval 2026-10-26 of the V11 estimate as the value; only the one-move class was measured (M-CAP '
             'arm-legacy-2026-10-27, p95 141.7 us under load ~11); the all-class match and the no-load re-measurement are waived')

# (family, enum variant, Params field, per-slot spec: a (kind, value, reason) tuple or LEFT)
TABLE = [
    ('g14', 'LegacyPublicFilerV1', 'palw_legacy_public_filer_v1',
     [LEFT, E2E, E2E, LEFT, LEFT, LEFT, DRILL]),  # V1 suite; V2/V3 E2E; V4 lg14a_* node E2E; V5 measured (no estimate); V6; V7
    ('g14', 'LegacyHeldDaV2', 'palw_legacy_held_da_v2', [LEFT, E2E, LEFT, LEFT, LEFT, LEFT, DRILL]),
    ('g14', 'StagedPublicCourtV1', 'palw_staged_public_court_v1', V17),
    ('g14', 'StagedTirRouteV1', 'palw_staged_tir_route_v1', V17),
    ('g14', 'TirPublicConvictionV1', 'palw_tir_public_conviction_v1', V17),
    ('g14', 'CourtCapacityV1', 'palw_court_capacity_v1',
     [LEFT, ('provisional-estimate', V11_VALUE, V11_BASIS), ('provisional-estimate', 'rho_court 3,125 sompi/DAA/unit, session weight 13',
                                                             'lane VAL V11 estimate approved by the user 2026-10-26'),
      DRILL, DRILL, DRILL, LEFT]),
    ('g14', 'ClaimHardDeadlineV1', 'palw_claim_hard_deadline_v1', [LEFT, ('provisional-estimate', X3_VALUE, X3_BASIS), E2E, DRILL]),
    ('g14', 'G14RouteRequiredV1', 'palw_g14_route_required_v1', [LEFT, LEFT, E2E, DRILL]),
    ('kernel-route', 'ProbabilisticConstraintsV1', 'palw_probabilistic_constraints_v1',
     [LEFT, E2E, LEFT, REVIEW, LEFT, LEFT, LEFT, DRILL]),  # #4 shadow, #5 9B-8k/2M, #6 court bounds: no estimate value
    ('kernel-route', 'ProviderCourtV1', 'palw_provider_court_v1', [LEFT, E2E, LEFT, DRILL]),
    ('rfc0018', 'IndependentlyWorkCertifiedV1', 'palw_independently_work_certified_v1',
     [('user-decision', 'Gate C activates at testnet-12 DAA 17,400 with the G14 list', D),
      LEFT,
      ('user-decision', 'certification m/k = 75/41; the drawn verifiers bear the re-executions', 'user approval 2026-10-26'),
      LEFT, LEFT, DRILL]),  # U-6 and U-2/U-3 were not ruled
]
TIER = [
    ('user-decision', 'c = 300 permille, kappa = 1000 permille from DAA 17,400; catalog edition 0 empty',
     'user decision 2026-10-27; c is the frozen premise (PRINCIPLES 9)'),
    LEFT, LEFT, LEFT, LEFT, LEFT, LEFT, LEFT,
]
FRAME_VALUE = ('PALW_G14_TIR_FRAME_COST_ESTIMATE_V1 (inclusion margin 4 DAA, rents 1, acquisition 1 DAA, collection 8 DAA, reorg '
               'retention 360 DAA)')
FRAME_BASIS = 'lane VAL estimate, user approval 2026-10-26 of the estimates as values; not measured'


def main():
    g = genesis_hex()
    waivers, leftovers, records = [], [], {'g14': [], 'kernel-route': [], 'rfc0018': []}
    for family, variant, field, slots in TABLE:
        ws = [(i, s[0], s[1], s[2]) for i, s in enumerate(slots) if s is not LEFT]
        left = [i for i, s in enumerate(slots) if s is LEFT]
        for i, kind, value, reason in ws + [(P.WAIVER_REVIEW,) + REVIEW]:
            waivers.append(P.make_waiver(field, g, AT, i, kind, value, reason, 'user 2026-10-27'))
        if left:
            leftovers += ['%s %s slot %d' % (family, field, i) for i in left]
            continue
        rec = P.build_record_with_waivers(family, variant, field, g, AT, SOURCE, len(slots), [], ws, review_waiver=REVIEW)
        records[family].append(rec)
    tier_left = [i for i, s in enumerate(TIER) if s is LEFT]
    for i, s in enumerate(TIER):
        if s is not LEFT:
            waivers.append(P.make_waiver('palw_model_tier_v1', g, AT, i, s[0], s[1], s[2], 'user 2026-10-27'))
    waivers.append(P.make_waiver('palw_model_tier_v1', g, AT, P.WAIVER_REVIEW, *REVIEW, 'user 2026-10-27'))
    leftovers += ['model-tier palw_model_tier_v1 slot %d' % i for i in tier_left]
    fc = [P.make_waiver('palw_tir_frame_cost_v1', g, AT, 0, 'provisional-estimate', FRAME_VALUE, FRAME_BASIS, 'user 2026-10-27'),
          P.make_waiver('palw_tir_frame_cost_v1', g, AT, P.WAIVER_REVIEW, *REVIEW, 'user 2026-10-27')]
    waivers += fc
    ge = 'crate::config::genesis::PALW_T12_GENESIS.hash'
    out = ['//! **GENERATED** by `scripts/release/gen_t12_17000_records.py` (lane ACT2) — do not edit by hand.\n',
           '//! testnet-12 G14 flag-day arm-readiness records and waivers (F1 kinds only: drill skipped, E2E not run, provisional\n',
           '//! estimate with its value, Claude-agent review, user decision). A record is emitted only when EVERY slot is a permitted\n',
           '//! waiver or a run; leftover slots (no permitted kind) are listed below and keep their fence refused. A waiver lifts no\n',
           '//! ordering, value, measurement, wiring or open-blocker refusal.\n//!\n//! Leftover slots:\n']
    out += ['//! - %s\n' % l for l in leftovers]
    out += ['use crate::palw_arming_waiver_v1::{PALW_ARMING_WAIVER_REVIEW_V1, PalwArmingWaiverKindV1, PalwArmingWaiverV1};\n\n',
            '/// Every pinned waiver.\n',
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
        out.append('/// The %s records (complete ones only).\npub const %s: &[%s::%s] = &[\n' % (family, const, mod, struct))
        for rec in recs:
            ev = ''.join('            %s, // slot %s\n' % (P._bytes(d), lab) for lab, d in rec['evidence'])
            out.append('    %s::%s {\n        version: %s::%s,\n        fence: %s::%s::%s,\n        genesis: %s,\n'
                       '        activation_daa: %d,\n        source_commit: %s,\n        evidence: &[\n%s        ],\n'
                       '        review: %s,\n    },\n' % (mod, struct, mod, ver, mod, fty, rec['fence'], ge, AT,
                                                         P._bytes(rec['sourceCommit']), ev, P._bytes(rec['review'])))
        out.append('];\n\n')
    out.append('/// The frame cost record for FpSelection: the lane VAL estimate (ProvisionalEstimate) and the agent review.\n')
    out.append('pub const PALW_T12_17000_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] = &[\n'
               '    crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1 {\n'
               '        version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORD_VERSION_V1,\n'
               '        genesis: %s,\n        activation_daa: %d,\n        source_commit: %s,\n        measurement: %s,\n'
               '        review: %s,\n        cost: crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1,\n    },\n];\n'
               % (ge, AT, P._bytes(SOURCE), P._bytes(fc[0]['digest']), P._bytes(fc[1]['digest'])))
    out.append('\n/// The model-tier record: not emitted while slots %s are leftover.\n' % tier_left)
    out.append('pub const PALW_T12_17000_MODEL_TIER_RECORDS_V1: &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1] = &[];\n')
    sys.stdout.write(''.join(out))
    sys.stderr.write('LEFTOVER %d\n' % len(leftovers) + ''.join('  %s\n' % l for l in leftovers))


def empty_genesis_catalog_root():
    """`AnnualModelCatalogV1::genesis(vec![]).root()` — blake2b-512 keyed `misaka-palw/annual-model-catalog/v1` over
    catalog_bytes_v1: u16 version 1, u32 edition 0, u64 activation 0, 64 zero bytes of previous root, u32 model count 0."""
    import hashlib
    body = (1).to_bytes(2, 'little') + (0).to_bytes(4, 'little') + (0).to_bytes(8, 'little') + bytes(64) + (0).to_bytes(4, 'little')
    return hashlib.blake2b(body, digest_size=64, key=b'misaka-palw/annual-model-catalog/v1').hexdigest()


if __name__ == '__main__':
    main()
