"""Reader adversarial tests with fake RPC replies, not native-chain evidence."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from protected_claim_observation import ContextMoved, expectation_from_studio, observe, observe_claim_progress, receipt_from_producer_log
from protected_claim_observation import validate_receipt_observation
from protected_private_registration_run import receipt_peer_view, receipt_window_elapsed, require_gate_c_status, retain_studio_claim, studio_admission


def h(n):
    return format(n, '0128x')


class Rpc:
    def __init__(self):
        self.expected = dict(version=1, genesisHash=h(1), consensusParamsId=h(2)[64:],
                             networkDomain=h(3), claim=h(4), transactionId=h(5), executionRoot=h(6))
        self.node = dict(genesisHash=h(1), consensusParamsId=h(2)[64:], importSessionAvailable=True,
                         importSessionEpoch=0, importSessionPoint=h(1), importSessionPhase='idle',
                         pruningImportStable=True)
        self.dag = dict(sink=h(10), pruningPointHash=h(1), virtualDaaScore=200)
        self.read = dict(version=1, claim=h(4), executionRoot=h(6), original=h(7),
                         carrierTransaction=h(5), carrierBlock=h(8), acceptingBlock=h(9),
                         acceptedTxOrdinal=0, eventOrdinal=0, acceptedDaa=100, acceptedBlueScore=100,
                         observedTip=h(10), observedTipDaa=200,
                         fullReplayApprovalRecorded=True, publicRefutationRecorded=False)
        self.detail = dict(version=1, kind='palwClaimDetail', gateCGoverns=True,
                           claim=dict(claimId=h(4), phase='final', phaseDaa=150, executorBond=h(20),
                                      seats=[h(21)], securityCertification='certified'),
                           protectedOriginal=self.read, tipBlock=h(10), tipDaa=200)
        self.blocks = {}
        for n in (8, 9, 10):
            self.blocks[h(n)] = dict(header=dict(hash=h(n), daaScore=100 if n == 9 else 200, blueScore=100, powAlgoId=7),
                verboseData=dict(hash=h(n), isHeaderOnly=False, isChainBlock=n != 8,
                    selectedParentHash=h(11), mergeSetBluesHashes=[h(8)] if n == 9 else [], mergeSetRedsHashes=[]),
                transactions=[dict(verboseData=dict(transactionId=h(5)))])
        self.entry = dict(acceptingBlockHash=h(9), acceptedTransactionIds=[h(5)])
        self.calls = []
        self.mutate = None

    def call(self, method, request=None):
        self.calls.append(method)
        if self.mutate:
            self.mutate(self, method, request)
        if method == 'getBlockDagInfo':
            return copy.deepcopy(self.dag)
        if method == 'getPalwNodeStatus':
            return copy.deepcopy(self.node)
        if method == 'getPalwClaimDetail':
            return dict(available=True, found=True, viewJson=json.dumps(self.detail))
        if method == 'getBlock':
            return dict(block=copy.deepcopy(self.blocks[request['hash']]))
        if method == 'getVirtualChainFromBlock':
            return dict(removedChainBlockHashes=[], addedChainBlockHashes=[h(9), h(10)] if request['startHash'] == h(11) else [],
                        acceptedTransactionIds=[copy.deepcopy(self.entry)])
        raise AssertionError(method)


class ObservationTests(unittest.TestCase):
    def test_resume_retries_only_the_existing_body_at_the_lagging_peer(self):
        from unittest.mock import Mock
        from protected_private_registration_run import retry_retained_sink
        clients = [Mock() for _ in range(3)]
        block = dict(header=dict(hash=h(8)), verboseData=dict(hash=h(8), isChainBlock=True, isHeaderOnly=False))
        clients[0].call.return_value = dict(block=block)
        clients[1].call.return_value = dict(report=dict(type='success'))
        views = [dict(sink=h(8), virtualDaaScore=8, tipHashes=[h(8)]),
                 dict(sink=h(7), virtualDaaScore=7, tipHashes=[h(8)]),
                 dict(sink=h(7), virtualDaaScore=7, tipHashes=[h(7)])]
        retry_retained_sink(clients, views)
        clients[0].call.assert_called_once_with('getBlock', dict(hash=h(8), includeTransactions=True))
        clients[1].call.assert_called_once_with('submitBlock', dict(block=block, allowNonDAABlocks=False))
        clients[2].call.assert_not_called()

    def test_restart_retries_the_single_shared_direct_child_through_native_validation(self):
        from unittest.mock import Mock
        from protected_private_registration_run import retry_shared_child
        clients = [Mock() for _ in range(3)]
        view = dict(sink=h(7), virtualDaaScore=7, tipHashes=[h(8)])
        child = dict(header=dict(hash=h(8)), verboseData=dict(hash=h(8), selectedParentHash=h(7),
                                                             isChainBlock=False, isHeaderOnly=False))
        accepted = dict(report=dict(type='success'))
        for client in clients:
            client.call.return_value = accepted
        clients[0].call.side_effect = [dict(block=child), accepted]
        self.assertFalse(retry_shared_child(clients, [view, view, dict(view, tipHashes=[h(9)])]))
        self.assertFalse(retry_shared_child(clients, [dict(view, tipHashes=[h(8), h(9)])] * 3))
        self.assertTrue(retry_shared_child(clients, [view] * 3))
        for client in clients:
            client.call.assert_called_with('submitBlock', dict(block=child, allowNonDAABlocks=False))

    def test_resume_keeps_original_carrier_and_rechecks_each_peer_without_submission(self):
        from protected_private_registration_run import resume_admitted_studio
        clients = [Rpc(), Rpc()]
        rpc = clients[0]
        chain = {k: rpc.expected[k] for k in ('genesisHash', 'consensusParamsId', 'networkDomain')}
        registration = dict(chain=chain)
        run = dict(fp_job_id=h(20), output_root=h(21))
        job = dict(object='misaka.request', job_id=h(20), output_root=h(21), claim_id=h(4), submission_txid=h(5))
        rail = dict(schema='misaka.palw.fp-rail-tx.v1', fp_claim_id=h(4), submitted=h(5),
                    protected_execution_root=h(6), protected_funding_sompi=100)
        original = observe(rpc, rpc.expected)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            old, new = root / 'old', root / 'new'
            (old / 'studio').mkdir(parents=True)
            new.mkdir()
            for name, value in [('studio-run', run), ('studio-evidence', dict(studioRun=run, studioJob=job, rail=rail, chain=chain)),
                                ('original', original)]:
                (old / 'studio' / (name + '.json')).write_text(json.dumps(value))
            args = (dict(studio=dict(timeoutSeconds=3)), new, old, registration, clients,
                    lambda: self.fail('unexpected heartbeat or submit'), new / 'clock', h(20))
            with patch('protected_private_registration_run.observe_admitted_studio', return_value='resumed') as continuation:
                self.assertEqual(resume_admitted_studio(*args), 'resumed')
                self.assertEqual(continuation.call_args.args[4], [new / 'clock', old / 'clock'])
                self.assertEqual(continuation.call_args.args[-2], original)
                self.assertTrue(all('getPalwClaimDetail' in client.calls for client in clients))
                clients[1].read['acceptingBlock'] = h(99)
                with self.assertRaisesRegex(ValueError, 'original inclusion changed'):
                    resume_admitted_studio(*args)
                continuation.assert_called_once()

    def test_only_the_executor_receives_its_matching_studio_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outbox, executor = root / 'studio', root / 'executor'
            (outbox / 'retention').mkdir(parents=True)
            executor.mkdir()
            claim = h(4)
            rail = {}
            for field, suffix in [('material_file', '.material'), ('answer_file', '.answer')]:
                source = outbox / 'retention' / (claim + suffix)
                source.write_bytes(field.encode())
                rail[field] = str(source)
            with patch.object(Rpc, 'call', return_value={'palwRetentionDir': str(executor)}):
                first = retain_studio_claim(Rpc(), rail, claim, outbox)
                self.assertEqual(retain_studio_claim(Rpc(), rail, claim, outbox), first)
                self.assertEqual((executor / (claim + '.material')).read_bytes(), b'material_file')
                (executor / (claim + '.answer')).write_bytes(b'prior')
                with self.assertRaisesRegex(ValueError, 'bytes differ'):
                    retain_studio_claim(Rpc(), rail, claim, outbox)
                self.assertEqual((executor / (claim + '.answer')).read_bytes(), b'prior')
                with self.assertRaisesRegex(ValueError, 'unexpected Studio retention'):
                    retain_studio_claim(Rpc(), rail, h(6), outbox)

    def test_studio_carrier_pending_dropped_and_admitted(self):
        rpc = Rpc()
        real_call = rpc.call

        def absent(method, request=None):
            if method == 'getPalwClaimDetail':
                return dict(available=True, found=False, viewJson='')
            return real_call(method, request)

        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory)
            with patch.object(rpc, 'call', side_effect=absent):
                self.assertIsNone(studio_admission(rpc, rpc.expected, rpc.dag['sink'], data))
                self.assertEqual(list(data.iterdir()), [])
                with self.assertRaisesRegex(ValueError, 'but claim .* is absent'):
                    studio_admission(rpc, rpc.expected, h(11), data)
            recorded = json.loads((data / 'carrier-accepted-without-claim.json').read_text())
            self.assertEqual(recorded['carrier']['acceptingBlock'], h(9))
            self.assertFalse(recorded['claimDetail']['found'])
            self.assertEqual(studio_admission(rpc, rpc.expected, h(11), data)['origin']['claim'], h(4))

    def test_final_is_insufficient_for_gate_c_milestone(self):
        detail = dict(gateCGoverns=True, claim=dict(phase='final', securityCertification='pending'))
        self.assertFalse(require_gate_c_status(detail))
        detail['claim']['securityCertification'] = 'expired'
        with self.assertRaisesRegex(ValueError, 'expired'):
            require_gate_c_status(detail)
        detail['claim']['securityCertification'] = 'certified'
        self.assertTrue(require_gate_c_status(detail))
        detail['gateCGoverns'] = False
        with self.assertRaisesRegex(ValueError, 'outside Gate C'):
            require_gate_c_status(detail)
        rpc = Rpc()
        rpc.detail['claim']['securityCertification'] = 'pending'
        with self.assertRaisesRegex(ValueError, 'before Gate C'):
            observe(rpc, rpc.expected, receipt=h(10))

    def test_receipt_window_elapsed_uses_original_acceptance_and_inclusive_hard_deadline(self):
        rpc = Rpc()
        original = observe(rpc, rpc.expected)
        self.assertFalse(receipt_window_elapsed(original, dict(tipDaa=10_100)))
        self.assertTrue(receipt_window_elapsed(original, dict(tipDaa=10_101)))

    def test_only_competing_native_capacity_is_retried_without_advancing_the_claim(self):
        from unittest.mock import Mock, patch
        from protected_claim_observation import ClaimReadBusy, read_claim_detail
        from protected_private_registration_run import until_claim_observation
        import time

        def failure(held, requested, limit):
            return RuntimeError(f'getPalwClaimDetail: local resource unavailable: typed requests exceed logical capacity '
                                f'(RAM bytes: outstanding {held}, request {requested}, limit {limit}; '
                                'DB bytes: outstanding 0, request 0, limit 100)')

        rpc = Mock()
        rpc.call.side_effect = [failure(6, 5, 9), {'available': True}]
        step = Mock()
        with patch('protected_private_registration_run.time.sleep'):
            result = until_claim_observation('read', lambda: read_claim_detail(rpc, h(4)), step=step,
                                            timeout=2, deadline=time.monotonic() + 2, check_alive=None)
        self.assertEqual(result, {'available': True})
        step.assert_not_called()
        for error in [failure(4, 12, 9), RuntimeError('unavailable: broken checkpoint')]:
            rpc.call.side_effect = error
            with self.assertRaises(RuntimeError) as raised:
                read_claim_detail(rpc, h(4))
            self.assertIs(raised.exception, error)
            self.assertNotIsInstance(raised.exception, ClaimReadBusy)

    def test_progress_reads_current_state_without_scanning_notice_history(self):
        rpc = Rpc()
        original = observe(rpc, rpc.expected)
        rpc.calls.clear()
        rpc.dag['virtualDaaScore'] = rpc.detail['tipDaa'] = rpc.read['observedTipDaa'] = 3300
        self.assertEqual(observe_claim_progress(rpc, rpc.expected, original)['tipDaa'], 3300)
        self.assertNotIn('getVirtualChainFromBlock', rpc.calls)
        self.assertNotIn('getBlock', rpc.calls)
        rpc.read['acceptingBlock'] = h(24)
        with self.assertRaisesRegex(ValueError, 'original inclusion changed'):
            observe_claim_progress(rpc, rpc.expected, original)

    def test_second_peer_must_confirm_same_final_receipt_and_consumption(self):
        rpc = Rpc()
        approval = dict(bond=h(21), carrierTransaction=h(22), acceptingBlock=h(9), carrierBlock=h(8),
                        acceptedTxOrdinal=1, acceptedDaa=100)
        rpc.detail['protectedReplayApprovals'] = [approval]
        rpc.entry['acceptedTransactionIds'].append(h(22))
        rpc.blocks[h(8)]['transactions'].append(dict(verboseData=dict(transactionId=h(22))))
        rpc.detail['gateCGoverns'] = True
        rpc.detail['claim'].update(quantaSpent=1, securityCertification='certified')
        rpc.detail['receiptQuanta'] = dict(total=3, spentCount=1, spentIndices=[1], spentIndicesComplete=True)
        producer = observe(rpc, rpc.expected, receipt=h(10), panel_approval=h(22))
        rpc.node['importSessionEpoch'] += 1
        second = receipt_peer_view(rpc, rpc.expected, producer, h(10), 1)
        self.assertNotEqual(producer['context'], second['context'])
        self.assertEqual(second['receiptBlock']['header']['hash'], h(10))
        rpc.detail['receiptQuanta']['spentIndices'] = [2]
        with self.assertRaisesRegex(ValueError, 'unique consumed quantum'):
            receipt_peer_view(rpc, rpc.expected, producer, h(10), 1)
        rpc.dag['virtualDaaScore'] -= 1
        with self.assertRaises(ContextMoved):
            receipt_peer_view(rpc, rpc.expected, producer, h(10), 1)

    def test_params_id_has_native_width(self):
        rpc = Rpc()
        self.assertEqual(len(observe(rpc, rpc.expected)['context'][4]), 64)
        rpc.expected['consensusParamsId'] = rpc.node['consensusParamsId'] = h(2)
        with self.assertRaisesRegex(ValueError, '32-byte consensus params ID'):
            observe(rpc, rpc.expected)

    def test_original_blue_score_matches_the_selected_header(self):
        rpc = Rpc()
        rpc.blocks[h(9)]['header']['blueScore'] += 1
        with self.assertRaisesRegex(ValueError, 'admission blue score'):
            observe(rpc, rpc.expected)

    def test_studio_handoff_rejects_a_different_job_or_carrier(self):
        rpc = Rpc()
        run = dict(fp_job_id=h(20), output_root=h(21))
        job = dict(object='misaka.request', job_id=h(20), output_root=h(21),
                   claim_id=h(4), submission_txid=h(5))
        rail = dict(schema='misaka.palw.fp-rail-tx.v1', fp_claim_id=h(4), submitted=h(5),
                    protected_execution_root=h(6), protected_funding_sompi=100)
        expected = expectation_from_studio(run, job, rail, rpc.expected)
        self.assertEqual(observe(rpc, expected)['origin']['acceptingBlock'], h(9))
        for doc, field, value in [(job, 'job_id', h(22)), (job, 'output_root', h(22)),
                                  (rail, 'submitted', h(22)), (rail, 'fp_claim_id', h(22)),
                                  (rail, 'protected_funding_sompi', None)]:
            with self.subTest(field=field):
                original = doc[field]
                doc[field] = value
                try:
                    with self.assertRaises(ValueError):
                        expectation_from_studio(run, job, rail, rpc.expected)
                finally:
                    doc[field] = original

    def test_studio_main_queue_requires_the_same_committed_job_roots_and_chain(self):
        chain = Rpc().expected
        queue = dict(status='committed', fp_job_id=h(20), claim_id=h(4))
        receipt = dict(schema='misaka.palw.output-receipt.v1', job_id=h(20), claim_id=h(4),
                       output_root=h(21), execution_root=h(6), network_domain=chain['networkDomain'])
        artifact = dict(schema='misaka.palw.fp-v3-gateway-artifact.v1', committed=True, fp_job_id=h(20),
                        fp_claim_id=h(4), output_root=h(21), receipt=receipt)
        rail = dict(schema='misaka.palw.fp-rail-tx.v1', fp_claim_id=h(4), submitted=h(5),
                    protected_execution_root=h(6), protected_funding_sompi=100)
        self.assertEqual(expectation_from_studio(queue, artifact, rail, chain)['transactionId'], h(5))
        for record, field, wrong in [(queue, 'status', 'queued'), (artifact, 'committed', False),
                                     (receipt, 'job_id', h(22)), (queue, 'claim_id', h(22)),
                                     (receipt, 'output_root', h(22)), (rail, 'protected_execution_root', h(22)),
                                     (receipt, 'network_domain', h(22)), (rail, 'submitted', None)]:
            with self.subTest(field=field):
                old = record[field]
                record[field] = wrong
                try:
                    with self.assertRaises(ValueError):
                        expectation_from_studio(queue, artifact, rail, chain)
                finally:
                    record[field] = old

    def test_panel_txid_requires_the_matching_stored_approval(self):
        rpc = Rpc()
        rpc.detail['protectedReplayApprovals'] = [dict(
            bond=h(21), carrierTransaction=h(22), acceptingBlock=h(9), carrierBlock=h(8),
            acceptedTxOrdinal=1, acceptedDaa=100)]
        rpc.entry['acceptedTransactionIds'] += [h(22), h(23)]
        rpc.blocks[h(8)]['transactions'].append(dict(verboseData=dict(transactionId=h(22))))
        result = observe(rpc, rpc.expected, panel_approval=h(22))
        self.assertEqual(result['panelApprovalAcceptance']['carrierTransaction'], h(22))
        with self.assertRaisesRegex(ValueError, 'stored approval'):
            observe(rpc, rpc.expected, panel_approval=h(23))
        rpc.detail['protectedReplayApprovals'][0]['acceptedTxOrdinal'] = 2
        with self.assertRaisesRegex(ValueError, 'ordinal'):
            observe(rpc, rpc.expected, panel_approval=h(22))
        rpc.entry['acceptedTransactionIds'] = [h(22), h(5)]
        rpc.read['acceptedTxOrdinal'] = 1
        rpc.detail['protectedReplayApprovals'][0]['acceptedTxOrdinal'] = 0
        with self.assertRaisesRegex(ValueError, 'predates original admission in the same block'):
            observe(rpc, rpc.expected, panel_approval=h(22))

    def test_independent_panel_log_matches_native_approval_and_excludes_executor(self):
        rpc = Rpc()
        rpc.detail['protectedReplayApprovals'] = [
            dict(bond=h(20), carrierTransaction=h(22), acceptingBlock=h(9), carrierBlock=h(8),
                 acceptedTxOrdinal=1, acceptedDaa=100),
            dict(bond=h(21), carrierTransaction=h(23), acceptingBlock=h(9), carrierBlock=h(8),
                 acceptedTxOrdinal=2, acceptedDaa=100),
        ]
        rpc.entry['acceptedTransactionIds'] += [h(22), h(23)]
        rpc.blocks[h(8)]['transactions'] += [
            dict(verboseData=dict(transactionId=h(22))), dict(verboseData=dict(transactionId=h(23))),
        ]
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'peer.log'
            prefix = f'submitted a full-replay approval claim={h(4)} original={h(7)} accepting_block={h(9)} txid='
            log.write_text(prefix + h(22) + '\n')
            with self.assertRaises(ContextMoved):
                observe(rpc, rpc.expected, panel_logs=[log], excluded_panel_bond=h(20))
            log.write_text(prefix + h(22) + '\n' + prefix + h(23) + '\n')
            result = observe(rpc, rpc.expected, panel_logs=[log], excluded_panel_bond=h(20))
            self.assertEqual(result['panelApprovalAcceptance']['carrierTransaction'], h(23))
            # A retransmission can pay its fee on-chain while only one
            # carrier supplies the stored native approval.
            rpc.detail['protectedReplayApprovals'] = rpc.detail['protectedReplayApprovals'][1:]
            via_cli = observe(rpc, rpc.expected, panel_log=log)
            self.assertEqual(via_cli['panelApprovalAcceptance'], result['panelApprovalAcceptance'])
            rpc.detail['claim']['seats'] = [h(24)]
            with self.assertRaisesRegex(ValueError, 'independently selected'):
                observe(rpc, rpc.expected, panel_approval=h(23))

    def test_receipt_log_skips_side_dag_candidate(self):
        rpc = Rpc()
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'producer.log'
            log.write_text(''.join(f'PFS3 submitted block={h(n)} claim={h(4)} quantum=2 parent=...\n'
                                   for n in (8, 10)))
            self.assertEqual(receipt_from_producer_log(log, h(4), rpc=rpc, spent_indices=[2]), (h(10), 2))
            self.assertIsNone(receipt_from_producer_log(log, h(4), rpc=rpc, spent_indices=[3]))
            resumed_log = log.with_name('resumed.log')
            resumed_log.write_text('')
            self.assertEqual(receipt_from_producer_log([resumed_log, log], h(4), rpc=rpc, spent_indices=[2]), (h(10), 2))

    def test_receipt_must_follow_admission_on_the_observed_selected_chain(self):
        rpc = Rpc()
        with self.assertRaisesRegex(ValueError, 'does not follow its prerequisite'):
            observe(rpc, rpc.expected, receipt=h(9))
        rpc.blocks[h(8)]['verboseData']['isChainBlock'] = True
        with self.assertRaisesRegex(ValueError, 'outside the admission-to-tip history'):
            observe(rpc, rpc.expected, receipt=h(8))
        self.assertEqual(observe(rpc, rpc.expected, receipt=h(10))['receiptBlock']['header']['hash'], h(10))
        rpc.detail['claim']['phaseDaa'] = 201
        with self.assertRaisesRegex(ValueError, 'receipt DAA'):
            observe(rpc, rpc.expected, receipt=h(10))

    def test_receipt_log_rejects_two_selected_candidates_for_one_quantum(self):
        rpc = Rpc()
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'producer.log'
            log.write_text(''.join(f'PFS3 submitted block={h(n)} claim={h(4)} quantum=2 parent=...\n'
                                   for n in (9, 10)))
            with self.assertRaisesRegex(ValueError, 'multiple selected receipt blocks'):
                receipt_from_producer_log(log, h(4), rpc=rpc, spent_indices=[2])

    def test_receipt_audit_requires_the_native_checks_even_with_matching_ids(self):
        rpc = Rpc()
        observed = observe(rpc, rpc.expected, receipt=h(10))
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'observation.json'
            output.write_text(json.dumps(observed))
            validator = output.with_name('validator')
            validator.write_text('fixture validator')
            report = dict(version=1, kind='protectedReceiptObservationAudit', claim=h(4), block=h(10),
                          quantumIndex=2, headerHashChecked=True, nativeReceiptEnvelopeValidated=False,
                          spentIndexObserved=True)
            def write_report(command, **_):
                Path(command[-1]).write_text(json.dumps(report))
            with patch('protected_claim_observation.subprocess.run', side_effect=write_report):
                with self.assertRaisesRegex(ValueError, 'did not confirm'):
                    validate_receipt_observation(rpc, output, observed, rpc.expected, h(10), validator, 2)
                report['nativeReceiptEnvelopeValidated'] = True
                self.assertEqual(validate_receipt_observation(
                    rpc, output, observed, rpc.expected, h(10), validator, 2), report)

    def test_reads_same_claim_without_declaring_completion(self):
        rpc = Rpc()
        first = observe(rpc, rpc.expected)
        second = observe(rpc, rpc.expected, previous=first, receipt=h(10))
        self.assertEqual(second['origin'], first['origin'])
        self.assertEqual(second['receiptBlock']['header']['hash'], h(10))
        self.assertFalse(second['actualClaimCompletion'])
        self.assertFalse(second['nativeReceiptEnvelopeValidated'])
        self.assertFalse(any('submit' in name.lower() for name in rpc.calls))

    def test_another_origin_cannot_continue_the_claim(self):
        rpc = Rpc()
        first = observe(rpc, rpc.expected)
        rpc.read['eventOrdinal'] += 1
        with self.assertRaisesRegex(ValueError, 'inclusion changed'):
            observe(rpc, rpc.expected, previous=first)

    def test_substituted_identity_and_unstable_import_rejected(self):
        for key in ('claim', 'executionRoot', 'carrierTransaction'):
            rpc = Rpc()
            rpc.read[key] = h(90)
            with self.assertRaises(ValueError):
                observe(rpc, rpc.expected)
        rpc = Rpc()
        rpc.node['pruningImportStable'] = False
        with self.assertRaisesRegex(ValueError, 'import is not stable'):
            observe(rpc, rpc.expected)

    def test_changed_generation_is_not_combined(self):
        rpc = Rpc()
        def move(rpc, method, request):
            if method == 'getBlock':
                rpc.node['importSessionEpoch'] += 1
        rpc.mutate = move
        with self.assertRaises(ContextMoved):
            observe(rpc, rpc.expected)

    def test_admission_txid_at_the_wrong_ordinal_is_rejected(self):
        rpc = Rpc()
        rpc.entry['acceptedTransactionIds'].insert(0, h(50))
        with self.assertRaisesRegex(ValueError, 'recorded ordinal'):
            observe(rpc, rpc.expected)

    def test_tx_body_without_actual_acceptance_is_rejected(self):
        rpc = Rpc()
        rpc.entry['acceptedTransactionIds'] = []
        with self.assertRaisesRegex(ValueError, 'not accepted'):
            observe(rpc, rpc.expected)
        rpc = Rpc()
        rpc.blocks[h(8)]['transactions'] = []
        with self.assertRaisesRegex(ValueError, 'carrier body unavailable'):
            observe(rpc, rpc.expected)

    def test_header_only_or_unselected_receipt_is_rejected(self):
        for field, value in [('isHeaderOnly', True), ('isChainBlock', False)]:
            rpc = Rpc()
            rpc.blocks[h(10)]['verboseData'][field] = value
            with self.assertRaises(ValueError):
                observe(rpc, rpc.expected, receipt=h(10))

    def test_selected_chain_is_paged_to_the_bracketed_tip(self):
        rpc = Rpc()
        call = rpc.call
        def paged(method, request=None):
            if method != 'getVirtualChainFromBlock':
                return call(method, request)
            start = request['startHash']
            if start == h(11):
                return dict(removedChainBlockHashes=[], addedChainBlockHashes=[h(9)],
                            acceptedTransactionIds=[rpc.entry])
            self.assertEqual(start, h(9))
            return dict(removedChainBlockHashes=[], addedChainBlockHashes=[h(10)], acceptedTransactionIds=[])
        rpc.call = paged
        self.assertEqual(observe(rpc, rpc.expected)['origin']['acceptingBlock'], h(9))
        with self.assertRaisesRegex(ValueError, 'scan bound'):
            observe(rpc, rpc.expected, maximum_chain_blocks=1)

    def test_reorg_and_scan_bound_are_not_success(self):
        rpc = Rpc()
        call = rpc.call
        def reorg(method, request=None):
            value = call(method, request)
            if method == 'getVirtualChainFromBlock':
                value['removedChainBlockHashes'] = [h(11)]
            return value
        rpc.call = reorg
        with self.assertRaises(ContextMoved):
            observe(rpc, rpc.expected)
        rpc = Rpc()
        with self.assertRaisesRegex(ValueError, 'scan bound'):
            observe(rpc, rpc.expected, maximum_chain_blocks=1)


if __name__ == '__main__':
    unittest.main()
