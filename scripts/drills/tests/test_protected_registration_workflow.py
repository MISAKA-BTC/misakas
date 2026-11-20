"""Orchestrator refusal cases with constructed replies, never chain evidence."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from protected_registration_workflow import RegistrationWorkflow, chain_identity, registration_status, next_control
from protected_private_registration_run import fund_private_carriers, registration_peer, relay_registered_control, require_independent_funding, warmup_checkpoint
from protected_inventory_acceptance import AcceptancePending
from rfc12_acceptance_observe import ContextMoved
from test_protected_claim_observation import Rpc as BaseRpc, h


class Rpc(BaseRpc):
    def __init__(self):
        super().__init__()
        self.view = dict(schema=1, openId=h(22), acceptedParent=dict(block=h(10), daa=199),
                         acceptedParentRoot=h(23), prospectiveDaa=200,
                         registration=dict(openId=h(22), openBodyHex='abcd', policyId=h(24),
                         accepted=dict(block=h(8), daa=198), quote=dict(partCount=2), chargeId=h(25), status='pending',
                         parts=[dict(index=i, offset=i*64, bytes=64, reservedDaa=200+i,
                                     received=False, accepted=None) for i in range(2)],
                         finishes=[dict(kind=k, reservedDaa=202+i, attempted=False)
                                   for i, k in enumerate(('normal', 'recovery'))], registrationId=None, terminal=None))
        self.found = True

    def call(self, method, request=None):
        if method == 'getPalwTokenizerRegistrationStatus':
            self.calls.append(method)
            result = dict(available=True, found=self.found, observationJson=json.dumps(self.view))
            if self.mutate:
                self.mutate(self, method, request)
            return result
        return super().call(method, request)

    def observe(self, previous=None):
        return registration_status(self, self.expected, h(22), 'abcd', previous)


class RegistrationTests(unittest.TestCase):
    def test_moving_snapshot_does_not_advance_past_current_control_slot(self):
        workflow = object.__new__(RegistrationWorkflow)
        workflow.deadline = time.monotonic() + 5
        workflow.progress = lambda: self.fail('moving snapshot must be re-read at the current slot')
        with patch('protected_registration_workflow.time.sleep'), patch(
                'protected_registration_workflow.registration_status',
                side_effect=[ContextMoved('peer advanced during read'), {'prospectiveDaa': 200}]) as read:
            self.assertEqual(workflow.wait(read), {'prospectiveDaa': 200})
            self.assertEqual(read.call_count, 2)

    def test_native_moving_tokenizer_view_is_polled_without_advancing(self):
        rpc = Rpc()

        for reason in ('tokenizer selected view changed; retry the read', 'tokenizer parent unavailable: NoOpinion'):
            with self.subTest(reason=reason):
                def moved(_, method, _request):
                    if method == 'getPalwTokenizerRegistrationStatus':
                        raise RuntimeError(reason)

                rpc.mutate = moved
                with self.assertRaisesRegex(ContextMoved, 'native tokenizer selected view temporarily unavailable'):
                    rpc.observe()

    def test_relay_supplies_pending_change_before_the_tokenizer_control(self):
        class Pool:
            def __init__(self, entries):
                self.entries = entries
                self.submitted = []

            def call(self, method, request):
                if method == 'getMempoolEntry':
                    txid = request['transactionId']
                    if txid not in self.entries:
                        raise RuntimeError('transaction not found')
                    return dict(mempoolEntry=dict(is_orphan=False, transaction=self.entries[txid]))
                self.assert_method(method, request)
                tx = request['transaction']
                txid = tx['verboseData']['transactionId']
                self.submitted.append(txid)
                self.entries[txid] = tx
                return dict(transactionId=txid)

            @staticmethod
            def assert_method(method, request):
                assert method == 'submitTransaction' and request['allowOrphan'] is False

        funding, control = h(1), h(2)
        source = Pool({funding: dict(verboseData=dict(transactionId=funding), inputs=[]),
                       control: dict(verboseData=dict(transactionId=control), inputs=[
                           dict(previousOutpoint=dict(transactionId=funding))])})
        miner = Pool({})
        relay_registered_control(source, miner, control, 2, lambda: True)
        self.assertEqual(miner.submitted, [funding, control])

    def test_registrant_uses_extra_seat_owner_and_refuses_its_reserved_float(self):
        float_id, carrier_id = h(1), h(2)
        bond = dict(bondOutpoint=f'{h(3)}:0', feeFloatOutpoint=f'{float_id}:0', address='private seat')
        bootstrap = dict(seats=[dict(bond=dict(bondOutpoint=f'{h(4)}:0')), dict(bond=bond)])
        config = dict(peers=[{}, dict(seat=0, extraSeats=[1])], registration=dict(bond=bond['bondOutpoint']))
        self.assertEqual(registration_peer(config, bootstrap), 1)

        class FundingRpc:
            def __init__(self, locked, txids):
                self.locked, self.txids = locked, txids

            def call(self, method, request):
                if method == 'getPalwProducerFacts':
                    return dict(lockedBondOutpoints=self.locked)
                return dict(entries=[dict(outpoint=dict(transactionId=txid, index=0),
                                          utxoEntry=dict(isCoinbase=False, amount=100)) for txid in self.txids])

        with self.assertRaisesRegex(ValueError, 'does not expose'):
            require_independent_funding(FundingRpc([], [float_id, carrier_id]), bond, 99)
        with self.assertRaisesRegex(ValueError, 'separate confirmed'):
            require_independent_funding(FundingRpc([], [float_id]), bond, 99, require_reservation=False)
        with self.assertRaisesRegex(ValueError, 'separate confirmed'):
            require_independent_funding(FundingRpc([bond['feeFloatOutpoint']], [float_id]), bond, 99)
        self.assertEqual(require_independent_funding(FundingRpc([bond['feeFloatOutpoint']], [float_id, carrier_id]), bond, 99),
                         dict(outpoint=f'{carrier_id}:0', amountSompi=100))

    def test_warmup_resume_reuses_exact_peer_policy_and_refuses_started_registration(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            config = dict(peers=[{}, {}], initialTimestampMs=100)
            profile = dict(genesis=h(1), consensusParamsId=h(2)[64:])
            (root / 'run.json').write_text(json.dumps(config))
            (root / 'clock').mkdir()
            (root / 'clock/clock.json').write_text(json.dumps(dict(version=1, genesis=h(1), timestampMs=120)))
            paths = [root / 'peer-0', root / 'peer-1']
            for path in paths:
                path.mkdir()
                (path / 'rfc12-private-probe-v1.json').write_text(json.dumps(profile))
            self.assertEqual(warmup_checkpoint(config, root, dict(profile=profile)), (paths, 120))
            with self.assertRaisesRegex(ValueError, 'another private policy'):
                warmup_checkpoint(config, root, dict(profile=dict(profile, genesis=h(3))))
            resumed = root / 'resumed'
            resumed.mkdir()
            (resumed / 'clock').mkdir()
            (resumed / 'run.json').write_text(json.dumps(config))
            (resumed / 'warmup-resume.json').write_text(json.dumps(dict(source=str(root))))
            (resumed / 'clock/clock.json').write_text(json.dumps(dict(version=1, genesis=h(1), timestampMs=220)))
            self.assertEqual(warmup_checkpoint(config, resumed, dict(profile=profile)), (paths, 220))
            (resumed / 'warmup-resume.json').write_text(json.dumps(dict(source=str(resumed))))
            with self.assertRaisesRegex(ValueError, 'circular'):
                warmup_checkpoint(config, resumed, dict(profile=profile))
            (root / 'registration').mkdir()
            with self.assertRaisesRegex(ValueError, 'never began registration'):
                warmup_checkpoint(config, root, dict(profile=profile))

    def test_fresh_funding_sends_once_and_waits_for_every_peer(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            for name in ('manifest.json', 'main.seed', 'misaka'):
                (root / name).touch()
            registration = dict(bootstrap=str(root / 'manifest.json'), cli=str(root / 'misaka'),
                                commandTimeoutSeconds=5, salt='59' * 32)
            main = dict(address='private main', seedFile='main.seed')
            bond = dict(address='private seat', bondOutpoint=f'{h(20)}:0', feeFloatOutpoint=f'{h(21)}:0')
            clients = [BaseRpc(), BaseRpc()]
            boot = dict(main=main, profile=dict(saltId='ec7f746d72e5b4d1'))
            peers = [dict(borsh=53502), dict(borsh=53512)]
            recipients = [('studio', 0, bond, 20)]
            _, stamp = chain_identity(clients[0], clients[0].expected)
            accepted = dict(acceptingBlock=h(23), context=list(stamp))
            ready = False

            def read_funds(_rpc, *_args, transaction=None, **_kwargs):
                if ready and transaction == h(22):
                    return dict(outpoint=f'{h(22)}:0', amountSompi=10_000_000_000)
                return None

            def send(argv, **kwargs):
                self.assertEqual(argv[argv.index('--rpc') + 1], '127.0.0.1:53502')
                result = {'ok': True, 'dryRun': False, 'from': main['address'], 'to': bond['address'],
                          'txid': h(22), 'amountSompi': 10_000_000_000}
                kwargs['stdout'].write(json.dumps(result).encode())

            def step():
                nonlocal ready
                self.assertFalse(ready)
                ready = True

            with (patch('protected_private_registration_run.independent_funding', side_effect=read_funds),
                  patch('protected_private_registration_run.subprocess.run', side_effect=send) as submit,
                  patch('protected_private_registration_run.observe_transaction',
                        side_effect=[accepted, AcceptancePending(), accepted, accepted])):
                fund_private_carriers(root, registration, boot, clients[0].expected, peers, clients, recipients, step)
                submit.assert_called_once()
            result = json.loads((root / 'fund-studio/acceptance.json').read_text())
            self.assertTrue(ready)
            self.assertEqual(result['outputs'], [dict(outpoint=f'{h(22)}:0', amountSompi=10_000_000_000)] * 2)

    def test_workflow_hands_off_accepting_clock_and_waits_for_native_completion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bootstrap = root / 'bootstrap.json'
            bootstrap.write_text(json.dumps(dict(networkDomain='01' * 64, profile=dict(protectedClaimPrivatePolicyV3=dict(activationDaa=100)))))
            workflow = object.__new__(RegistrationWorkflow)
            workflow.output, workflow.rpc = root, Rpc()
            workflow.expected = workflow.rpc.expected
            workflow.chain = dict(workflow.expected, networkDomain='01' * 64)
            workflow.activation_daa = 100
            workflow.config = dict(bootstrap=str(bootstrap), artifact='artifact', container='container', semantics='semantics',
                                  rawTokenizer='tokenizer', eogOpening='eog', policy='policy', bond='bond', palwClass='palw-class',
                                  salt='59'*32, inventoryAcceptSpanDaa=50, maximumInventoryPrepaidSompi=100,
                                  tkrAcceptSpanDaa=5, tkrFinishSpanDaa=37, maximumRegistrationChargeSompi=100,
                                  maximumTkrFeeSompi=200, nonce=h(50))
            workflow.deadline = time.monotonic() + 5
            workflow.prefix, workflow.tokenizer, workflow.key = ['cli'], ['cli', 'tokenizer'], ['--key-file', 'private-seed']
            workflow.progress = lambda: self.fail('unexpected wait without a state transition')
            calls = []
            def submit(label, paths):
                calls.append((label, list(map(str, paths))))
                return [dict(acceptedDaa=190+i) for i in range(len(paths))]
            workflow.submit_objects = submit
            def run(label, argv, json_result=True):
                calls.append((label, argv))
                if label == 'inventory-continue-build':
                    self.assertEqual(argv[argv.index('--accepted-open-daa')+1], '190')
                    return dict(parts=2)
                if label == 'parent-pins':
                    self.assertEqual(argv[argv.index('--semantics-bound-daa')+1], '191')
                    return dict(parentsAuthenticated=False, partialSpecification={})
                if label == 'tkr-draft':
                    (root / 'tkr-draft').mkdir()
                    (root / 'tkr-draft/open-body.borsh').write_bytes(bytes.fromhex('abcd'))
                    return dict(openId=h(22))
                if label.startswith('tkr-send-'):
                    row = workflow.rpc.view['registration']
                    now = workflow.rpc.view['prospectiveDaa']
                    slot = None if label == 'tkr-send-open' else now
                    if 'part-' in label:
                        index = 0 if 'part-0000' in label else 1
                        row['parts'][index].update(received=True, accepted=dict(block=h(60+index), daa=now))
                        workflow.rpc.view['prospectiveDaa'] += 1
                        workflow.rpc.dag['virtualDaaScore'] += 1
                    elif 'finish-' in label:
                        row.update(status='complete', registrationId=h(70), terminal=dict(block=h(71), daa=now))
                        row['finishes'][0]['attempted'] = True
                    return dict(submitted=True, openId=h(22), reservedDaa=slot, txid=h(80))
                return {}
            workflow.run = run
            result = workflow.execute(relay_control=lambda txid, slot: calls.append(('relay', (txid, slot))))
            self.assertTrue(result['nativeTokenizerRegistered'])
            self.assertEqual(result['chain'], workflow.chain)
            self.assertFalse(result['actualClaimCompletion'])
            names = [c[0] for c in calls]
            self.assertEqual([n for n in names if n.startswith('inventory-part-')],
                             ['inventory-part-0000-submit', 'inventory-part-0001-submit'])
            self.assertLess(names.index('inventory-part-0001-submit'), names.index('inventory-finish-submit'))
            self.assertLess(names.index('inventory-finish-submit'), names.index('tkr-prepare'))
            self.assertEqual([n for n in names if n.startswith('tkr-send-')],
                             ['tkr-send-open', 'tkr-send-part-0000-200', 'tkr-send-part-0001-201', 'tkr-send-finish-202'])
            sends = [i for i, name in enumerate(names) if name.startswith('tkr-send-')]
            self.assertEqual([calls[i + 1] for i in sends],
                             [('relay', (h(80), slot)) for slot in (None, 200, 201, 202)])

    def test_tokenizer_retry_requires_collected_open_and_rechecks_finished_inventory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            previous, finished, opened = [root / name / 'registration' for name in ('retry', 'finished', 'opened')]
            for path in (previous, finished, opened):
                path.mkdir(parents=True)
            for child, parent in ((previous, finished), (finished, opened)):
                (child.parent / 'warmup-resume.json').write_text(json.dumps(dict(source=str(parent.parent))))
            workflow = object.__new__(RegistrationWorkflow)
            workflow.config = dict(maximumChainBlocks=100)
            workflow.expected = dict(version=1, genesisHash=h(10), consensusParamsId='01' * 32)
            workflow.rpc = Rpc()
            def wait(read):
                value = read()
                if value is None:
                    raise TimeoutError('still retained')
                return value
            workflow.wait = wait
            (previous / 'input.json').write_text(json.dumps(workflow.config))
            (previous / '038-tkr-draft.stdout').write_text(json.dumps(dict(
                openId=h(22), finishUntilDaa=287, materialUntilDaa=307)))
            (previous / '039-tkr-send-open.stdout').write_text(json.dumps(dict(openId=h(22))))
            inventory = opened / 'inventory-open'
            inventory.mkdir()
            (inventory / 'offer.borsh').write_bytes(b'original offer')
            accepted = dict(expected=workflow.expected, acceptingBlock=h(8), acceptedDaa=250,
                            acceptedTransactionIds={'acceptedTransactionIds': [h(70)]})
            for label in ('setup-submit', 'inventory-finish-submit'):
                (finished / f'{label}-acceptance.json').write_text(json.dumps([accepted]))

            with patch('protected_registration_workflow.observe_transaction', return_value=accepted) as read, patch(
                    'protected_registration_workflow.chain_identity', return_value=(dict(virtualDaaScore=307), ('stable',))):
                with self.assertRaisesRegex(ValueError, 'was not expired'):
                    workflow.resume_tokenizer(previous)
                workflow.rpc.view['registration']['status'] = 'expired'
                with self.assertRaisesRegex(TimeoutError, 'still retained'):
                    workflow.resume_tokenizer(previous)
                workflow.rpc.found = False
                workflow.rpc.view['registration'] = None
                rows, source, retry_nonce = workflow.resume_tokenizer(previous)
                self.assertEqual((rows, source), ([accepted], inventory))
                self.assertNotEqual(retry_nonce, h(22))
                self.assertEqual(len(retry_nonce), 128)
                self.assertEqual(read.call_count, 6)

    def test_absence_is_pollable_but_accepted_disappearance_is_fatal(self):
        rpc = Rpc()
        old = rpc.observe()
        rpc.found = False
        rpc.view['registration'] = None
        with self.assertRaises(AcceptancePending):
            rpc.observe()
        with self.assertRaisesRegex(ValueError, 'disappeared'):
            rpc.observe(old)

    def test_wrong_identity_or_open_cannot_advance(self):
        for change in (
            lambda r: r.node.update(consensusParamsId=h(99)),
            lambda r: r.view.update(openId=h(99)),
            lambda r: r.view['registration'].update(openBodyHex='abce'),
            lambda r: r.view['registration'].update(status='expired'),
        ):
            rpc = Rpc()
            change(rpc)
            with self.assertRaises(ValueError):
                rpc.observe()

    def test_mixed_generation_or_parent_is_not_usable(self):
        rpc = Rpc()
        rpc.view['acceptedParent']['block'] = h(99)
        with self.assertRaises(ContextMoved):
            rpc.observe()
        rpc = Rpc()
        def change(r, method, _):
            if method == 'getPalwTokenizerRegistrationStatus':
                r.node['importSessionEpoch'] += 1
        rpc.mutate = change
        with self.assertRaises(ContextMoved):
            rpc.observe()

    def test_origin_quote_and_slots_are_immutable(self):
        for change in (
            lambda v: v['registration']['accepted'].update(daa=197),
            lambda v: v['registration'].update(chargeId=h(99)),
            lambda v: v['registration']['parts'][0].update(reservedDaa=201),
            lambda v: v['registration']['finishes'][1].update(reservedDaa=204),
        ):
            rpc = Rpc()
            old = rpc.observe()
            change(rpc.view)
            with self.assertRaises(ValueError):
                rpc.observe(old)

    def test_only_accepted_parts_advance_and_missed_slot_is_not_replaced(self):
        rpc = Rpc()
        self.assertEqual(next_control(rpc.observe()), ('part-0000.tkr1', 200))
        part = rpc.view['registration']['parts'][0]
        part.update(received=True, accepted=dict(block=h(30), daa=200))
        old = rpc.observe()
        self.assertEqual(next_control(old), ('part-0001.tkr1', 201))
        rpc.view['prospectiveDaa'] = 202
        with self.assertRaisesRegex(ValueError, 'missed'):
            next_control(rpc.view)
        rpc.view['prospectiveDaa'] = 200
        part.update(received=False, accepted=None)
        with self.assertRaisesRegex(ValueError, 'disappeared'):
            rpc.observe(old)

    def test_parts_follow_native_slot_order_even_when_indices_are_reversed(self):
        rpc = Rpc()
        first, second = rpc.view['registration']['parts']
        first['reservedDaa'], second['reservedDaa'] = 201, 200
        self.assertEqual(next_control(rpc.observe()), ('part-0001.tkr1', 200))
        second.update(received=True, accepted=dict(block=h(31), daa=200))
        self.assertEqual(next_control(rpc.observe()), ('part-0000.tkr1', 201))

    def test_finish_recovery_requires_all_parts_and_complete_native_record(self):
        rpc = Rpc()
        row = rpc.view['registration']
        for part in row['parts']:
            part.update(received=True, accepted=dict(block=h(30 + part['index']), daa=part['reservedDaa']))
        self.assertEqual(next_control(rpc.observe()), ('finish.tkr1', 202))
        row['finishes'][0]['attempted'] = True
        self.assertEqual(next_control(rpc.observe()), ('finish.tkr1', 203))
        row.update(status='complete', registrationId=h(40), terminal=dict(block=h(41), daa=203))
        with self.assertRaisesRegex(ValueError, 'outside'):
            rpc.observe()
        row['finishes'][1]['attempted'] = True
        self.assertIsNone(next_control(rpc.observe()))
        row['parts'][1].update(received=False, accepted=None)
        with self.assertRaisesRegex(ValueError, 'lacks accepted'):
            rpc.observe()

    def test_uncertain_command_is_logged_once_and_never_retried(self):
        with tempfile.TemporaryDirectory() as directory:
            workflow = object.__new__(RegistrationWorkflow)
            workflow.output = Path(directory)
            workflow.deadline = time.monotonic() + 10
            workflow.config = dict(commandTimeoutSeconds=5)
            workflow.rpc = Rpc()
            workflow.expected = workflow.rpc.expected
            workflow.sequence = 0
            workflow.progress = lambda: self.fail('unexpected retry')
            with patch('protected_registration_workflow.subprocess.run',
                       side_effect=subprocess.TimeoutExpired(['sender'], 5)) as command:
                with self.assertRaises(subprocess.TimeoutExpired):
                    workflow.run('send', ['sender'])
                self.assertEqual(command.call_count, 1)
            record = json.loads((workflow.output / '001-send.intent.json').read_text())
            self.assertFalse(record['retryAutomatically'])
            self.assertTrue(record['submissionMayBeUncertain'])
            self.assertFalse((workflow.output / '001-send.result.json').exists())


if __name__ == '__main__':
    unittest.main()
