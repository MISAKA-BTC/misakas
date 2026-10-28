#!/usr/bin/env python3
"""RFC-0019 real-process driver: a staged rollout — old and new binaries on owned local nodes, a rolling upgrade while consensus
stays on the old rules, the switch at a designated DAA, and what must hold on each side of it. From a plan pinned BEFORE launch.

  rolling   (§11 R-1) every node starts on the OLD binary; nodes are restarted one at a time on the NEW binary (same app dir)
            while the tip stays below the switch. After every step the MIXED set has one sink, one sink header (UTXO / PALW
            commitments), one PALW state root and frontier, one adopted-claim set — old and new agree on blocks, state and fork choice.
  switch    (§11 R-2) continue past the switch on the upgraded set: one sink, one state; every upgraded node reports the same catalog
            edition / root in force and the same tier for every class (getPalwModelRegistry v4 annualCatalog, modelTier) right
            after the switch. Writes one misaka/interop-record/v1 per upgraded node (input to misaka-release-readiness.py M3).
  stale     (§11 R-3) one node is left on the OLD binary across the switch: it must never report a successful sync (isSynced) on a
            sink the upgraded set does not have, and the upgraded set must not adopt its blocks (fork-id refusal by name).
  boundary  (§11 R-4) sibling attempts mined BELOW the switch on isolated producers, merged (BLUE / RED) by chain blocks AT OR ABOVE
            it after reconnecting: every sibling retained once, one accepted DAA / accepting block per claim on every node, no claim
            twice (no double rights across the boundary).
  restart   (§11 R-5) after the switch: kill / restart an upgraded node on its own app dir, a fresh IBD node from an empty app dir,
            a partition and rejoin, pruning progress — the same root, tiers and state everywhere afterwards.
  liveness  (§11 R-6) no miner of any catalog model runs: heartbeat / BASE-0 fixtures alone carry the DAA across the switch on
            every upgraded node.

The testnet-12 DAA 14,000 upgrade (docs/design/palw/integration/t12-daa14000-release-plan-2026-10-15.md) is the first use: the SAME catalog
(edition 0, no new model), a rolling binary upgrade, the switch at the flag-day height. Run it with `switch_daa` = the plan's
calibration height on a local devnet built from the shipped binary, then `rolling`, `switch`, `stale`, `restart`, `liveness`.

Ground rules (the RFC-0018 / RFC-12 drills', kept): every role is its own process with an owned app dir; only ordinary P2P bytes
cross driver-owned loopback links (`--nodnsseed --outpeers=0`, no `--connect` / `--addpeer`); producers are plan-pinned, finite
fixture argv lists; every verdict is read from public wRPC at a stable sink. **This driver ARMS NOTHING and installs nothing**: a
NEW binary that does not schedule `switch_daa` (getPalwNodeStatus.fenceSchedule) yields BLOCKED. Its output is an observation
record, never an approval; a run whose every check held is OBSERVED_CONSISTENT.

Plan (JSON): {
  "nodes": {"<ROLE>": {"old_argv": [...], "new_argv": [...], "wrpc": "127.0.0.1:P", "p2p": "127.0.0.1:Q", "operator": "<name>"}},
  "switch_daa": 400,                       # the fence / edition height under test
  "margin_daa": 10,                        # how far below the switch the rolling phase must finish, and past it the checks run
  "roll_order": ["A", "B", "C"],           # rolling upgrade order (default: sorted roles minus stale / ibd roles)
  "stale_role": "D", "ibd_role": "Z",
  "expected": {"new_schedule_id": "<hex>", "catalog_root_after": "<hex, optional>", "catalog_edition_after": 0},
  "fixtures": {"bootstrap": [[argv]], "produce": [[argv]],          # `produce`: finite, advances the DAA by a few blocks
               "heartbeat": [[argv]], "siblings": {"<ROLE>": [[argv]]}, "advance_to_pruning": [[argv]]},
  "link_delays_ms": {"A>B": 0, ...}, "partition": {"groups": [["A"], ["B", "C"]]},
  "stale_grace_s": 300, "scenario_timeout_s": 3600, "probe_timeout_s": 30
}
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time

from misaka_wrpc_json import WsRpc
from rfc0018_dag_drill import Blocked, DelayedLink, field, norm


class Drill:
    def __init__(self, plan, root):
        self.plan, self.root = plan, root
        self.processes, self.binary, self.logs, self.links, self.observations = {}, {}, {}, {}, []
        self.records, self.report = [], {}
        self.values = {'ROOT': str(root)}
        for role, node in plan['nodes'].items():
            self.values[role + '_APPDIR'] = str(root / role / 'appdir')
            self.values[role + '_WRPC'] = node['wrpc']
        self.switch = int(plan['switch_daa'])
        self.margin = int(plan.get('margin_daa', 10))

    # ---- processes -------------------------------------------------------------------------------------------------

    def argv(self, template):
        assert isinstance(template, list) and template and all(isinstance(x, str) for x in template), 'argv lists only'
        return [x.format_map(self.values) for x in template]

    def start(self, role, binary):
        folder = self.root / role
        folder.mkdir(exist_ok=True)
        argv = self.argv(self.plan['nodes'][role][binary + '_argv'])
        assert '--nodnsseed' in argv and '--outpeers=0' in argv, 'peer discovery is the driver-owned links only'
        assert not any(x.startswith(('--connect', '--addpeer')) for x in argv), 'no unobserved peer route'
        assert self.values[role + '_APPDIR'] in ' '.join(argv), 'each node owns its app dir'
        self.logs[role] = open(folder / ('node-%s-%d.log' % (binary, len(self.logs))), 'w')
        self.processes[role] = subprocess.Popen(argv, stdout=self.logs[role], stderr=subprocess.STDOUT)
        self.binary[role] = binary
        self.wait(lambda: self.status(role), '%s RPC startup (%s binary)' % (role, binary))
        # Links survive a restart (the listener is the driver's); re-dial the ones this role originates.
        for key, link in self.links.items():
            src, _ = key.split('>')
            if src == role and link.enabled.is_set():
                ip, p = link.address.rsplit(':', 1)
                self.call(src, 'addPeer', {'peerAddress': {'ip': ip, 'port': int(p)}, 'isPermanent': True})

    def kill(self, role):
        process = self.processes.pop(role)
        process.kill()
        process.wait(timeout=30)
        self.logs[role].close()

    def command(self, argv, label):
        out = self.root / (label + '.out')
        with out.open('w') as sink:
            done = subprocess.run(argv, stdout=sink, stderr=subprocess.STDOUT, timeout=self.plan.get('command_timeout_s', 900))
        if done.returncode:
            raise Blocked('%s exited %s; see %s' % (label, done.returncode, out))

    def fixture(self, name, role=None):
        entry = self.plan.get('fixtures', {}).get(name, [])
        commands = entry.get(role, []) if isinstance(entry, dict) else entry
        if not commands:
            raise Blocked('the plan has no %r fixture%s' % (name, ' for ' + role if role else ''))
        for i, argv in enumerate(commands):
            self.command(self.argv(argv), 'fixture-%s-%s-%d' % (name, role or 'all', i))

    def wait(self, predicate, label, timeout=None):
        deadline = time.monotonic() + (timeout or self.plan.get('scenario_timeout_s', 3600))
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

    def call(self, role, method, params=None):
        host, port = self.plan['nodes'][role]['wrpc'].split(':')
        rpc = WsRpc(host=host, port=int(port), timeout=self.plan.get('probe_timeout_s', 30))
        try:
            return rpc.call(method, params or {})
        finally:
            rpc.close()

    def status(self, role):
        return self.call(role, 'getPalwNodeStatus')

    def daa(self, role):
        return int(field(self.call(role, 'getBlockDagInfo'), 'virtualDaaScore', default=0) or 0)

    def probe(self, role):
        """One stable observation: the sink before and after must agree, or the probe is retried."""
        dag = self.call(role, 'getBlockDagInfo')
        node = self.status(role)
        registry = self.call(role, 'getPalwModelRegistry')
        sink = self.call(role, 'getBlock', {'hash': dag['sink'], 'includeTransactions': False})['block']
        claims = self.call(role, 'getPalwClaims', {})
        after = self.call(role, 'getBlockDagInfo')
        assert after['sink'] == dag['sink'], 'the sink moved during the probe; retry'
        row = {'role': role, 'binary': self.binary.get(role), 'dag': dag, 'node': node, 'registry': registry,
               'sinkHeader': sink['header'], 'claims': claims}
        self.observations.append(row)
        return row

    # ---- links -----------------------------------------------------------------------------------------------------

    def link(self, src, dst):
        key = '%s>%s' % (src, dst)
        if key in self.links:
            self.links[key].enabled.set()
        else:
            host, port = self.plan['nodes'][dst]['p2p'].split(':')
            assert host == '127.0.0.1', 'loopback peers only'
            self.links[key] = DelayedLink((host, int(port)), self.plan.get('link_delays_ms', {}).get(key, 0))
            self.links[key].enabled.set()
        ip, p = self.links[key].address.rsplit(':', 1)
        self.call(src, 'addPeer', {'peerAddress': {'ip': ip, 'port': int(p)}, 'isPermanent': True})

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

    def roles(self):
        skip = {self.plan.get('stale_role'), self.plan.get('ibd_role')}
        return [r for r in sorted(self.plan['nodes']) if r not in skip]

    def claim_rows(self, observation):
        rows = field(observation['claims'], 'claims', 'rows', default=[]) or []
        return {norm(field(r, 'claimId', 'id')): r for r in rows}

    def converged(self, roles):
        """One sink, one sink header (commitments), one PALW state / frontier, one claim set — whatever binary each node runs."""
        rows = [self.probe(role) for role in roles]
        if len({row['dag']['sink'] for row in rows}) != 1:
            return None
        assert len({json.dumps(row['sinkHeader'], sort_keys=True) for row in rows}) == 1, 'the sink header differs'
        verbose = [field(self.call(role, 'getBlock', {'hash': rows[0]['dag']['sink'], 'includeTransactions': False})['block'],
                         'verboseData', default={}) for role in roles]
        reading = {json.dumps([field(v, 'selectedParentHash'), sorted(field(v, 'mergeSetBluesHashes', default=[]) or []),
                               sorted(field(v, 'mergeSetRedsHashes', default=[]) or [])]) for v in verbose}
        assert len(reading) == 1, 'nodes read the sink mergeset differently (fork choice)'
        assert len({json.dumps(sorted(self.claim_rows(row))) for row in rows}) == 1, 'the adopted claim sets differ'
        economics = {json.dumps([field(row['node'], 'palwStateRoot', 'stateRoot'), field(row['node'], 'safeFrontierBlueScore'),
                                 field(row['node'], 'safeWeight'), field(row['node'], 'liveTotal')]) for row in rows}
        assert len(economics) == 1, 'the PALW state / fork-choice standing differs'
        return rows

    def require_new_schedules_switch(self, role):
        schedule = [int(x) for x in field(self.status(role), 'fenceSchedule', default=[]) or []]
        if self.switch not in schedule:
            raise Blocked('%s (new binary) does not schedule DAA %d: this binary cannot run the drill (arms nothing)' % (role, self.switch))
        pinned = norm(self.plan.get('expected', {}).get('new_schedule_id'))
        if pinned and norm(field(self.status(role), 'consensusScheduleId')) != pinned:
            raise Blocked('%s runs a schedule other than the pinned one' % role)

    def advance(self, roles, target, fixture='produce'):
        """Run the finite producer fixture until every role's virtual DAA reaches `target`."""
        while min(self.daa(r) for r in roles) < target:
            self.fixture(fixture)
        return self.wait(lambda: self.converged(roles), 'convergence at DAA %d' % target)

    def catalog_reading(self, row):
        cat = field(row['registry'], 'annualCatalog', default={}) or {}
        tiers = {norm(field(c, 'classId')): (field(c, 'modelTier'), field(c, 'modelTierPermille'))
                 for c in field(row['registry'], 'classes', default=[]) or []}
        return {'edition': field(cat, 'currentEdition'), 'root': norm(field(cat, 'currentRoot')), 'tiers': tiers}

    def interop_record(self, role, checks):
        argv0 = self.argv(self.plan['nodes'][role]['new_argv'])[0]
        try:
            digest = hashlib.sha256(Path(argv0).read_bytes()).hexdigest()
        except OSError:
            digest = None
        self.records.append({'schema': 'misaka/interop-record/v1', 'operator': self.plan['nodes'][role].get('operator', role),
                             'node': role, 'binary_sha256': digest, 'crossed_switch_daa': self.switch, 'checks': checks})

    # ---- scenarios -------------------------------------------------------------------------------------------------

    def scenario_rolling(self):
        roles = self.roles()
        for role in roles:
            self.start(role, 'old')
        self.mesh(roles)
        self.fixture('bootstrap')
        self.wait(lambda: self.converged(roles), 'a common tip on the old binary')
        steps = []
        for role in self.plan.get('roll_order', roles):
            if max(self.daa(r) for r in roles) >= self.switch - self.margin:
                raise Blocked('the tip reached the switch before the rolling upgrade finished: raise switch_daa or shorten the roll')
            self.kill(role)
            self.start(role, 'new')
            self.require_new_schedules_switch(role)
            self.fixture('produce')
            rows = self.wait(lambda: self.converged(roles), 'mixed-binary convergence after upgrading %s' % role)
            ids = {self.binary[r]: norm(field(row['node'], 'consensusParamsId')) for r, row in zip(roles, rows)}
            steps.append({'upgraded': role, 'daa': self.daa(role), 'sink': rows[0]['dag']['sink'], 'binaries': dict(self.binary),
                          'paramsIdByBinary': ids})
        self.report['rolling'] = steps
        return roles

    def scenario_switch(self):
        roles = self.scenario_rolling()
        rows = self.advance(roles, self.switch + self.margin)
        readings = {r: self.catalog_reading(row) for r, row in zip(roles, rows)}
        assert len({json.dumps(v, sort_keys=True) for v in readings.values()}) == 1, 'upgraded nodes read different catalogs / tiers: %r' % readings
        one = next(iter(readings.values()))
        exp = self.plan.get('expected', {})
        if exp.get('catalog_root_after'):
            assert one['root'] == norm(exp['catalog_root_after']), 'the catalog root in force is not the pinned one'
        if exp.get('catalog_edition_after') is not None:
            assert one['edition'] == exp['catalog_edition_after'], 'the edition in force is not the pinned one'
        self.report['switch'] = {'daa': self.daa(roles[0]), 'catalog': one}
        for role in roles:
            self.interop_record(role, {'sync': True, 'reorg': None, 'ibd': None})
        return roles

    def scenario_stale(self):
        stale = self.plan.get('stale_role')
        if not stale:
            raise Blocked('the plan names no stale_role')
        roles = self.roles()
        self.start(stale, 'old')
        for role in roles:
            self.link(stale, role)
        roles = self.scenario_switch()
        upgraded_sink = self.probe(roles[0])['dag']['sink']
        grace = time.monotonic() + self.plan.get('stale_grace_s', 300)
        samples = []
        while time.monotonic() < grace:
            info = self.call(stale, 'getServerInfo')
            dag = self.call(stale, 'getBlockDagInfo')
            synced = bool(field(info, 'isSynced', default=False))
            samples.append({'isSynced': synced, 'sink': dag['sink'], 'daa': int(field(dag, 'virtualDaaScore', default=0) or 0)})
            if synced and dag['sink'] != upgraded_sink and samples[-1]['daa'] >= self.switch:
                raise AssertionError('the stale node reports a successful sync on a sink the upgraded set does not have: %r' % samples[-1])
            time.sleep(5)
        peers = self.call(stale, 'getConnectedPeerInfo')
        self.report['stale'] = {'samples': samples[-5:], 'connectedPeers': len(field(peers, 'peerInfo', default=[]) or []),
                                'upgradedSink': upgraded_sink,
                                'note': 'the upgraded set refuses the stale build by fork id from the switch; see the stale log for the named refusal'}
        assert self.converged(roles), 'the upgraded set lost convergence while a stale node was attached'

    def scenario_boundary(self):
        roles = self.scenario_rolling()
        producers = sorted(self.plan.get('fixtures', {}).get('siblings', {}))
        if len(producers) < 2:
            raise Blocked('the boundary scenario needs sibling fixtures on at least two producers')
        self.advance(roles, self.switch - self.margin // 2)
        before = {k for k in self.claim_rows(self.probe(roles[0]))}
        for p in producers:
            self.cut_between([p], [r for r in roles if r != p])
        for p in producers:
            self.fixture('siblings', p)
        mined_below = {p: self.daa(p) for p in producers}
        assert all(d < self.switch for d in mined_below.values()), 'siblings must be mined below the switch: %r' % mined_below
        # One producer carries the chain across the switch alone; the others' siblings merge into it after reconnecting.
        carrier = producers[0]
        while self.daa(carrier) < self.switch + 1:
            self.fixture('produce')
        self.mesh(roles)
        self.fixture('produce')
        rows = self.wait(lambda: self.converged(roles), 'convergence after merging siblings across the switch')
        claims = [self.claim_rows(row) for row in rows]
        new_ids = set(claims[0]) - before
        assert new_ids, 'no sibling claim was adopted'
        for cid in new_ids:
            accepted = {(field(c[cid], 'acceptedDaa'), field(c[cid], 'acceptedBlock')) for c in claims}
            assert len(accepted) == 1, 'nodes record a different accepted DAA / block for %s: %r' % (cid, accepted)
        listed = [cid for c in claims for cid in c]
        assert len(listed) == len(claims) * len(claims[0]), 'a claim id appears twice'
        self.report['boundary'] = {'minedBelow': mined_below, 'adopted': sorted(new_ids),
                                   'acceptedDaa': {cid: field(claims[0][cid], 'acceptedDaa') for cid in new_ids}}

    def scenario_restart(self):
        roles = self.scenario_switch()
        victim = roles[-1]
        self.kill(victim)
        self.start(victim, 'new')
        self.wait(lambda: self.converged(roles), 'restart convergence')
        groups = self.plan.get('partition', {}).get('groups')
        reorg_ok = None
        if groups:
            self.cut_between(groups[0], groups[1])
            self.fixture('produce')
            self.mesh(roles)
            self.fixture('produce')
            self.wait(lambda: self.converged(roles), 'partition rejoin convergence')
            reorg_ok = True
        ibd_ok = None
        join = self.plan.get('ibd_role')
        if join:
            self.start(join, 'new')
            self.link(join, roles[0])
            self.wait(lambda: self.converged(roles + [join]), 'IBD convergence')
            ibd_ok = True
        if self.plan.get('fixtures', {}).get('advance_to_pruning'):
            before = self.probe(roles[0])['dag']['pruningPointHash']
            self.fixture('advance_to_pruning')
            self.wait(lambda: self.probe(roles[0])['dag']['pruningPointHash'] != before, 'pruning progress')
            self.wait(lambda: self.converged(roles), 'convergence after pruning')
        self.records = [r for r in self.records if r['node'] not in roles]
        for role in roles:
            self.interop_record(role, {'sync': True, 'reorg': reorg_ok, 'ibd': ibd_ok})
        self.report['restart'] = {'reorg': reorg_ok, 'ibd': ibd_ok}

    def scenario_liveness(self):
        roles = self.roles()
        for role in roles:
            self.start(role, 'new')
            self.require_new_schedules_switch(role)
        self.mesh(roles)
        self.fixture('bootstrap')
        start = min(self.daa(r) for r in roles)
        self.advance(roles, self.switch + self.margin, fixture='heartbeat')
        self.report['liveness'] = {'fromDaa': start, 'toDaa': min(self.daa(r) for r in roles),
                                   'note': 'heartbeat / BASE-0 fixtures only; no catalog-model miner ran'}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('scenario', choices=['rolling', 'switch', 'stale', 'boundary', 'restart', 'liveness'])
    parser.add_argument('--plan', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--interop-out', help='where to write the misaka/interop-record/v1 list (readiness M3)')
    args = parser.parse_args()
    raw = Path(args.plan).read_bytes()
    plan = json.loads(raw)
    ports = [n[k] for n in plan['nodes'].values() for k in ('wrpc', 'p2p')]
    assert all(p.startswith('127.0.0.1:') for p in ports) and len(ports) == len(set(ports)), 'distinct owned loopback ports'
    assert all('old_argv' in n and 'new_argv' in n for n in plan['nodes'].values()), 'every role names its old and new binary'
    root = Path(tempfile.mkdtemp(prefix='misaka-rfc0019-%s-' % args.scenario))
    drill = Drill(plan, root)
    result = {'scenario': args.scenario, 'planSha256': hashlib.sha256(raw).hexdigest(), 'artifactDirectory': str(root),
              'switchDaa': drill.switch, 'status': 'FAIL', 'approval': False}
    try:
        getattr(drill, 'scenario_' + args.scenario)()
        result.update(status='OBSERVED_CONSISTENT', report=drill.report)
    except Blocked as error:
        result.update(status='BLOCKED', reason=str(error))
    except Exception as error:  # noqa: BLE001 — every failure is recorded with the observations
        result['reason'] = repr(error)
    finally:
        drill.close()
        result['observations'] = drill.observations
        Path(args.out).write_text(json.dumps(result, indent=2) + '\n')
        if args.interop_out and result['status'] == 'OBSERVED_CONSISTENT':
            Path(args.interop_out).write_text(json.dumps(drill.records, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'observations'}, indent=2)[:4000])
    return 0 if result['status'] == 'OBSERVED_CONSISTENT' else 1


if __name__ == '__main__':
    raise SystemExit(main())
