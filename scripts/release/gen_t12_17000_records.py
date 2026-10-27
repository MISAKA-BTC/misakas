#!/usr/bin/env python3
"""Generate `consensus/core/src/palw_t12_17000_records_v1.rs` — the testnet-12 DAA 17,000 arm-readiness records (lane ACT2).

User decision 2026-10-27: arm the 17,000 list with the drill skipped. A requirement has a slot filled by a passing run output (the
pin script's JSON form) OR by a typed waiver. No run output in that form exists for any requirement of this list (lanes ran tests and
E2E and reported them in prose; none kept a `passed: true` JSON tied to one source commit), so every slot below is a waiver. A waiver
says the requirement was NOT run as pinned evidence, or that it is a user decision. It never says "passed", and it lifts no other
refusal (ordering, values, measurement derivation, wiring, open blockers).

Run: python3 scripts/release/gen_t12_17000_records.py > consensus/core/src/palw_t12_17000_records_v1.rs
"""
import json
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import pin_arming_record as P  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parents[2]
AT = 17_400  # the G14 flag day (user: 17,000 -> 16,600 -> 17,400, 2026-10-27)
SOURCE = 'fbd1998dd'  # resolved below: the commit run_arming_evidence.py ran (every run JSON names it)
RUNS_DIR = ROOT / 'scripts/release/arming-runs-17400'
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
# Leftover classes (ACT2 table, 2026-10-27): X = no run / estimate / review closes it before the fence; U = needs a user ruling;
# A = needs a NEW agent review (none exists). A leftover keeps its record unpinned.
X, U, A = 'X', 'U', 'A'
LEFT = X  # kept for readability


def RUN(rid):
    """(R) a slot closed by a real run: the pin-form JSON run_arming_evidence.py wrote."""
    return ('run', rid)


# X-d (Lead decision 2026-10-27, pinned as slot-class overrides in palw_arming_waiver_v1): real-network / shipped-binary / restart
# runs, under the user's 10-26 drill skip. Never recorded as run.
XD = ('e2e-not-run', '', 'Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run')
STAGED_V5 = ('provisional-estimate',
             'maximum legal delay = the X3 hard deadline (Delta 2,016 DAA); capacity under contention = V11 cap 42 sessions with the '
             '42-DAA turn deadline',
             'estimate from the pinned X3 decided terms and the user-approved V11 estimate (user 2026-10-22 / 2026-10-26); not measured')


def V17(v1_run):  # PALW_G14_VALIDATION_STEPS_V1: V1 build/regressions (run); V2-V4, V6, V7 X-d; V5 X-m
    return [RUN(v1_run) if v1_run else X, XD, XD, XD, STAGED_V5, XD, XD]

FILER_V5 = ('provisional-estimate',
            'hard-deadline envelope = the X3 decided policy (Delta 2,016 DAA, inside trace retention minus the disclosure window); '
            'Panel capacity under the Valid lock = the lane VAL Panel=1 schedule (2 packets per 833-DAA interval)',
            'estimate from the pinned X3 terms and the lane VAL V1 schedule (user 2026-10-22 estimates); not measured')
HELD_V5 = ('provisional-estimate',
           'reservation budget 34 sessions (design constant), court session cap 42 (V11), answer bytes within the 34,000 B one-move '
           'budget; descent rounds bounded by the same budget',
           'estimate from design constants and the user-approved V11 estimate (2026-10-26); not measured')
KERNEL_9B = ('provisional-estimate',
             '9B-8k: producer exec ~440 s / ~56 GB, verifier ~41 GB, court ~710 s / ~19 GB (x4.5 of the Qwen3.5-2B measurement); '
             '2M: not estimated - in this release only the wide128 class is attested, so no 9B class can be registered',
             'provisional value derived from KCONV measurements on OTHER models (claude/kconv 0d98e8e6c, kconv-report.md): Dolphin 8k '
             'exec 115 s / 6.47 GB, verifier 4.24 GB, court 132 s / 2.22 GB; Qwen3.5-2B exec 97.6 s / 12.4 GB, verifier 9.19 GB, '
             'court 158 s / 4.22 GB; scaled linearly to parameter count')
KERNEL_BOUNDS = ('provisional-estimate',
                 'a late single fault is localized within the V11 court budgets (cap 42, 34,000 B one-move) and the X3 hard deadline '
                 '(Delta 2,016 DAA); concurrent disputes bounded by cap 42',
                 'estimate from the user-approved V11 estimate and the pinned X3 terms; not measured')
V_REVIEW = ('agent-review',
            'lane V-REVIEW (read-only Claude agent) at fbd1998dd6e495562cbcf79500872e5bd334d7f2: PASS - the preset is '
            'genesis_only(Vec::new()) (full_activation_v1.rs:464-466), catalog-tip refusal palw_model_tier_v1.rs:465-470, KCAT '
            '1792ea6aa not in the release',
            'review: edition 0 only, none adds a model at 1.0 / new local LLM after the fence (independent review, user-approved reviewer)')
PROVIDER_VALUE = ('lease reservation >= 100 MSK, challenge bond 10 MSK, challenge fee 1 MSK, response window 20 DAA, >= 2 '
                  'providers, <= 8 open challenges (palw_provider_court_v1 interim constants)')
PROVIDER_BASIS = 'the interim constants in palw_provider_court_v1, pinned as estimates (user 2026-10-22); not measured'
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
     [RUN('core-g14_close_legacy'), E2E, E2E, FILER_V5, XD, RUN('consensus-lg14a_restart'), DRILL]),
    ('g14', 'LegacyHeldDaV2', 'palw_legacy_held_da_v2',
     [RUN('core-g14_close_legacy'), E2E, RUN('kaspad-lg14b_suite'), HELD_V5, XD, RUN('kaspad-lg14b_restart'), DRILL]),
    ('g14', 'StagedPublicCourtV1', 'palw_staged_public_court_v1', V17('core-lib-staged_court')),
    ('g14', 'StagedTirRouteV1', 'palw_staged_tir_route_v1', V17('core-lib-tir_public_fold')),  # split run (lane T)
    ('g14', 'TirPublicConvictionV1', 'palw_tir_public_conviction_v1', V17('core-lib-tir_public_claim_state')),
    ('g14', 'CourtCapacityV1', 'palw_court_capacity_v1',
     [RUN('core-g14_court_capacity'), ('provisional-estimate', V11_VALUE, V11_BASIS), ('provisional-estimate', 'rho_court 3,125 sompi/DAA/unit, session weight 13',
                                                             'lane VAL V11 estimate approved by the user 2026-10-26'),
      DRILL, DRILL, DRILL, RUN('core-court_capacity_v7')]),
    ('g14', 'ClaimHardDeadlineV1', 'palw_claim_hard_deadline_v1', [RUN('core-lib-ruling_fences'), ('provisional-estimate', X3_VALUE, X3_BASIS), E2E, DRILL]),
    ('g14', 'G14RouteRequiredV1', 'palw_g14_route_required_v1', [RUN('core-g14_close_gates-T-N8'), RUN('core-g14_close_gates-T-N10'), E2E, DRILL]),
    ('kernel-route', 'ProbabilisticConstraintsV1', 'palw_probabilistic_constraints_v1',
     [RUN('core-lib-kernel_route_onboarding_fold'), E2E, RUN('kernel-adversarial'), REVIEW, XD, KERNEL_9B, KERNEL_BOUNDS, DRILL]),
    ('kernel-route', 'ProviderCourtV1', 'palw_provider_court_v1', [RUN('core-lib-provider_court'), E2E, ('provisional-estimate', PROVIDER_VALUE, PROVIDER_BASIS), DRILL]),
    ('rfc0018', 'IndependentlyWorkCertifiedV1', 'palw_independently_work_certified_v1',
     [('user-decision', 'Gate C activates at testnet-12 DAA 17,400 with the G14 list', D),
      RUN('core-lib-rfc0018'),
      ('user-decision', 'certification m/k = 75/41; the drawn verifiers bear the re-executions', 'user approval 2026-10-26'),
      ('user-decision', 'U-6: no veto (m/k 75/41, veto 0)', 'user ruling 2026-10-27 ("推奨どおり", relayed by the coordinator)'),
      ('user-decision', 'U-2/U-3: the issuance-leg hold is paid only after SecurityCertified; free-prompt gets no coinbase carve '
                        'before certification (code: hold_issuance_reward_at_final_v1, palw_rfc0018_v1 free-prompt refusal)', 'user ruling 2026-10-27 ("推奨どおり", relayed by the coordinator)'),
      DRILL]),
]
TIER = [
    ('user-decision', 'c = 300 permille, kappa = 1000 permille from DAA 17,400; catalog edition 0 empty',
     'user decision 2026-10-27; c is the frozen premise (PRINCIPLES 9)'),
    RUN('core-lib-model_tier'), RUN('core-rfc0019_s11'), XD, V_REVIEW, V_REVIEW, XD,
    ('user-decision', 'readiness M1-M5: satisfied by the ACT2 report (edition 0 only, nothing added after the fence)', 'user ruling 2026-10-27 ("推奨どおり", relayed by the coordinator)'),  # 1 tier tests; 2 RFC-0019 s11 A-E (none exist); 3 R-1..R-6 shipped runs;
    # 4/5 "no later edition" (new review); 6 edition manifests (none); 7 readiness M1-M5 to the user
]
FRAME_VALUE = ('PALW_G14_TIR_FRAME_COST_ESTIMATE_V1 (inclusion margin 4 DAA, rents 1, acquisition 1 DAA, collection 8 DAA, reorg '
               'retention 360 DAA)')
FRAME_BASIS = 'lane VAL estimate, user approval 2026-10-26 of the estimates as values; not measured'


def main():
    g = genesis_hex()
    # Every run JSON must name ONE source commit (the final evidence run); a run that is missing, failed or from another commit
    # leaves its slot pending, so its record is not emitted.
    sources = {json.loads(f.read_text()).get('sourceCommit') for f in RUNS_DIR.glob('*.json')} if RUNS_DIR.exists() else set()
    if len(sources) > 1:
        sys.exit('run outputs name several source commits %s: re-run run_arming_evidence.py once on one commit' % sorted(sources))
    source = next(iter(sources), '0' * 40)

    def run_ok(rid):
        try:
            P.check_evidence(rid, str(RUNS_DIR / (rid + '.json')), source)
            return True
        except P.PinError:
            return False

    waivers, leftovers, records = [], [], {'g14': [], 'kernel-route': [], 'rfc0018': []}
    for family, variant, field, slots in TABLE:
        ws = [(i, s[0], s[1], s[2]) for i, s in enumerate(slots) if isinstance(s, tuple) and s[0] != 'run']
        ev = [(i, str(RUNS_DIR / (s[1] + '.json'))) for i, s in enumerate(slots)
              if isinstance(s, tuple) and s[0] == 'run' and run_ok(s[1])]
        left = [(i, s) for i, s in enumerate(slots) if isinstance(s, str)]
        left += [(i, 'R pending: ' + s[1]) for i, s in enumerate(slots) if isinstance(s, tuple) and s[0] == 'run' and not run_ok(s[1])]
        for i, kind, value, reason in ws + [(P.WAIVER_REVIEW,) + REVIEW]:
            waivers.append(P.make_waiver(field, g, AT, i, kind, value, reason, 'user 2026-10-27'))
        if left:
            leftovers += ['%s %s slot %d (%s)' % (family, field, i, c) for i, c in left]
            continue
        rec = P.build_record_with_waivers(family, variant, field, g, AT, source, len(slots), ev, ws, review_waiver=REVIEW)
        records[family].append(rec)
    tier_left = [(i, s) for i, s in enumerate(TIER) if isinstance(s, str)]
    tier_left += [(i, 'R pending: ' + s[1]) for i, s in enumerate(TIER) if isinstance(s, tuple) and s[0] == 'run' and not run_ok(s[1])]
    tier_slots = {}
    for i, s in enumerate(TIER):
        if isinstance(s, tuple) and s[0] != 'run':
            w = P.make_waiver('palw_model_tier_v1', g, AT, i, s[0], s[1], s[2], 'user 2026-10-27')
            waivers.append(w)
            tier_slots[i] = ('waiver', w['digest'])
        elif isinstance(s, tuple) and run_ok(s[1]):
            tier_slots[i] = ('run', P._digest(str(RUNS_DIR / (s[1] + '.json'))))
    tier_review = P.make_waiver('palw_model_tier_v1', g, AT, P.WAIVER_REVIEW, *REVIEW, 'user 2026-10-27')
    waivers.append(tier_review)
    leftovers += ['model-tier palw_model_tier_v1 slot %d (%s)' % (i, c) for i, c in tier_left]
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
    out.append('/// The frame cost record for FpSelection: the lane VAL estimate (ProvisionalEstimate) and the agent review. Emitted only\n'
               '/// with the final evidence run\'s candidate commit as its source (a zero commit is refused, never pinned).\n')
    if source == '0' * 40:
        out.append('pub const PALW_T12_17000_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] = &[];\n')
    else:
      out.append('pub const PALW_T12_17000_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] = &[\n'
               '    crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1 {\n'
               '        version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORD_VERSION_V1,\n'
               '        genesis: %s,\n        activation_daa: %d,\n        source_commit: %s,\n        measurement: %s,\n'
               '        review: %s,\n        cost: crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1,\n    },\n];\n'
               % (ge, AT, P._bytes(source), P._bytes(fc[0]['digest']), P._bytes(fc[1]['digest'])))
    if tier_left:
        out.append('\n/// The model-tier record: not emitted while slots %s are leftover.\n' % tier_left)
        out.append('pub const PALW_T12_17000_MODEL_TIER_RECORDS_V1: &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1] = &[];\n')
    else:
        tip = empty_genesis_catalog_root()
        ev = ''.join('            %s, // slot %d %s\n' % (P._bytes(tier_slots[i][1]), i, tier_slots[i][0]) for i in range(len(TIER)))
        out.append('\n/// The model-tier record: c 300, kappa 1000 (user decision 2026-10-27), catalog edition 0 (empty) as the tip.\n'
                   'pub const PALW_T12_17000_MODEL_TIER_RECORDS_V1: &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1] = &[\n'
                   '    crate::palw_model_tier_v1::PalwModelTierArmingRecordV1 {\n'
                   '        version: crate::palw_model_tier_v1::PALW_MODEL_TIER_ARMING_RECORD_VERSION_V1,\n'
                   '        genesis: %s,\n        activation_daa: %d,\n        registered_permille: 300,\n'
                   '        registered_cap_permille: 1000,\n        catalog_tip_edition: 0,\n'
                   '        catalog_tip_root: crate::Hash64::from_bytes(%s),\n        source_commit: %s,\n        evidence: &[\n%s        ],\n'
                   '        review: %s,\n    },\n];\n' % (ge, AT, P._bytes(tip), P._bytes(source), ev, P._bytes(tier_review['digest'])))
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
