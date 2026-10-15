"""Pinned RFC-0012 acceptance plan refusals; parser controls only, never node acceptance."""
import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc12_acceptance_plan as P  # noqa: E402


def valid_plan():
    return {
        'schemaVersion': 1, 'kind': 'rfc12-acceptance-plan', 'commit': 'ab' * 20,
        'network': {'kind': 'private-profile', 'profile': 'paid-v4'},
        'salt': '74' * 32,
        'binaries': {'node': '/owned/bin/misaka-rfc12-private-node', 'cli': '/owned/bin/misaka'},
        'identity': {'genesis': '11' * 64, 'consensusParamsId': '22' * 32, 'retirementActivationDaa': 1,
                     'retirementPolicyId': '33' * 64},
        'bounds': {'workflowTimeoutSeconds': 86400, 'phaseTimeoutSeconds': 7200, 'commandTimeoutSeconds': 3600,
                   'probeTimeoutSeconds': 15, 'factoryTimeoutSeconds': 120},
        'observationBoundsFile': '/owned/plan/bounds.json',
        'fees': {'maximumWorkflowCarrierFeeSompi': 10**9, 'maximumProviderAckFeePerCarrierSompi': 10**6,
                 'maximumProviderCarrierTotalSompi': 10**8, 'maximumKernelClaimFeeSompi': 10**6,
                 'maximumKernelCarrierTotalSompi': 10**7, 'maximumCheckerReceiptFeeSompi': 10**6,
                 'maximumCheckerCarrierTotalSompi': 10**7, 'maximumDepositLockFeeSompi': 10**6,
                 'maximumWorkflowTotalCostSompi': 10**10, 'maximumBindingReservationSompi': 0,
                 'maximumCompleteCheckFeeSompi': 0},
        'scan': {'maximumScannedChainBlocks': 500000, 'maximumMergesetBlockReads': 100000, 'maximumScannedEvmBlocks': 10000,
                 'maximumUtxoRowsPerAddress': 1000, 'kernelFinalsLimit': 1000},
        'settlement': {'minimumDepth': 1, 'minimumWork': '1'},
        'usefulWork': {'requestCommand': ['/owned/bin/request', '--rpc={PRODUCER_WRPC}', '--out={ROOT}/request.borsh'],
                       'requestFile': '{ROOT}/request.borsh', 'claimSalt': '44' * 64, 'commitDaaOffset': 0,
                       'expiresDaaSpan': 1000, 'kernelPayerSeat': 4, 'maxInputBytes': 1 << 20, 'maxMaterialBytes': 1 << 20,
                       'maxWorkspaceBytes': 1 << 24, 'maxUnits': 64,
                       'checker': {'hosts': ['checker'], 'round': 0, 'providerSeat': 1, 'payerSeats': [5],
                                   'maxUnitBytes': 4096, 'maxTotalFetchBytes': 1 << 20, 'maxWorkspaceBytes': 1 << 24,
                                   'maxRequests': 64, 'challengeValidUntilDaaSpan': None}},
        'topology': {'forkFixture': [['/owned/bin/fork', '{ROOT}']], 'partitionFixture': [['/owned/bin/partition']],
                     'pruneFixture': [['/owned/bin/prune', '{OLD_BLOCK}']], 'forkWinner': 'producer',
                     'partitionWinner': 'provider', 'minimumIsolatedDaaSpan': 5400, 'freshJoinSource': 'producer'},
        'scale': {'claimFixture': [], 'claimCounter': 'kernelFinalsTotal', 'minimumClaims': 100000,
                  'registryPopulation': {'enabled': False, 'bonds': 100000, 'timeoutSeconds': 600}},
        'crash': {'points': [
            {'name': 'outbox-before-tx', 'role': 'producer', 'phase': 'usefulWork',
             'trigger': {'kind': 'kernel-outbox', 'present': 'object.borsh', 'absent': 'transaction.borsh'},
             'maximumWaitSeconds': 600},
            {'name': 'import-utxo', 'role': 'crashJoin', 'phase': 'freshJoin',
             'trigger': {'kind': 'import-phase', 'phases': ['utxo-stream']}, 'maximumWaitSeconds': 600}],
            'generationMismatch': {'role': 'fresh', 'wrongProfile': 'paid-v3', 'exitWithinSeconds': 60},
            'conflictFixture': []},
        'capacity': {'mode': 'running-node', 'role': 'checker',
                     'constrainedArgs': ['--palw-native-resource-admission-v1', '--palw-native-memory-budget=1048576'],
                     'refusalWindowSeconds': 60, 'participationSignal': 'sink-follows'},
        'evm': {'consumerCheckpoints': ['usefulWork', 'fork', 'prune', 'freshJoin', 'coldReopen'],
                'roles': ['producer', 'provider', 'checker', 'fresh']},
        'bridge': {'depositorKey': 'funding', 'evmAddress': '0x' + '5a' * 20, 'depositAmountSompi': 10**9,
                   'claimTipSompi': 0, 'depositLockFeeSompi': 10**5, 'claimTimeoutDaaDelta': 100000,
                   'postClaimEvmBlocks': 10,
                   'refund': {'evmAddress': '0x' + '6b' * 20, 'amountSompi': 10**8, 'lockFeeSompi': 10**5,
                              'timeoutDaaDelta': 100, 'commands': [], 'destinationAddress': 'misakatest:qrefunddestination0',
                              'maximumRefundFeeSompi': 10**5},
                   'withdrawal': {'commands': [], 'sourceEvmAddress': '0x' + '5a' * 20,
                                  'destinationAddress': 'misakatest:qwithdrawaldest00', 'amountSompi': 10**7,
                                  'maximumGasWei': '1000000000000000'}},
        'market': {'enabled': False, 'openCommands': [], 'buyCommands': [], 'sellCommands': [],
                   'holderId': '77' * 64, 'lineId': '88' * 64, 'buyUnits': 2, 'sellUnits': 1},
        'accounting': {'incomeSeats': [0], 'providerPayerSeat': 3},
        'reconciliation': {'bondPageLimit': 100, 'maximumBondPages': 100},
        'coverage': {'archiveRole': 'provider', 'maximumWaitSeconds': 300},
    }


def raw(plan):
    return json.dumps(plan).encode()


class PlanControls(unittest.TestCase):
    def refuses(self, mutate, message=None):
        plan = valid_plan()
        mutate(plan)
        if message:
            with self.assertRaisesRegex(P.PlanError, message):
                P.parse_plan(raw(plan))
        else:
            with self.assertRaises(P.PlanError):
                P.parse_plan(raw(plan))

    def test_valid_plan_parses_and_pins_its_exact_bytes(self):
        data = raw(valid_plan())
        parsed = P.parse_plan(data)
        self.assertEqual(len(parsed['sha256']), 64)
        self.assertEqual(parsed['coverage']['prunedRole'], 'producer')
        self.assertNotEqual(parsed['sha256'], P.parse_plan(data + b' ')['sha256'], 'the hash covers the exact bytes')
        with self.assertRaises(P.PlanError):
            P.parse_plan(json.loads(data))  # a decoded object is not the pinned bytes

    def test_missing_or_unknown_keys_never_default(self):
        self.refuses(lambda p: p.pop('coverage'), 'missing')
        self.refuses(lambda p: p.update(extra=1), 'unknown')
        self.refuses(lambda p: p['fees'].pop('maximumWorkflowTotalCostSompi'), 'missing')
        self.refuses(lambda p: p['usefulWork']['checker'].update(compactRecordPath='/x'), 'unknown')
        self.refuses(lambda p: p['usefulWork']['checker'].pop('challengeValidUntilDaaSpan'), 'missing')

    def test_numbers_are_exact_integers(self):
        for value in (True, 1.0, -1, '1', None):
            self.refuses(lambda p, value=value: p['usefulWork'].update(commitDaaOffset=value))
        self.refuses(lambda p: p['scan'].update(kernelFinalsLimit=2**32))
        self.refuses(lambda p: p['settlement'].update(minimumWork='0'))
        self.refuses(lambda p: p['settlement'].update(minimumWork='01'))
        self.refuses(lambda p: p['bridge']['withdrawal'].update(maximumGasWei=10))

    def test_existing_drill_floors_are_kept(self):
        self.refuses(lambda p: p['scale'].update(minimumClaims=99_999))
        self.refuses(lambda p: p['scale']['registryPopulation'].update(bonds=99_999))
        self.refuses(lambda p: p['topology'].update(minimumIsolatedDaaSpan=5399))

    def test_a_preset_parses_only_to_be_recorded_blocked(self):
        plan = valid_plan()
        plan['network'] = {'kind': 'preset', 'name': 'testnet-12'}
        self.assertEqual(P.parse_plan(raw(plan))['network'], {'kind': 'preset', 'name': 'testnet-12'})
        self.refuses(lambda p: p.update(network={'kind': 'private-profile', 'profile': 'paid-v3'}))
        self.refuses(lambda p: p.update(network={'kind': 'preset', 'profile': 'paid-v4'}))

    def test_operator_commands_are_argv_with_a_closed_placeholder_set(self):
        self.refuses(lambda p: p['topology'].update(forkFixture=['/bin/sh -c fork']), 'argv')
        self.refuses(lambda p: p['topology'].update(forkFixture=[['/bin/fork', '{EVIL}']]), 'placeholder')
        self.refuses(lambda p: p['topology'].update(forkFixture=[['/bin/fork', '{ROOT!r}']]), 'placeholder')
        self.refuses(lambda p: p['topology'].update(forkFixture=[['/bin/fork', '{ROOT:>9}']]), 'placeholder')
        self.refuses(lambda p: p['topology'].update(forkFixture=[]), 'at least 1')
        self.refuses(lambda p: p['usefulWork'].update(requestFile='/tmp/request.borsh'), 'ROOT')
        self.refuses(lambda p: p['usefulWork'].update(requestFile='{ROOT}/../escape'), 'ROOT')

    def test_fill_refuses_unbound_placeholders_instead_of_empty_text(self):
        self.assertEqual(P.fill_argv(['--root={ROOT}'], {'ROOT': '/owned'}), ['--root=/owned'])
        with self.assertRaisesRegex(P.PlanError, 'OLD_BLOCK'):
            P.fill('{OLD_BLOCK}', {'ROOT': '/owned'})

    def test_carrier_payers_are_dedicated_seats(self):
        self.refuses(lambda p: p['usefulWork'].update(kernelPayerSeat=3), 'dedicated')
        self.refuses(lambda p: p['usefulWork']['checker'].update(payerSeats=[4]), 'dedicated')
        self.refuses(lambda p: p['usefulWork']['checker'].update(payerSeats=[5, 6]), 'one distinct payer')
        self.refuses(lambda p: p['accounting'].update(incomeSeats=[0, 5]), 'income')
        self.refuses(lambda p: p['accounting'].update(providerPayerSeat=4), 'seat 3')
        self.refuses(lambda p: p['usefulWork']['checker'].update(hosts=['fresh'], payerSeats=[5]))

    def test_crash_triggers_are_bound_to_their_roles_and_phases(self):
        self.refuses(lambda p: p['crash']['points'][1].update(role='producer'), 'cannot be armed')
        self.refuses(lambda p: p['crash']['points'][0].update(phase='fork'), 'cannot be armed')
        self.refuses(lambda p: p['crash']['points'][1]['trigger'].update(phases=['idle']), 'import phases')
        self.refuses(lambda p: p['crash']['points'][0]['trigger'].update(absent='object.borsh'), 'differ')
        self.refuses(lambda p: p['crash']['points'][1].update(name='outbox-before-tx'), 'unique')
        self.refuses(lambda p: p['crash']['points'].append(
            {'name': 'checker-wal', 'role': 'provider', 'phase': 'usefulWork', 'maximumWaitSeconds': 60,
             'trigger': {'kind': 'checker-outbox', 'present': 'receipt.borsh', 'absent': None}}), 'hosts no checker')
        self.refuses(lambda p: p['crash']['generationMismatch'].update(wrongProfile='population-v1'))

    def test_capacity_constraints_are_local_flags_only(self):
        self.refuses(lambda p: p['capacity'].update(constrainedArgs=['--override-params-file=/x']), 'allowlisted')
        self.refuses(lambda p: p['capacity'].update(constrainedArgs=['--palw-native-memory-budget']), 'positive u64')
        self.refuses(lambda p: p['capacity'].update(constrainedArgs=['--palw-native-memory-budget=0']), 'positive u64')
        self.refuses(lambda p: p['capacity'].update(constrainedArgs=['--palw-native-resource-admission-v1=1']), 'no value')
        self.refuses(lambda p: p['capacity'].update(constrainedArgs=['--palw-resource-admission-v2'] * 2), 'repeats')
        self.refuses(lambda p: p['capacity'].update(mode='import'))
        plan = valid_plan()
        plan['capacity'].update(mode='import', role='crashJoin')
        P.parse_plan(raw(plan))
        self.refuses(lambda p: p['capacity'].update(mode='import', role='crashJoin', participationSignal='produced-blocks-increase'))
        self.refuses(lambda p: p['capacity'].update(participationSignal='provider-store-grows'), 'provider')

    def test_bridge_steps_keep_accounts_and_windows_separable(self):
        self.refuses(lambda p: p['bridge']['refund'].update(evmAddress='0x' + '5a' * 20), 'separate EVM')
        self.refuses(lambda p: p['bridge']['withdrawal'].update(sourceEvmAddress='0x' + '6b' * 20), 'credited deposit')
        self.refuses(lambda p: p['bridge']['refund'].update(timeoutDaaDelta=100000), 'shorter')
        self.refuses(lambda p: p['bridge']['refund'].update(maximumRefundFeeSompi=10**8), 'positive refunded')
        self.refuses(lambda p: p['bridge']['withdrawal'].update(destinationAddress='misakatest:qrefunddestination0'))
        self.refuses(lambda p: p['bridge'].update(claimTipSompi=10**9 + 1))
        self.refuses(lambda p: p['bridge'].update(depositLockFeeSompi=10**6 + 1), 'maximumDepositLockFeeSompi')
        self.refuses(lambda p: p['bridge'].update(evmAddress='0x' + '00' * 20))

    def test_bounds_and_coverage_relations(self):
        self.refuses(lambda p: p['bounds'].update(phaseTimeoutSeconds=86401), 'cannot exceed')
        self.refuses(lambda p: p['bounds'].update(probeTimeoutSeconds=7201))
        self.refuses(lambda p: p['fees'].update(maximumProviderAckFeePerCarrierSompi=10**8 + 1))
        self.refuses(lambda p: p['coverage'].update(archiveRole='checker') or p['usefulWork']['checker'].update(hosts=['provider']),
                     'custody node')
        plan = valid_plan()
        plan['coverage']['archiveRole'] = 'checker'
        P.parse_plan(raw(plan))
        self.refuses(lambda p: p['evm'].update(roles=['producer']), 'two distinct')


if __name__ == '__main__':
    unittest.main()
