#!/usr/bin/env python3
"""Freeze then run one RFC06/07/08/12 campaign on one candidate and one testnet.

Implementation must be complete before --execute is used. This entry point
never builds, swaps binaries, resets nodes, or creates per-scenario databases.
Without --execute it writes a freeze/UNKNOWN report and launches no children.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile

import rfc_gate_inputs
from rfc_integration_campaign import (CampaignError, FrozenCampaign, GATES, final_status,
    load_base_drill, make_campaign_drill, require)


def exclusive_json(path, value):
    path = Path(path)
    require(not path.is_symlink() and not any(p.is_symlink() for p in path.parents), 'output path traverses a symlink')
    with path.open('x') as output:
        json.dump(value, output, indent=2)
        output.write('\n')
        output.flush()
        os.fsync(output.fileno())
    parent_fd = os.open(str(path.parent), os.O_RDONLY)
    try:
        os.fsync(parent_fd)
    finally:
        os.close(parent_fd)


def gates_report(frozen, scenarios, base_passed, resources_passed):
    facts = {fact['id']: fact for row in scenarios for fact in row.get('facts', [])}
    facts['base-drill-mechanics'] = dict(id='base-drill-mechanics', kind='base-drill-mechanics',
        status='PASS' if base_passed and resources_passed else 'UNKNOWN',
        authority='actual inherited MeasuredDrill run + every owned launch/resource sample; no soundness approval')
    result = {}
    for name, needed_kind in GATES.items():
        evidence = []
        for fact_id in frozen.plan['gates'][name]:
            fact = facts.get(fact_id)
            evidence.append(fact if fact is not None else dict(id=fact_id, status='UNKNOWN', reason='required stage/fact was never observed'))
        status = 'FAIL' if any(fact.get('status') == 'FAIL' for fact in evidence) else (
            'PASS' if all(fact.get('status') == 'PASS' for fact in evidence) else 'UNKNOWN')
        result[name] = dict(status=status,
            requiredKind=needed_kind, evidence=evidence)
    return rfc_gate_inputs.cap_gates(result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', required=True)
    parser.add_argument('--out', required=True, help='new aggregate report; never overwrite an existing campaign report')
    parser.add_argument('--root', help='new owned campaign directory; each role gets one DB reused throughout')
    parser.add_argument('--inputs', help='optional external-input manifest; unavailable K3/native39 inputs are refused, never substituted')
    parser.add_argument('--aggregate-root', help='existing campaign root: rebuild the report from its scenario ledger without launching anything')
    parser.add_argument('--execute', action='store_true', help='only after all implementation is complete; run the frozen campaign once')
    args = parser.parse_args()
    require(not Path(args.out).exists(), 'campaign output already exists; preserving the prior report')
    if args.inputs:
        rfc_gate_inputs.load_input_manifest(args.inputs)
    frozen = FrozenCampaign(args.plan)
    base = load_base_drill()
    base.validate_scenario_bounds(frozen.drill)
    limits = base.read_process_limits(frozen.limits_pin.path)
    freeze = frozen.manifest()
    if args.aggregate_root:
        rfc_gate_inputs.verify_aggregate_root(args.aggregate_root, freeze)
        ledger = Path(args.aggregate_root) / 'scenario-ledger.jsonl'
        lines = ledger.read_text().splitlines() if ledger.is_file() else []
        scenarios = rfc_gate_inputs.aggregate_ledger(lines, [row['id'] for row in frozen.plan['scenarios']])
        # base-drill mechanics cannot be reconstructed from a ledger: stays UNKNOWN.
        gates = gates_report(frozen, scenarios, False, False)
        status = 'FAIL' if any(row.get('status') == 'FAIL' for row in scenarios) else 'UNKNOWN'
        exclusive_json(args.out, dict(schemaVersion=1, kind='misaka-rfc-integration-campaign-report-v1', status=status,
            reason='re-aggregated from scenario ledger; nothing was launched or resumed; never PASS',
            freeze=freeze, actualExecution=False, scenarios=scenarios, gates=gates, securitySoundnessMeasured=False))
        return 1 if status == 'FAIL' else 2
    if not args.execute:
        exclusive_json(args.out, dict(schemaVersion=1, kind='misaka-rfc-integration-campaign-report-v1',
            status='UNKNOWN', reason='freeze only; implementation-complete execution was not requested',
            freeze=freeze, actualExecution=False, securitySoundnessMeasured=False))
        return 2
    if args.root:
        root = Path(args.root).absolute()
        require(not root.exists() and not any(parent.is_symlink() for parent in (root,) + tuple(root.parents)), 'campaign root must be a new owned directory')
        root.mkdir(mode=0o700)
    else:
        root = Path(tempfile.mkdtemp(prefix='misaka-rfc-campaign-')).resolve(strict=True)
    exclusive_json(root / 'campaign-freeze.json', freeze)
    # The executable plan, explicit scenario/claim ids and evidence contracts
    # are durably fixed before the first child. Later writes cannot redefine PASS.
    freeze_bytes = (root / 'campaign-freeze.json').read_bytes()
    marker_sha256 = hashlib.sha256(freeze_bytes).hexdigest()
    exclusive_json(root / 'campaign-once.json', dict(campaignId=frozen.plan['campaignId'], freezeSha256=marker_sha256,
        candidateSha256=frozen.candidate.sha256, root=str(root), actualExecutionStarted=True))
    drill = make_campaign_drill(base)(frozen, root)
    drill.ledger_path = root / 'scenario-ledger.jsonl'
    result = dict(schemaVersion=1, kind='misaka-rfc-integration-campaign-report-v1', freeze=freeze,
        status='UNKNOWN', actualExecution=True, artifactDirectory=str(root), securitySoundnessMeasured=False,
        releaseApproval=False, baseDrill=dict(status='UNKNOWN'), resourceLimits=limits, resourceLimitsMet=False,
        campaignFreezeSha256=marker_sha256)
    base_passed, resources_passed, fatal = False, False, None
    try:
        frozen.validate()
        drill.stage('run.before')
        # Exactly one call. MeasuredDrill owns all A/B/JOIN launches, P2P links,
        # fork/partition/pruning/IBD/crash/DNS mechanics and database lifetimes.
        drill.run()
        base_passed = True
        result['baseDrill']['status'] = 'PASS'
        drill.stage('run.after')
    except base.Blocked as error:
        result['baseDrill'].update(status='UNKNOWN', reason=str(error))
    except CampaignError as error:
        fatal = str(error)
        result['baseDrill'].update(status='FAIL', reason=str(error))
    except Exception as error:
        fatal = type(error).__name__ + ': ' + str(error)
        result['baseDrill'].update(status='FAIL', reason=fatal)
    finally:
        try:
            drill.close()
        except Exception as error:
            fatal = 'owned cleanup failed: ' + repr(error)
        try:
            result['sampledOwnedResources'] = drill.sampler.stop()
            result['requiredNodeLaunchSamples'] = drill.node_launches
            result['ownedDatabaseLogicalBytes'] = {role: sum(path.stat().st_size for path in (root / role).rglob('*') if path.is_file())
                for role in ('A', 'B', 'JOIN')}
            result['probeLatenciesMs'] = drill.latencies
            base.check_process_limits(limits, result)
            resources_passed = True
        except Exception as error:
            result['resourceObservationError'] = repr(error)
        try:
            frozen.validate()
            require(hashlib.sha256((root / 'campaign-freeze.json').read_bytes()).hexdigest() == marker_sha256, 'freeze marker changed during campaign')
        except Exception as error:
            fatal = 'post-run identity failure: ' + repr(error)
        scenarios = drill.scenario_results
        observed = {row['id'] for row in scenarios}
        for planned in frozen.plan['scenarios']:
            if planned['id'] not in observed:
                scenarios.append(dict(id=planned['id'], kind=planned['kind'], stage=planned['stage'], claims=planned['claims'],
                    status='UNKNOWN', reason='frozen stage was never reached; no attack prevention inferred', facts=[]))
        gates = gates_report(frozen, scenarios, base_passed, resources_passed)
        result.update(scenarios=scenarios, gates=gates, stageContexts=drill.stage_records,
            identityChecks=drill.integrity_records, pinVerificationCosts=frozen.integrity_costs,
            fixtureCommands=drill.command_observations, resolvedCommandDigests=drill.resolved_commands,
            nodeObservations=drill.observations, kernelWorkMilestones=drill.kernel_observations,
            nativePaidObservations=drill.paid_observations,
            resourceLimitsMet=resources_passed)
        result['status'] = final_status(scenarios, gates, base_passed, resources_passed, fatal)
        if fatal is not None:
            result['fatal'] = fatal
        exclusive_json(args.out, result)
    print(json.dumps(dict(status=result['status'], campaignId=frozen.plan['campaignId'], report=str(Path(args.out).absolute()),
        candidateSha256=frozen.candidate.sha256, securitySoundnessMeasured=False)))
    return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (CampaignError, OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps(dict(status='FAIL', actualExecution='UNKNOWN', reason=type(error).__name__ + ': ' + str(error))))
        raise SystemExit(1)
