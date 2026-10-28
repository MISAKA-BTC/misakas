#!/usr/bin/env python3
"""RFC-0018 real-process drivers: three scenarios on owned local nodes, from a plan pinned BEFORE launch.

  siblings   (§10 T-1) three models, three miners, separate Panels, sibling attempts from the SAME parents:
             every sibling retained and merged (selected parent / blue / red), every claim recorded with its own
             bond and class and carrying block, bound to its own Panel, paid to its own miner.
  partition  (§10 T-3) partition, reconnect and a fresh IBD node, with a different per-link delivery delay on every
             link (so the blocks arrive in a different order at every node): one sink, one GHOSTDAG reading of the
             merged blocks, one adopted-claim set, one PALW economic state and one fork-choice standing everywhere.
  attack     (§10 T-0, the mandatory top-level acceptance) the "300M closed-model / self-Panel" case on a LOCAL CALIBRATION devnet
             whose ruleset arms Gate C and rule E v2: an adversary group runs a model nobody else can obtain, staffs its own Panels
             and splits its capital into minimum bonds; honest groups run open models. PASS CRITERION (recorded, never self-labelled):
             no illegitimate consensus finality — reported SEPARATELY: safe-frontier movement, safe weight, settlement depth,
             rule-E participation share, settled anchors, reward outflow to adversary payouts — across a reorg (partition and
             rejoin), a restart (kill/start on the same app dir), pruning progress and a fresh IBD node.
  retired    (§10 T-5) the same partition/IBD drill on a LOCAL CALIBRATION devnet whose ruleset arms DNS retirement,
             rule E, rule E participation v2 and Gate C (a calibration genesis — never a public preset). Additionally:
             no uncertified claim contributes participation, weight, frontier or settled-anchor credit.

Ground rules (the RFC-12 drill's, kept): every role is its own process with a fresh owned app dir; only ordinary
P2P bytes cross driver-owned loopback links (`--nodnsseed --outpeers=0`, no `--connect`/`--addpeer`); miners are
plan-pinned fixture argv lists (finite, owned children); every verdict is read from public wRPC at a stable sink.
This driver ARMS NOTHING: a binary/ruleset that does not expose the scenario's fences yields BLOCKED, which is the
expected outcome on every shipped preset today. Its output is an observation record, not an approval; the status
for a run whose every check held is OBSERVED_CONSISTENT.

Plan (JSON): {
  "nodes": {"<ROLE>": {"argv": [...], "wrpc": "127.0.0.1:P", "p2p": "127.0.0.1:Q"}, ...},
  "miners": {"<NAME>": {"node": "<ROLE>", "model": "<class id hex>", "bond": "<txid:index>", "payout": "<address>",
                        "argv": [...]  # mines ONE attempt of its model on its node's current virtual, then exits}},
  "fixtures": {"<name>": [[argv], ...]},          # optional extra finite commands (bootstrap, registrations)
  "expected_consensus_params_id": "<hex>",       # pinned ruleset; a different binary/ruleset is BLOCKED
  "required_fences": ["palw_..."],               # must be reported active by getPalwNodeStatus.fences (retired only)
  "link_delays_ms": {"A>B": 0, "B>A": 900, ...}, # per directed link delivery delay (arrival-order variation)
  "partition": {"groups": [["A", "C"], ["B", "D"]], "daa_span": 40, "join_role": "Z"},
  "scenario_timeout_s": 900, "probe_timeout_s": 30
}
"""
import argparse
import hashlib
import json
from pathlib import Path
import selectors
import socket
import subprocess
import tempfile
import threading
import time

from misaka_wrpc_json import WsRpc


class Blocked(RuntimeError):
    pass


def norm(value):
    return value.removeprefix('0x').lower() if isinstance(value, str) else value


def field(row, *names, default=None):
    """The first present field of `row` among `names` (RPC JSON spellings vary by op version)."""
    for name in names:
        if isinstance(row, dict) and name in row and row[name] is not None:
            return row[name]
    return default


class DelayedLink:
    """Opaque loopback P2P link with a fixed per-direction delivery delay; `cut` closes only its own sockets."""

    def __init__(self, target, delay_ms):
        self.target, self.delay = target, delay_ms / 1000.0
        self.enabled, self.closed = threading.Event(), threading.Event()
        self.lock, self.connections = threading.Lock(), set()
        self.listener = socket.socket()
        self.listener.bind(('127.0.0.1', 0))
        self.listener.listen()
        self.listener.settimeout(.2)
        self.address = '127.0.0.1:%d' % self.listener.getsockname()[1]
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while not self.closed.is_set():
            try:
                incoming, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            if not self.enabled.is_set():
                incoming.close()
                continue
            threading.Thread(target=self.forward, args=(incoming,), daemon=True).start()

    def forward(self, incoming):
        outgoing = None
        try:
            outgoing = socket.create_connection(self.target, timeout=5)
            with self.lock:
                self.connections.update((incoming, outgoing))
            with selectors.DefaultSelector() as poll:
                poll.register(incoming, selectors.EVENT_READ, outgoing)
                poll.register(outgoing, selectors.EVENT_READ, incoming)
                while self.enabled.is_set() and not self.closed.is_set():
                    for event, _ in poll.select(.2):
                        data = event.fileobj.recv(65536)
                        if not data:
                            return
                        if self.delay and event.fileobj is incoming:
                            time.sleep(self.delay)  # this direction delivers late: a different arrival order downstream
                        event.data.sendall(data)
        except OSError:
            pass
        finally:
            with self.lock:
                self.connections.discard(incoming)
                self.connections.discard(outgoing)
            incoming.close()
            if outgoing:
                outgoing.close()

    def cut(self):
        self.enabled.clear()
        with self.lock:
            for conn in list(self.connections):
                try:
                    conn.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                conn.close()

    def close(self):
        self.cut()
        self.closed.set()
        self.listener.close()


class Drill:
    def __init__(self, plan, root):
        self.plan, self.root = plan, root
        self.processes, self.logs, self.links, self.observations = {}, {}, {}, []
        self.values = {'ROOT': str(root)}
        for role, node in plan['nodes'].items():
            self.values[role + '_APPDIR'] = str(root / role / 'appdir')
            self.values[role + '_WRPC'] = node['wrpc']

    # ---- processes -------------------------------------------------------------------------------------------------

    def argv(self, template):
        assert isinstance(template, list) and template and all(isinstance(x, str) for x in template), 'argv lists only'
        return [x.format_map(self.values) for x in template]

    def start(self, role):
        folder = self.root / role
        folder.mkdir(exist_ok=True)
        argv = self.argv(self.plan['nodes'][role]['argv'])
        assert '--nodnsseed' in argv and '--outpeers=0' in argv, 'peer discovery is the driver-owned links only'
        assert not any(x.startswith(('--connect', '--addpeer')) for x in argv), 'no unobserved peer route'
        assert self.values[role + '_APPDIR'] in ' '.join(argv), 'each node owns a fresh app dir'
        self.logs[role] = open(folder / ('node-%d.log' % len(self.logs)), 'w')
        self.processes[role] = subprocess.Popen(argv, stdout=self.logs[role], stderr=subprocess.STDOUT)

    def kill(self, role):
        process = self.processes.pop(role)
        process.kill()
        process.wait(timeout=20)
        self.logs[role].close()

    def command(self, argv, label):
        out = self.root / (label + '.out')
        with out.open('w') as sink:
            done = subprocess.run(argv, stdout=sink, stderr=subprocess.STDOUT, timeout=self.plan.get('command_timeout_s', 900))
        if done.returncode:
            raise Blocked('%s exited %s; see %s' % (label, done.returncode, out))

    def fixture(self, name):
        for i, argv in enumerate(self.plan.get('fixtures', {}).get(name, [])):
            self.command(self.argv(argv), 'fixture-%s-%d' % (name, i))

    def wait(self, predicate, label):
        deadline = time.monotonic() + self.plan.get('scenario_timeout_s', 900)
        last = None
        while time.monotonic() < deadline:
            for role, process in self.processes.items():
                if process.poll() is not None:
                    raise Blocked('%s exited %s; inspect its preserved log' % (role, process.returncode))
            try:
                result = predicate()
                if result:
                    return result
            except (OSError, ValueError, AssertionError, RuntimeError, KeyError) as error:
                last = repr(error)
            time.sleep(.5)
        raise AssertionError('%s timed out: %s' % (label, last))

    # ---- RPC -------------------------------------------------------------------------------------------------------

    def rpc(self, role):
        host, port = self.plan['nodes'][role]['wrpc'].split(':')
        return WsRpc(host=host, port=int(port), timeout=self.plan.get('probe_timeout_s', 30))

    def call(self, role, method, params=None):
        rpc = self.rpc(role)
        try:
            return rpc.call(method, params or {})
        finally:
            rpc.close()

    def probe(self, role):
        """One stable observation: the sink before and after must agree, or the probe is retried."""
        rpc = self.rpc(role)
        try:
            dag = rpc.call('getBlockDagInfo')
            node = rpc.call('getPalwNodeStatus')
            sink = rpc.call('getBlock', {'hash': dag['sink'], 'includeTransactions': False})['block']
            claims = rpc.call('getPalwClaims', {})
            after = rpc.call('getBlockDagInfo')
            assert after['sink'] == dag['sink'], 'the sink moved during the probe; retry'
        finally:
            rpc.close()
        row = {'role': role, 'dag': dag, 'node': node, 'sinkHeader': sink['header'], 'claims': claims}
        self.observations.append(row)
        return row

    def block(self, role, block_hash):
        return self.call(role, 'getBlock', {'hash': block_hash, 'includeTransactions': True})['block']

    # ---- links -----------------------------------------------------------------------------------------------------

    def link(self, src, dst):
        key = '%s>%s' % (src, dst)
        if key in self.links:
            self.links[key].enabled.set()
            return self.links[key]
        host, port = self.plan['nodes'][dst]['p2p'].split(':')
        assert host == '127.0.0.1', 'loopback peers only'
        link = DelayedLink((host, int(port)), self.plan.get('link_delays_ms', {}).get(key, 0))
        link.enabled.set()
        self.links[key] = link
        ip, p = link.address.rsplit(':', 1)
        self.call(src, 'addPeer', {'peerAddress': {'ip': ip, 'port': int(p)}, 'isPermanent': True})
        return link

    def mesh(self, roles):
        for a in roles:
            for b in roles:
                if a < b:
                    self.link(a, b)

    def cut_between(self, group_a, group_b):
        for key, link in self.links.items():
            src, dst = key.split('>')
            if (src in group_a and dst in group_b) or (src in group_b and dst in group_a):
                link.cut()

    def close(self):
        for link in self.links.values():
            link.close()
        for role in list(self.processes):
            self.kill(role)

    # ---- checks ----------------------------------------------------------------------------------------------------

    def ruleset(self, roles):
        for role in roles:
            node = self.probe(role)['node']
            pinned = norm(self.plan.get('expected_consensus_params_id'))
            if pinned and norm(field(node, 'consensusParamsId')) != pinned:
                raise Blocked('%s runs a ruleset other than the pinned one' % role)
            active = set(field(node, 'activeFences', 'fences', default=[]) or [])
            missing = [f for f in self.plan.get('required_fences', []) if f not in active]
            if missing:
                raise Blocked('%s does not report %s active: this binary/ruleset cannot run the scenario (arms nothing)' % (role, missing))

    def claim_rows(self, observation):
        rows = field(observation['claims'], 'claims', 'rows', default=[]) or []
        return {norm(field(r, 'claimId', 'id')): r for r in rows}

    def converged(self, roles):
        """One sink, one GHOSTDAG reading of its mergeset, one PALW state commitment, one claim set, one standing."""
        rows = [self.probe(role) for role in roles]
        sinks = {row['dag']['sink'] for row in rows}
        if len(sinks) != 1:
            return None
        headers = {json.dumps(row['sinkHeader'], sort_keys=True) for row in rows}
        assert len(headers) == 1, 'the sink header (UTXO/PALW commitments) differs across nodes'
        verbose = [field(self.block(role, rows[0]['dag']['sink']), 'verboseData', default={}) for role in roles]
        reading = {json.dumps([field(v, 'selectedParentHash'), sorted(field(v, 'mergeSetBluesHashes', default=[]) or []),
                               sorted(field(v, 'mergeSetRedsHashes', default=[]) or [])]) for v in verbose}
        assert len(reading) == 1, 'nodes read the sink mergeset differently (selected parent / blues / reds)'
        claim_sets = {json.dumps(sorted(self.claim_rows(row))) for row in rows}
        assert len(claim_sets) == 1, 'the adopted claim sets differ'
        economics = {json.dumps([field(row['node'], 'palwStateRoot', 'stateRoot'), field(row['node'], 'safeFrontierBlueScore'),
                                 field(row['node'], 'safeWeight'), field(row['node'], 'liveTotal')]) for row in rows}
        assert len(economics) == 1, 'the PALW economic state / fork-choice standing differs'
        return rows

    # ---- scenarios -------------------------------------------------------------------------------------------------

    def scenario_siblings(self):
        roles = sorted(self.plan['nodes'])
        for role in roles:
            self.start(role)
        for role in roles:
            self.wait(lambda role=role: self.probe(role), role + ' RPC startup')
        self.ruleset(roles)
        self.mesh(roles)
        self.fixture('bootstrap')
        base = self.wait(lambda: self.converged(roles), 'a common tip before the siblings')[0]['dag']['sink']
        miners = self.plan['miners']
        assert len({m['model'] for m in miners.values()}) >= 3, 'three distinct models'
        assert len({m['bond'] for m in miners.values()}) >= 3, 'three distinct miners (bonds)'
        assert len({m['node'] for m in miners.values()}) >= 3, 'three distinct producer nodes'
        # Isolate the producer nodes so each mines on the SAME base, then run the three miners concurrently.
        producers = sorted({m['node'] for m in miners.values()})
        for a in producers:
            self.cut_between([a], [r for r in roles if r != a])
        threads, failures = [], []
        for name, miner in sorted(miners.items()):
            def go(name=name, miner=miner):
                try:
                    self.command(self.argv(miner['argv']), 'miner-' + name)
                except Exception as error:  # noqa: BLE001 — recorded, then raised below
                    failures.append((name, repr(error)))
            threads.append(threading.Thread(target=go))
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        if failures:
            raise Blocked('a miner fixture failed: %r' % failures)
        siblings = {}
        for name, miner in miners.items():
            tips = self.call(miner['node'], 'getBlockDagInfo')['tipHashes']
            mined = [t for t in tips if t != base]
            assert len(mined) == 1, '%s: exactly one new tip on its own node' % name
            header = self.block(miner['node'], mined[0])['header']
            assert base in (field(header, 'parentsByLevel', default=[[]])[0] or []), '%s mined on the common base' % name
            siblings[name] = mined[0]
        for key in list(self.links):
            self.links[key].enabled.set()
        self.mesh(roles)
        self.fixture('merge')  # e.g. one heartbeat that merges the tips
        rows = self.wait(lambda: self.converged(roles), 'convergence after the siblings')
        sink = rows[0]['dag']['sink']
        report = {'base': base, 'siblings': siblings, 'colour': {}, 'claims': {}}
        verbose = field(self.block(roles[0], sink), 'verboseData', default={})
        chain = [field(verbose, 'selectedParentHash')]
        blues, reds = field(verbose, 'mergeSetBluesHashes', default=[]) or [], field(verbose, 'mergeSetRedsHashes', default=[]) or []
        claims = self.claim_rows(rows[0])
        by_block = {norm(field(r, 'acceptedBlock', 'carryingBlock')): r for r in claims.values()}
        for name, block in siblings.items():
            colour = 'SELECTED_PARENT' if block in chain else 'BLUE' if block in blues else 'RED' if block in reds else 'IN_PAST'
            report['colour'][name] = colour
            row = by_block.get(norm(block))
            assert row is not None, '%s: its claim is recorded (blue or red alike)' % name
            miner = miners[name]
            assert norm(field(row, 'bond', 'executorBond')) == norm(miner['bond']), '%s: attributed to its own bond' % name
            assert norm(field(row, 'classId', 'class')) == norm(miner['model']), '%s: recorded under its own model' % name
            report['claims'][name] = row
        # Panels (RFC18 audit D-numbers, 2026-10-17): each claim binds ITS OWN Panel, verified per claim — missing data FAILS (never a
        # default seat list or the requested id echoed back), and the binding, the seats, the verification result and the state
        # transition that follows are each asserted.
        self.fixture('advance_to_binding')
        panels = {}
        for name in siblings:
            claim_id = norm(field(report['claims'][name], 'claimId', 'id'))
            panel = self.call(roles[0], 'getPalwPanelAssignments', {'claimId': claim_id})
            answered = field(panel, 'claimId')
            if answered is None:
                raise AssertionError('%s: the panel read names no claim id (missing data is a failure, not the requested id)' % name)
            assert norm(answered) == claim_id, '%s: the panel answers for its own claim, not %s' % (name, answered)
            seats = field(panel, 'seats', 'assignments')
            if not seats:
                raise AssertionError('%s: the panel read carries no seats (missing data is a failure, never [])' % name)
            row = self.claim_rows(self.probe(roles[0])).get(claim_id)
            assert row is not None, '%s: its claim row is still served' % name
            assert field(row, 'phase') in ('panel_bound', 'receipt_licensed', 'final', 'voided'), \
                '%s: the claim is bound (phase %s)' % (name, field(row, 'phase'))
            row_seats = sorted(norm(x) for x in (field(row, 'seats', default=[]) or []))
            read_seats = sorted(norm(field(x, 'bond', 'seatBond') if isinstance(x, dict) else x) for x in seats)
            assert row_seats and row_seats == read_seats, '%s: the claim row binds exactly the seats the panel read names' % name
            assert norm(field(row, 'bond', 'executorBond')) not in read_seats, '%s: the producer never sits its own panel' % name
            panels[name] = {'claim': claim_id, 'seats': read_seats, 'boundDaa': field(row, 'boundDaa')}
        # Verification and the transition after it, per claim: a licence (or a void naming the reason) is observed for each.
        self.fixture('advance_to_verdict')
        for name, panel in panels.items():
            def verdict(panel=panel):
                r = self.claim_rows(self.probe(roles[0])).get(panel['claim'])
                return r if r is not None and field(r, 'phase') in ('receipt_licensed', 'final', 'voided') else None
            r = self.wait(verdict, '%s: its panel verifies and the claim moves on' % name)
            panel['phase'] = field(r, 'phase')
            panel['voidReason'] = field(r, 'voidReason')
            if panel['phase'] == 'voided':
                assert panel['voidReason'], '%s: a void names its reason' % name
        # A real per-claim check (each claim bound to its own derived panel above), and the cross-claim disjointness reported apart.
        report['panelsBoundPerClaim'] = len(panels) == len(siblings) and all(p['seats'] for p in panels.values())
        report['panelsDistinctByClaim'] = report['panelsBoundPerClaim']
        report['panelSeatSetsDisjoint'] = len({json.dumps(p['seats']) for p in panels.values()}) == len(panels)
        report['panels'] = panels
        self.report = report

    def scenario_partition(self, retired=False):
        spec = self.plan['partition']
        groups = spec['groups']
        roles = [r for g in groups for r in g]
        for role in roles:
            self.start(role)
        for role in roles:
            self.wait(lambda role=role: self.probe(role), role + ' RPC startup')
        self.ruleset(roles)
        self.mesh(roles)
        self.fixture('bootstrap')
        self.wait(lambda: self.converged(roles), 'a common tip before the partition')
        self.cut_between(groups[0], groups[1])
        start = {r: self.probe(r)['dag']['virtualDaaScore'] for r in roles}
        self.fixture('partitioned_mining')  # miners on BOTH sides, plan-pinned
        self.wait(lambda: all(self.probe(r)['dag']['virtualDaaScore'] - start[r] >= spec['daa_span'] for r in roles),
                  'both sides advance the predeclared DAA span')
        split = {r: self.probe(r)['dag']['sink'] for r in roles}
        assert split[groups[0][0]] != split[groups[1][0]], 'the partition produced two branches'
        for key in list(self.links):
            self.links[key].enabled.set()
        self.mesh(roles)
        healed = self.wait(lambda: self.converged(roles), 'reconnect convergence (arrival orders differ per link)')
        join = spec.get('join_role')
        if join:
            self.start(join)  # fresh app dir: IBD from the healed network
            self.wait(lambda: self.probe(join), join + ' RPC startup')
            self.link(join, roles[0])
            self.wait(lambda: self.converged(roles + [join]), 'fresh IBD convergence')
        report = {'split': split, 'healedSink': healed[0]['dag']['sink']}
        if retired:
            report['gateC'] = self.gate_c_checks(roles)
        self.report = report

    def adversary_metrics(self, role, adversary_payouts):
        """The six reported quantities, each from public RPC, attributable to the adversary's claims."""
        row = self.probe(role)
        claims = self.claim_rows(row)
        adversary = {cid: r for cid, r in claims.items() if norm(field(r, 'payout', 'payoutAddress', 'bondPayout')) in adversary_payouts
                     or field(r, 'adversary') is True}
        def total(key):
            return sum(int(field(r, key, default=0) or 0) for r in adversary.values())
        settlement = self.call(role, 'getPalwSettlement', {'daaScore': 0})
        premise = field(settlement, 'securityCertificationPremise')
        if premise is None:
            raise Blocked('%s does not state the certification premise: Gate C is not in force on this ruleset' % role)
        frontier_claim = norm(field(row['node'], 'safeFrontierClaim', default=''))
        return {
            'safeFrontierMovedByAdversary': bool(frontier_claim) and frontier_claim in adversary,
            'adversarySafeWeight': total('finalWeight'),
            'adversaryProvisionalWeight': total('provisionalWeight'),
            'settlementDepthFromAdversary': sum(1 for r in adversary.values() if field(r, 'securityCertified') is True),
            'ruleEParticipationCapitalAdversary': total('participationCapital'),
            'adversarySettledAnchorCredit': total('settledAnchorCredit'),
            'adversaryRewardOutflow': self.reward_outflow(role, adversary_payouts),
            'uncertifiedFinalAnchors': field(settlement, 'uncertifiedFinalAnchors', default=0),
            'premise': premise,
        }

    def reward_outflow(self, role, payouts):
        """Sompi paid to adversary payout scripts by the selected chain's coinbases since genesis (bounded by the plan's window)."""
        dag = self.call(role, 'getBlockDagInfo')
        chain = self.call(role, 'getVirtualChainFromBlock', {'startHash': self.plan['attack']['window_start'], 'includeAcceptedTransactionIds': False})
        paid = 0
        for block_hash in field(chain, 'addedChainBlockHashes', default=[]) or []:
            txs = field(self.block(role, block_hash), 'transactions', default=[]) or []
            if txs:
                for out in field(txs[0], 'outputs', default=[]) or []:
                    spk = norm(field(field(out, 'scriptPublicKey', default={}), 'scriptPublicKey', 'script', default=''))
                    if spk in payouts:
                        paid += int(field(out, 'value', default=0) or 0)
        assert dag['sink'], 'a sink'
        return paid

    def assert_no_illegitimate_finality(self, label, metrics):
        bad = {k: v for k, v in metrics.items() if k not in ('premise', 'uncertifiedFinalAnchors') and v not in (0, False)}
        assert not bad, '%s: illegitimate consensus credit from the closed model: %r' % (label, bad)

    def scenario_attack(self):
        spec = self.plan['attack']
        groups = self.plan['partition']['groups']
        roles = [r for g in groups for r in g]
        payouts = {norm(p) for p in spec['adversary_payouts']}
        assert spec['adversary_bonds'] >= 23_000, 'the ~23,076 minimum-bond split is part of the case'
        for role in roles:
            self.start(role)
        for role in roles:
            self.wait(lambda role=role: self.probe(role), role + ' RPC startup')
        self.ruleset(roles)
        self.mesh(roles)
        self.fixture('bootstrap')            # calibration genesis: bonds, the closed model, the adversary's split bonds
        self.fixture('adversary_mining')     # closed-model attempts through self-staffed Panels, voided attempts, a Free-Prompt Final
        self.fixture('honest_mining')        # open models with independently drawn verifiers attesting
        report = {'phases': {}}
        rows = self.wait(lambda: self.converged(roles), 'steady convergence')
        report['phases']['steady'] = m = self.adversary_metrics(roles[0], payouts)
        self.assert_no_illegitimate_finality('steady', m)
        # Reorg: the adversary group is partitioned off, mines alone, and rejoins.
        self.cut_between(groups[0], groups[1])
        self.fixture('adversary_mining')
        self.fixture('honest_mining')
        for key in list(self.links):
            self.links[key].enabled.set()
        self.mesh(roles)
        rows = self.wait(lambda: self.converged(roles), 'reorg convergence')
        report['phases']['reorg'] = m = self.adversary_metrics(roles[0], payouts)
        self.assert_no_illegitimate_finality('reorg', m)
        # Restart: kill and restart a node on its own app dir.
        victim = roles[-1]
        self.kill(victim)
        self.start(victim)
        self.wait(lambda: self.probe(victim), victim + ' restart')
        rows = self.wait(lambda: self.converged(roles), 'restart convergence')
        report['phases']['restart'] = m = self.adversary_metrics(victim, payouts)
        self.assert_no_illegitimate_finality('restart', m)
        # Pruning: the pruning point must advance (bounded progress) though no adversary claim ever certifies.
        before = rows[0]['dag']['pruningPointHash']
        self.fixture('advance_to_pruning')
        self.wait(lambda: self.probe(roles[0])['dag']['pruningPointHash'] != before, 'pruning progress without adversary credit')
        report['phases']['pruning'] = m = self.adversary_metrics(roles[0], payouts)
        self.assert_no_illegitimate_finality('pruning', m)
        # IBD: a fresh node syncs and reads the same.
        join = self.plan['partition'].get('join_role')
        if join:
            self.start(join)
            self.wait(lambda: self.probe(join), join + ' RPC startup')
            self.link(join, roles[0])
            self.wait(lambda: self.converged(roles + [join]), 'IBD convergence')
            report['phases']['ibd'] = m = self.adversary_metrics(join, payouts)
            self.assert_no_illegitimate_finality('ibd', m)
        self.report = report

    def gate_c_checks(self, roles):
        """Retired-only: an uncertified claim contributes nothing (as the node reports each claim)."""
        violations = []
        for role in roles:
            for claim_id, row in self.claim_rows(self.probe(role)).items():
                certified = field(row, 'independentlyWorkCertified')
                if certified is None:
                    raise Blocked('the node does not report Gate C per claim (independentlyWorkCertified): RFC-0018 GAP-R5')
                if certified is False:
                    for key in ('finalWeight', 'provisionalWeight', 'participationCapital', 'settledAnchorCredit', 'reward'):
                        if int(field(row, key, default=0) or 0) != 0:
                            violations.append((role, claim_id, key))
        assert not violations, 'uncertified claims contributed: %r' % violations[:10]
        return {'uncertifiedContributionViolations': 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('scenario', choices=['siblings', 'partition', 'retired', 'attack'])
    parser.add_argument('--plan', required=True)
    parser.add_argument('--out', required=True)
    args = parser.parse_args()
    raw = Path(args.plan).read_bytes()
    plan = json.loads(raw)
    ports = [n[k] for n in plan['nodes'].values() for k in ('wrpc', 'p2p')]
    assert all(p.startswith('127.0.0.1:') for p in ports) and len(ports) == len(set(ports)), 'distinct owned loopback ports'
    if args.scenario in ('retired', 'attack'):
        needed = {'palw_dns_retirement_v1', 'palw_fork_choice_rule_e_v1', 'palw_rule_e_participation_v2',
                  'palw_independently_work_certified_v1'}
        assert needed <= set(plan.get('required_fences', [])), 'the retired scenario pins its four fences'
    root = Path(tempfile.mkdtemp(prefix='misaka-rfc0018-%s-' % args.scenario))
    drill = Drill(plan, root)
    result = {'scenario': args.scenario, 'planSha256': hashlib.sha256(raw).hexdigest(), 'artifactDirectory': str(root),
              'status': 'FAIL', 'approval': False}
    try:
        if args.scenario == 'siblings':
            drill.scenario_siblings()
        elif args.scenario == 'attack':
            drill.scenario_attack()
        else:
            drill.scenario_partition(retired=args.scenario == 'retired')
        result.update(status='OBSERVED_CONSISTENT', report=drill.report)
    except Blocked as error:
        result.update(status='BLOCKED', reason=str(error))
    except Exception as error:  # noqa: BLE001 — every failure is recorded with the observations
        result['reason'] = repr(error)
    finally:
        drill.close()
        result['observations'] = drill.observations
        Path(args.out).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'observations'}, indent=2)[:4000])
    return 0 if result['status'] == 'OBSERVED_CONSISTENT' else 1


if __name__ == '__main__':
    raise SystemExit(main())
