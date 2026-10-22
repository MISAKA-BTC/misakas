"""Unexecuted source controls: counterexamples do not establish live acceptance."""
from copy import deepcopy
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc_integration_campaign as campaign


def h(number):
    return ('%02x' % number) * 64


def plan_and_drill():
    nodes = {}
    for i, role in enumerate(('A', 'B', 'JOIN')):
        nodes[role] = dict(argv=['/candidate/kaspad', '--nodnsseed', '--outpeers=0', '--appdir={' + role + '_APPDIR}'],
            wrpc='127.0.0.1:' + str(1000 + i), p2p='127.0.0.1:' + str(2000 + i), eth='http://127.0.0.1:' + str(3000 + i))
    normal = dict(id='normal', kind='normal', rfc=[6, 7, 8, 12], stage='fixture.bootstrap.after', roles=['A', 'B'],
        claims=[h(3), h(5)], commands=[], observationTimeoutSeconds=5, requirements=[])
    attack = dict(id='attack', kind='attack', rfc=[6, 7, 8, 12], stage='milestone.partition-healed', roles=['A', 'B'],
        claims=[h(4)], commands=[['/candidate/submitter', '/candidate/attack.json']], observationTimeoutSeconds=5, requirements=[])
    for kind in sorted(set(campaign.GATES.values()) - {'base-drill-mechanics'}):
        normal['requirements'].append(dict(id='normal.' + kind, kind=kind, role='A', claimId=h(3), expected={}))
    for claim in normal['claims']:
        for role in normal['roles']:
            normal['requirements'].append(dict(id='normal.claim-' + str(normal['claims'].index(claim)) + '-' + role.lower(),
                kind='native-claim-observed', role=role, claimId=claim, expected=dict(classId=h(6), jobId=h(7),
                    producerBond=h(8) + ':0', expectedConvicted=False, maximumRecordBytes=100)))
    for kind in ('attack-submission', 'attack-import', 'attack-adjudication'):
        attack['requirements'].append(dict(id='attack.' + kind, kind=kind, role='B', claimId=h(4), expected={}))
    gates = {gate: ['base-drill-mechanics'] if kind == 'base-drill-mechanics' else ['normal.' + kind]
        for gate, kind in campaign.GATES.items()}
    plan = dict(schemaVersion=1, kind=campaign.KIND, campaignId='one-integrated-run',
        candidate=dict(path='/candidate/kaspad', sha256='11' * 32), sourceClaims=dict(revision='declared-source', buildInputs={}),
        network=dict(network='misaka-testnet-12', genesisHash=h(1), consensusParamsId=h(2)),
        drillPlan=dict(path='/candidate/drill.json', sha256='22' * 32), resourceLimits=dict(path='/candidate/limits.json', sha256='33' * 32),
        integrityPolicy=dict(fullDigestIntervalSeconds=60, maximumLogicalFileReadBytes=10**9, maximumSingleCheckMilliseconds=60_000),
        pinnedInputs=[], scenarios=[normal, attack], gates=gates)
    return plan, dict(nodes=nodes, ruleset_id=h(2))


class FrozenPlanCounterexamples(unittest.TestCase):
    def test_one_candidate_must_be_used_by_all_node_launches(self):
        plan, drill = plan_and_drill()
        campaign.validate_campaign(plan, drill)
        drift = deepcopy(drill)
        drift['nodes']['JOIN']['argv'][0] = '/candidate/another-kaspad'
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drift)
        drift = deepcopy(drill)
        drift['nodes']['B']['argv'][-1] = '--appdir={ROOT}/scenario-2-db'
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drift)

    def test_no_submission_or_no_native_import_is_never_an_attack(self):
        plan, drill = plan_and_drill()
        plan['scenarios'][1]['commands'] = []
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)
        plan, drill = plan_and_drill()
        plan['scenarios'][1]['requirements'] = plan['scenarios'][1]['requirements'][:1]
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)

    def test_gate_cannot_be_satisfied_with_a_weaker_native_claim_identity(self):
        plan, drill = plan_and_drill()
        plan['gates']['rfc07-selected-approval'] = ['normal.native-fee-settlement']
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)
        plan, drill = plan_and_drill()
        del plan['gates']['rfc07-cumulative-soundness']
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)

    def test_fresh_join_cannot_be_claimed_before_the_process_exists(self):
        plan, drill = plan_and_drill()
        plan['scenarios'][0]['roles'] = ['A', 'JOIN']
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)

    def test_extra_declared_claim_without_its_own_receipts_is_not_multiple_claim_evidence(self):
        plan, drill = plan_and_drill()
        plan['scenarios'][1]['claims'].append(h(9))
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)
        plan, drill = plan_and_drill()
        plan['scenarios'][0]['requirements'] = [fact for fact in plan['scenarios'][0]['requirements']
            if not (fact['kind'] == 'native-claim-observed' and fact['role'] == 'B')]
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)

    def test_builds_inline_evaluation_and_shells_are_not_campaign_commands(self):
        for argv in (['/usr/bin/cargo', 'build'], ['/usr/bin/bash', 'plan.sh'], ['/usr/bin/python3', '-c', 'print("PASS")']):
            with self.assertRaises(campaign.CampaignError):
                campaign.validate_argv(argv)

    def test_identical_bytes_at_replaced_inode_do_not_replace_a_frozen_binary(self):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary).resolve()
            binary = folder / 'candidate'
            binary.write_bytes(b'candidate bytes, not an executable test')
            pin = campaign.FilePin.capture(str(binary), hashlib.sha256(binary.read_bytes()).hexdigest())
            replacement = folder / 'replacement'
            replacement.write_bytes(binary.read_bytes())
            replacement.replace(binary)
            with self.assertRaises(campaign.CampaignError):
                pin.revalidate()


class ActualEvidenceCounterexamples(unittest.TestCase):
    def test_old_final_and_receipt_count_do_not_create_an_approval_collector(self):
        base = type('FakeBase', (), dict(MeasuredDrill=object))
        drill_type = campaign.make_campaign_drill(base)
        requirement = dict(id='approval', kind='selected-verifier-approval', role='A', claimId=h(3),
            expected=dict(state='Final', receiptCount=100))
        reader = SimpleNamespace(plan=dict(nodes=dict(A=dict(argv=[]))))
        result = drill_type.fact(reader, requirement)
        self.assertEqual(result['status'], 'UNKNOWN')
        self.assertFalse(result['nativeAuthorityEstablished'])

    def test_paid_scope_requires_exact_role_claim_flag_and_cannot_replace_approval_gate(self):
        plan, drill = plan_and_drill()
        request = dict(id='paid.rows', kind='native-paid-observed', role='A', claimId=h(3), expected={})
        plan['scenarios'][0]['requirements'].append(request)
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)
        drill['nodes']['A']['argv'] += ['--private-probe-profile=paid-v4',
            '--private-probe-paid-observe-claims=' + h(3)]
        campaign.validate_campaign(plan, drill)
        plan['gates']['rfc07-selected-approval'] = ['paid.rows']
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)

    def test_source_paid_policy_is_explicit_pinned_and_identical_even_for_idle_join(self):
        plan, drill = plan_and_drill()
        policy = '/frozen/source-policy.borsh'
        for node in drill['nodes'].values():
            node['argv'] += ['--private-probe-profile=source-paid-v1', '--private-probe-source-policy=' + policy]
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)
        plan['pinnedInputs'].append(dict(path=policy, sha256='aa' * 32))
        campaign.validate_campaign(plan, drill)
        drill['nodes']['JOIN']['argv'][-1] = '--private-probe-source-policy=/frozen/other-policy.borsh'
        with self.assertRaises(campaign.CampaignError):
            campaign.validate_campaign(plan, drill)

    def test_missing_import_or_timeout_never_counts_as_a_defensive_success(self):
        scenarios = [dict(id='attack', status='UNKNOWN', reason='submission/import timeout', facts=[])]
        self.assertEqual(campaign.final_status(scenarios, {'gate': dict(status='PASS')}, True, True), 'UNKNOWN')
        self.assertEqual(campaign.final_status([dict(status='PASS')], {'gate': dict(status='UNKNOWN')}, True, True), 'UNKNOWN')
        self.assertEqual(campaign.final_status([dict(status='PASS')], {'gate': dict(status='PASS')}, False, True), 'UNKNOWN')

    def test_live_native_claim_identity_still_does_not_authenticate_paid_approval(self):
        requirement = dict(id='claim', kind='native-claim-observed', role='A', claimId=h(3), expected=dict(
            classId=h(4), jobId=h(5), producerBond=h(6) + ':0', expectedConvicted=False, maximumRecordBytes=100))
        payload = dict(available=True, found=True, claimId=h(3), classId=h(4), jobId=h(5), producerBond=h(6) + ':0',
            tipDaa=10, convicted=False, publicRecord='0102', recordHeader='0304', ledgerRoot=h(7), auxRoot=h(8),
            state='Final', receiptCount=999, settlement='paid')
        result = campaign.native_claim_fact(requirement, payload, dict(virtualDaaScore=10))
        self.assertEqual(result['status'], 'PASS')
        self.assertEqual(result['selectedVerifierApproval'], 'UNKNOWN')
        self.assertEqual(result['settlement'], 'UNKNOWN')
        payload['classId'] = h(9)
        with self.assertRaises(campaign.NativeMismatch):
            campaign.native_claim_fact(requirement, payload, dict(virtualDaaScore=10))


class ImportProgressAndPinCostControls(unittest.TestCase):
    def test_fresh_join_import_is_pending_while_network_identity_remains_mandatory(self):
        base = type('FakeBase', (), dict(MeasuredDrill=object,
            import_stamp=staticmethod(lambda node: (node['epoch'], node['point'], node['phase'], node['stable'])),
            probe_context=staticmethod(lambda dag, node: (dag['sink'], node['epoch'], node['phase']))))
        drill_type = campaign.make_campaign_drill(base)
        reader = SimpleNamespace(frozen=SimpleNamespace(plan=dict(network=dict(genesisHash=h(1), consensusParamsId=h(2)))))
        observation = dict(dag=dict(sink=h(3)), node=dict(genesisHash=h(1), consensusParamsId=h(2),
            epoch=1, point=h(4), phase='applying', stable=False))
        self.assertFalse(drill_type.assert_network(reader, observation))
        observation['node'].update(point='', phase='idle', stable=True)
        self.assertTrue(drill_type.assert_network(reader, observation))
        observation['node']['genesisHash'] = h(5)
        with self.assertRaises(campaign.CampaignIdentityError):
            drill_type.assert_network(reader, observation)

    def test_hot_metadata_probe_does_not_rehash_and_full_check_counts_actual_file_reads(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary).resolve() / 'candidate'
            binary.write_bytes(b'pin me')
            pin = campaign.FilePin.capture(str(binary))
            read_bytes = []
            pin.observe_metadata()
            pin.revalidate(on_read=read_bytes.append)
            self.assertEqual(sum(read_bytes), len(b'pin me'))


if __name__ == '__main__':
    unittest.main()
