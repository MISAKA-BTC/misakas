"""Actual HTTP/read-consumer controls; these are not node, bridge or market acceptance evidence."""
from contextlib import contextmanager
import copy
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
from pathlib import Path
import sys
import threading
import time
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc12_acceptance_observe as O
import rfc12_acceptance_judgement as J

SPEC = importlib.util.spec_from_file_location('rfc12_evm_market_driver',
    Path(__file__).resolve().parents[1] / 'misaka-rfc12-acceptance.py')
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)
HASH = '0x' + '11' * 32
HOLDER, LINE, SINK = '22' * 64, '33' * 64, '44' * 64
DAG = dict(sink=SINK, virtualDaaScore=100, pruningPointHash='55' * 64)
NODE = dict(importSessionAvailable=True, importSessionEpoch=1, importSessionPoint='',
    importSessionPhase='idle', pruningImportStable=True, genesisHash='66' * 64, consensusParamsId='77' * 32)


@contextmanager
def endpoint(reply):
    requests = []
    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(request)
            response = dict(jsonrpc='2.0', id=request['id'], **reply(request['method'], request['params']))
            body = json.dumps(response).encode()
            self.send_response(200)
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        def log_message(self, *args):
            pass
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=lambda: server.serve_forever(poll_interval=.02), daemon=True)
    thread.start()
    try:
        yield 'http://127.0.0.1:%d' % server.server_port, requests
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


class HashPinnedBalances(unittest.TestCase):
    def test_real_http_balance_requests_use_the_observed_canonical_hash(self):
        def reply(method, params):
            if method == 'eth_getBlockByNumber':
                return dict(result=dict(hash=HASH, number='0x2'))
            selector = dict(blockHash=HASH, requireCanonical=True)
            return dict(result='0x64' if params[1] == selector else '0xdead')
        accounts = ['0x' + '88' * 20, '0x' + '99' * 20]
        with endpoint(reply) as (url, requests):
            value = O.evm_value(url, accounts, 2)
        self.assertEqual(value['balances'], dict.fromkeys(accounts, 100))
        self.assertEqual(value['ethLatestHash'], O.norm(HASH))
        self.assertEqual([r['params'][1] for r in requests if r['method'] == 'eth_getBalance'],
            [dict(blockHash=HASH, requireCanonical=True)] * 2)
        self.assertEqual([r['method'] for r in requests],
            ['eth_getBlockByNumber', 'eth_getBalance', 'eth_getBalance', 'eth_getBlockByNumber'])

    def test_same_height_reorg_or_head_advance_is_not_a_coherent_balance_row(self):
        for changed in (dict(hash='0x' + 'aa' * 32, number='0x2'), dict(hash=HASH, number='0x3'), None):
            reads = 0
            def reply(method, params):
                nonlocal reads
                if method == 'eth_getBlockByNumber':
                    reads += 1
                    return dict(result=dict(hash=HASH, number='0x2') if reads == 1 else changed)
                return dict(result='0x64')
            with self.subTest(changed=changed), endpoint(reply) as (url, _):
                with self.assertRaises(O.ContextMoved):
                    O.evm_value(url, ['0x' + '88' * 20], 2)

    def test_actual_bridge_balance_set_refuses_different_native_sinks_with_the_same_eth_hash(self):
        def reply(method, params):
            return dict(result=dict(hash=HASH, number='0x2') if method == 'eth_getBlockByNumber' else '0x64')
        class Rpc:
            def __init__(self, role):
                self.role = role
            def call(self, method):
                if method == 'getPalwNodeStatus':
                    return copy.deepcopy(NODE)
                return dict(DAG, sink=SINK if self.role == 'A' else 'ff' * 64)
            def close(self):
                pass
        with endpoint(reply) as (url, _):
            run = driver.AcceptanceRun.__new__(driver.AcceptanceRun)
            run.bounds = dict(probeTimeoutSeconds=2)
            run.drill = SimpleNamespace(plan=dict(nodes={role: dict(eth=url) for role in ('A', 'B')}),
                latencies=[], rpc=Rpc)
            run.parallel = lambda roles, fn: [fn(role) for role in roles]
            with self.assertRaisesRegex(J.Pending, 'different sinks'):
                run.evm_set(['A', 'B'], 'bridge-balance', ['0x' + '88' * 20])

    def test_malformed_or_out_of_width_eth_quantities_are_not_balance_evidence(self):
        for field, invalid in (('balance', '-0x1'), ('balance', '0x00'), ('balance', hex(2**256)),
                               ('balance', True), ('number', hex(2**64)), ('number', '0x02'),
                               ('hash', '0x11')):
            def reply(method, params):
                if method == 'eth_getBlockByNumber':
                    return dict(result=dict(hash=invalid if field == 'hash' else HASH,
                        number=invalid if field == 'number' else '0x2'))
                return dict(result=invalid if field == 'balance' else '0x64')
            with self.subTest(field=field, invalid=invalid), endpoint(reply) as (url, _):
                with self.assertRaises(J.NotObservable):
                    O.evm_value(url, ['0x' + '88' * 20], 2)

    def test_actual_bridge_consumer_checks_all_balances_and_block_numbers(self):
        def reply(number, balance):
            def answer(method, params):
                return dict(result=dict(hash=HASH, number=number) if method == 'eth_getBlockByNumber' else balance)
            return answer
        for number, balance, error in (('0x3', '0x64', J.Pending), ('0x2', '0x65', J.Violation)):
            with endpoint(reply('0x2', '0x64')) as (url_a, _), endpoint(reply(number, balance)) as (url_b, _):
                run = driver.AcceptanceRun.__new__(driver.AcceptanceRun)
                run.bounds = dict(probeTimeoutSeconds=2)
                run.drill = SimpleNamespace(plan=dict(nodes=dict(A=dict(eth=url_a), B=dict(eth=url_b))),
                    latencies=[], rpc=lambda role: PositionRpc(lambda: position()))
                run.parallel = lambda roles, fn: [fn(role) for role in roles]
                with self.assertRaises(error):
                    run.evm_set(['A', 'B'], 'bridge-balance', ['0x' + '88' * 20])

    def test_hash_selector_refusal_does_not_fall_back_to_numeric_or_latest_state(self):
        def reply(method, params):
            if method == 'eth_getBlockByNumber':
                return dict(result=dict(hash=HASH, number='0x2'))
            return dict(error=dict(code=-32602, message='requireCanonical: block left the canonical chain'))
        with endpoint(reply) as (url, requests):
            with self.assertRaises(J.Pending):
                O.evm_value(url, ['0x' + '88' * 20], 2)
        self.assertEqual(len(requests), 2)


class PositionRpc:
    def __init__(self, answer):
        self.answer = answer
    def call(self, method, params=None):
        if method == 'getBlockDagInfo':
            return copy.deepcopy(DAG)
        if method == 'getPalwNodeStatus':
            return copy.deepcopy(NODE)
        if method == 'getPalwModelPositions':
            assert params == dict(holder=HOLDER)
            return copy.deepcopy(self.answer())
        raise AssertionError(method)
    def close(self):
        pass


def position(units=5):
    return dict(holder=HOLDER, tipHash=SINK, tipDaa=100, positions=[dict(lineId=LINE, units=units)])


class NativeMarketRead(unittest.TestCase):
    def test_stale_response_tip_is_retried_even_under_stable_outer_rpc_generation(self):
        for key, value in (('tipHash', 'ff' * 64), ('tipDaa', 99)):
            answer = dict(position(), **{key: value})
            with self.subTest(key=key), self.assertRaises(J.Pending):
                O.model_position_value(PositionRpc(lambda: answer), DAG, HOLDER, LINE)

    def test_missing_rows_wrong_holder_or_duplicate_rows_never_become_zero_or_a_fill(self):
        for answer in (dict(position(), positions=None), dict(position(), holder='ff' * 64),
                       dict(position(), positions=[dict(lineId=LINE, units=2), dict(lineId=LINE, units=3)])):
            with self.assertRaises(J.NotObservable):
                O.model_position_value(PositionRpc(lambda: answer), DAG, HOLDER, LINE)
        for bad in (True, 5.0, -1, 2**64):
            with self.subTest(units=bad), self.assertRaises(J.NotObservable):
                O.model_position_value(PositionRpc(lambda: position(bad)), DAG, HOLDER, LINE)
        empty = dict(position(), positions=[])
        self.assertEqual(O.model_position_value(PositionRpc(lambda: empty), DAG, HOLDER, LINE)['units'], 0)

    def run_market(self, drift=None):
        run = driver.AcceptanceRun.__new__(driver.AcceptanceRun)
        run.plan = dict(market=dict(enabled=True, holderId=HOLDER, lineId=LINE, openCommands=[],
            buyCommands=[['owned-buy']], sellCommands=[['owned-sell']], buyUnits=2, sellUnits=1))
        units, executed = {'A': 5, 'B': 5}, []
        def answer(role):
            value = position(units[role])
            if drift:
                drift(value, role)
            return value
        run.drill = SimpleNamespace(latencies=[], rpc=lambda role: PositionRpc(lambda: answer(role)))
        run.live = lambda roles: ['A', 'B']
        run.parallel = lambda roles, fn: [fn(role) for role in roles]
        run.wait = lambda predicate, label: predicate()
        run.rows = []
        run.judge = lambda box, label, fn, *args: fn(*args)
        def fixtures(name, commands):
            executed.append(name)
            change = 2 if name == 'MARKET_BUY' else -1 if name == 'MARKET_SELL' else 0
            for role in units:
                units[role] += change
        run.fixtures = fixtures
        return run, executed

    def test_actual_market_phase_uses_generation_checked_reads_before_and_after_each_fill(self):
        run, executed = self.run_market()
        run.phase_market()
        self.assertEqual(executed, ['MARKET_OPEN', 'MARKET_BUY', 'MARKET_SELL'])
        self.assertEqual([r['value']['units'] for r in run.rows], [5, 5, 7, 7, 6, 6])
        self.assertTrue(all(r['value']['tipHash'] == r['dag']['sink'] for r in run.rows))

    def test_actual_market_phase_cannot_trade_from_stale_or_disagreeing_baselines(self):
        for drift, error in ((lambda value, role: value.update(tipDaa=99), J.Pending),
                             (lambda value, role: value['positions'][0].update(units=6 if role == 'B' else 5), J.Violation)):
            run, executed = self.run_market(drift)
            with self.assertRaises(error):
                run.phase_market()
            self.assertEqual(executed, [])


BASE, B1, B2 = '10' * 32, '20' * 32, '30' * 32
T1, T2 = '40' * 32, '50' * 32
SENDER = '0x' + '88' * 20


def fee_history():
    blocks, receipts = {}, {}
    for number, own, parent, txid, gas, price in ((1, B1, BASE, T1, 2, 3), (2, B2, B1, T2, 3, 4)):
        blocks[own] = dict(hash='0x' + own, number=hex(number), parentHash='0x' + parent,
            transactions=[dict(hash='0x' + txid, **{'from': SENDER}, value='0x3')])
        receipts[txid] = dict(transactionHash='0x' + txid, blockHash='0x' + own, blockNumber=hex(number),
            transactionIndex='0x0', gasUsed=hex(gas), effectiveGasPrice=hex(price), status='0x1')
    return blocks, receipts


def history_reply(blocks, receipts, moved=False):
    checks = 0
    def reply(method, params):
        nonlocal checks
        if method == 'eth_getBlockByNumber':
            checks += 1
            value = copy.deepcopy(blocks[B2])
            if moved and checks > 1:
                value['hash'] = '0x' + 'ff' * 32
            return dict(result=value)
        if method == 'eth_getBlockByHash':
            return dict(result=blocks.get(O.norm(params[0])))
        if method == 'eth_getTransactionReceipt':
            return dict(result=receipts.get(O.norm(params[0])))
        raise AssertionError(method)
    return reply


class FrozenWithdrawalGas(unittest.TestCase):
    def read(self, url, **kwargs):
        return O.evm_spends(url, SENDER, 1, 2, 2, 2, parent_hash=BASE, last_hash=B2, **kwargs)

    def test_actual_hash_walk_and_bound_receipts_reconcile_in_forward_order(self):
        blocks, receipts = fee_history()
        with endpoint(history_reply(blocks, receipts)) as (url, requests):
            spends = self.read(url)
        self.assertEqual([s['hash'] for s in spends], [T1, T2])
        self.assertEqual([s['blockHash'] for s in spends], [B1, B2])
        self.assertEqual(sum(s['gasWei'] for s in spends), 18)
        self.assertEqual([r['params'][0] for r in requests if r['method'] == 'eth_getBlockByHash'], ['0x' + B2, '0x' + B1])
        self.assertEqual([r['params'] for r in requests if r['method'] == 'eth_getBlockByNumber'], [['0x2', False]] * 2)

    def test_adapter_full_body_error_does_not_become_an_empty_fee_interval(self):
        blocks, receipts = fee_history()
        base = history_reply(blocks, receipts)
        def reply(method, params):
            if method == 'eth_getBlockByHash' and O.norm(params[0]) == B1:
                return dict(error=dict(code=-32000, message='full transaction data for the requested block is unavailable'))
            return base(method, params)
        with endpoint(reply) as (url, _):
            with self.assertRaisesRegex(J.Pending, 'full transaction data'):
                self.read(url)

    def test_empty_interval_requires_the_exact_before_balance_hash(self):
        blocks, receipts = fee_history()
        with endpoint(history_reply(blocks, receipts)) as (url, requests):
            self.assertEqual(O.evm_spends(url, SENDER, 3, 2, 2, 2, parent_hash=B2, last_hash=B2), [])
            self.assertEqual(len(requests), 1)
            with self.assertRaises(O.ContextMoved):
                O.evm_spends(url, SENDER, 3, 2, 2, 2, parent_hash=BASE, last_hash=B2)
            count = len(requests)
            with self.assertRaises(J.NotObservable):
                O.evm_spends(url, SENDER, 4, 2, 2, 2, parent_hash=BASE, last_hash=B2)
            self.assertEqual(len(requests), count)

    def test_receipt_other_block_transaction_position_or_missing_body_is_not_zero_gas(self):
        mutations = [lambda r: r[T2].update(blockHash='0x' + B1),
            lambda r: r[T2].update(transactionHash='0x' + T1),
            lambda r: r[T2].update(blockNumber='0x1'),
            lambda r: r[T2].update(transactionIndex='0x1'),
            lambda r: r.pop(T2)]
        for mutate in mutations:
            blocks, receipts = fee_history()
            mutate(receipts)
            with endpoint(history_reply(blocks, receipts)) as (url, _):
                with self.assertRaises((O.ContextMoved, J.Pending)):
                    self.read(url)

    def test_walk_refuses_skips_cycles_missing_blocks_or_another_before_balance_parent(self):
        mutations = [lambda b: b[B1].update(number='0x0'),
            lambda b: b[B2].update(parentHash='0x' + B2),
            lambda b: b.pop(B1),
            lambda b: b[B1].update(parentHash='0x' + 'ff' * 32)]
        for mutate in mutations:
            blocks, receipts = fee_history()
            mutate(blocks)
            with endpoint(history_reply(blocks, receipts)) as (url, _):
                with self.assertRaises((J.NotObservable, J.Pending, O.ContextMoved)):
                    self.read(url)

    def test_endpoint_reorg_after_receipts_refuses_the_whole_scan(self):
        blocks, receipts = fee_history()
        with endpoint(history_reply(blocks, receipts, moved=True)) as (url, _):
            with self.assertRaises(O.ContextMoved):
                self.read(url)

    def test_span_limit_and_remaining_deadline_bound_the_actual_http_scan(self):
        blocks, receipts = fee_history()
        with endpoint(history_reply(blocks, receipts)) as (url, requests):
            with self.assertRaises(J.NotObservable):
                O.evm_spends(url, SENDER, 1, 2, 2, 1, parent_hash=BASE, last_hash=B2)
            self.assertEqual(requests, [])
            calls = 0
            def remaining():
                nonlocal calls
                calls += 1
                if calls >= 3:
                    raise driver.WaitTimeout('predeclared control deadline')
                return 2
            with self.assertRaises(driver.WaitTimeout):
                self.read(url, remaining=remaining)
            self.assertEqual(len(requests), 1, 'deadline must stop before the first hash-block body')

    def test_receipt_quantities_are_exact_and_full_bodies_cannot_disappear(self):
        mutations = [lambda b, r: b[B1].update(transactions=None),
            lambda b, r: b[B1]['transactions'].append(copy.deepcopy(b[B1]['transactions'][0])),
            lambda b, r: r[T1].update(gasUsed='-0x1'),
            lambda b, r: r[T1].update(effectiveGasPrice=hex(2**256)),
            lambda b, r: r[T1].update(status='0x2')]
        for mutate in mutations:
            blocks, receipts = fee_history()
            mutate(blocks, receipts)
            with endpoint(history_reply(blocks, receipts)) as (url, _):
                with self.assertRaises(J.NotObservable):
                    self.read(url)

    def test_real_reconciliation_caller_requires_the_join_balance_native_generation(self):
        blocks, receipts = fee_history()
        reference = dict(role='JOIN', dag=copy.deepcopy(DAG), node=copy.deepcopy(NODE),
            value=dict(ethLatestNumber=2, ethLatestHash=B2, balances={SENDER: 42}))
        with endpoint(history_reply(blocks, receipts)) as (url, requests):
            run = driver.AcceptanceRun.__new__(driver.AcceptanceRun)
            run.bounds = dict(probeTimeoutSeconds=2)
            run.plan = dict(scan=dict(maximumScannedEvmBlocks=2))
            run.remaining = lambda: 5
            run.evm_set = lambda *args: [reference]
            run.drill = SimpleNamespace(plan=dict(nodes=dict(JOIN=dict(eth=url))), latencies=[],
                rpc=lambda role: PositionRpc(lambda: position()))
            balances, gas = run.evm_reconciliation(['JOIN'], 'withdrawal', SENDER, 1, BASE, time.monotonic() + 5)
            self.assertEqual(balances, [reference])
            self.assertEqual(sum(s['gasWei'] for s in gas['value']['spends']), 18)
            self.assertEqual(O.context(gas['dag'], gas['node']), O.context(reference['dag'], reference['node']))
            reference['dag']['virtualDaaScore'] = 99
            count = len(requests)
            with self.assertRaises(O.ContextMoved):
                run.evm_reconciliation(['JOIN'], 'withdrawal', SENDER, 1, BASE, time.monotonic() + 5)
            self.assertEqual(len(requests), count, 'native generation mismatch must stop before gas RPC reads')
            reference['dag']['virtualDaaScore'] = 100
            with self.assertRaises(driver.WaitTimeout):
                run.evm_reconciliation(['JOIN'], 'withdrawal', SENDER, 1, BASE, time.monotonic() - 1)
            self.assertEqual(len(requests), count, 'expired phase budget must stop before gas RPC reads')


if __name__ == '__main__':
    unittest.main()
