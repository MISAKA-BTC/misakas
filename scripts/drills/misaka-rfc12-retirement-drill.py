#!/usr/bin/env python3
"""RFC-0012 §4.4 final evidence: the item-7 retirement reconciliation against the SHIPPED binary's drill.

The user decision (2026-10-15 §8) makes the final evidence for the DNS retirement
reconciliation "the same reconciliation run against the shipped binary across
testnet-12 DAA 14,000 in the drill". This script is that run's reader. It starts
no node and writes nothing to any node: the drill nodes are the shipped kaspad
with --palw-drill-genesis-salt, run by the drill tooling on drill hosts
(misaka-palw-t12-rcore-drill.sh conventions). It attaches read-only to their
loopback wRPC listeners named in a pinned plan, waits until the chain is past
the fence plus the plan's observation span, and on every ARCHIVAL node:
  * scans chain blocks from the plan's DAA to the current one (bounded);
  * classifies every coinbase output (rfc12_retirement_reconciliation): DNS
    validator share from the bonds' own StakeBond transactions, miner from
    coinbase payloads, PALW payouts from the drill keyring groups the plan names;
  * judges checkbox 7 exactly as the private acceptance run does;
and on EVERY node checks the shipped ruleset arms the retirement at the pinned
height (getPalwSettlement dnsRetiredAt) with no live DNS authority after it.
Archival nodes must agree on the classification of every block they share.

Verdicts: OBSERVED / BLOCKED / FAIL / NOT_RUN; never PASS. Nothing here is a
release approval; it is one reader's mechanical record of one drill.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import rfc12_acceptance_judgement as J  # noqa: E402
import rfc12_acceptance_plan as P  # noqa: E402
import rfc12_retirement_reconciliation as R  # noqa: E402
from misaka_wrpc_json import WsRpc  # noqa: E402

PLAN_KIND = 'rfc12-retirement-drill-plan'
PLAN_VERSION = 1
EVIDENCE_KIND = 'rfc12-retirement-drill-evidence'
KEYRING_GROUPS = ('seats', 'bonds', 'payout', 'heartbeat', 'main')


def _endpoint(value, name):
    if not isinstance(value, str) or not value.startswith('127.0.0.1:') or not value.split(':', 1)[1].isdigit():
        raise P.PlanError(name + ' must be a loopback 127.0.0.1:port wRPC listener (read-only attach)')
    return value


def parse_drill_plan(raw):
    if not isinstance(raw, (bytes, bytearray)):
        raise P.PlanError('the plan must be read as exact bytes')
    try:
        plan = json.loads(raw)
    except ValueError as error:
        raise P.PlanError('plan is not JSON: %s' % error)
    P._keys(plan, ('schemaVersion', 'kind', 'commit', 'shippedBinary', 'salt', 'identity', 'nodes', 'keyringManifest',
                   'palwPayoutGroups', 'window', 'scan', 'bounds'), 'plan')
    if type(plan['schemaVersion']) is not int or plan['schemaVersion'] != PLAN_VERSION or plan['kind'] != PLAN_KIND:
        raise P.PlanError('unsupported retirement drill plan kind/version')
    binary = P._keys(plan['shippedBinary'], ('path', 'sha256'), 'shippedBinary')
    keyring = P._keys(plan['keyringManifest'], ('path', 'sha256'), 'keyringManifest')
    identity = P._identity(plan['identity'])
    if identity['retirementActivationDaa'] < 2:
        raise P.PlanError('a drill reconciliation needs a fence above genesis so pre-fence history exists')
    nodes, names, archival = [], set(), 0
    if not isinstance(plan['nodes'], list) or not plan['nodes']:
        raise P.PlanError('nodes must list the drill nodes to attach to')
    for index, node in enumerate(plan['nodes']):
        P._keys(node, ('name', 'wrpc', 'archival'), 'nodes[%d]' % index)
        name = P._text(node['name'], 'nodes[%d].name' % index, r'[a-z0-9][a-z0-9-]{0,31}')
        if name in names:
            raise P.PlanError('node names must be unique')
        names.add(name)
        archival += P._bool(node['archival'], 'nodes[%d].archival' % index)
        nodes.append(dict(name=name, wrpc=_endpoint(node['wrpc'], 'nodes[%d].wrpc' % index), archival=node['archival']))
    if not archival:
        raise P.PlanError('at least one archival node must serve the pre-fence bodies')
    groups = plan['palwPayoutGroups']
    if not isinstance(groups, list) or not groups or len(set(groups)) != len(groups) or any(g not in KEYRING_GROUPS for g in groups):
        raise P.PlanError('palwPayoutGroups must name distinct keyring groups (%s)' % ', '.join(KEYRING_GROUPS))
    window = P._keys(plan['window'], ('scanFromDaa', 'observeAfterFenceDaa', 'legacySettlementWindowDaa', 'dnsCadence',
                                       'derivation'), 'window')
    if not isinstance(window['derivation'], str) or not window['derivation'].strip():
        raise P.PlanError('window.derivation must cite the node rule the legacy settlement window is derived from')
    scan = P._keys(plan['scan'], ('maximumChainBlocks', 'maximumBlockReads', 'maximumStatusReads', 'bondPageLimit',
                                  'maximumBondPages'), 'scan')
    bounds = P._keys(plan['bounds'], ('workflowTimeoutSeconds', 'probeTimeoutSeconds', 'pollSeconds'), 'bounds')
    out = dict(schemaVersion=PLAN_VERSION, kind=PLAN_KIND, sha256=hashlib.sha256(bytes(raw)).hexdigest(),
               commit=P._hex(plan['commit'], 'commit', 40, True),
               shippedBinary=dict(path=P._absolute(binary['path'], 'shippedBinary.path'),
                                  sha256=P._hex(binary['sha256'], 'shippedBinary.sha256', 64, True)),
               salt=P._hex(plan['salt'], 'salt', 64, True), identity=identity, nodes=nodes,
               keyringManifest=dict(path=P._absolute(keyring['path'], 'keyringManifest.path'),
                                    sha256=P._hex(keyring['sha256'], 'keyringManifest.sha256', 64, True)),
               palwPayoutGroups=list(groups),
               window=dict(scanFromDaa=P._int(window['scanFromDaa'], 'window.scanFromDaa'),
                           observeAfterFenceDaa=P._int(window['observeAfterFenceDaa'], 'window.observeAfterFenceDaa', 1),
                           legacySettlementWindowDaa=P._int(window['legacySettlementWindowDaa'],
                                                            'window.legacySettlementWindowDaa'),
                           dnsCadence=P.dns_cadence(window['dnsCadence'], 'window.dnsCadence'),
                           derivation=window['derivation']),
               scan={key: P._int(scan[key], 'scan.' + key, 1) for key in scan},
               bounds={key: P._int(bounds[key], 'bounds.' + key, 1) for key in bounds})
    if out['window']['scanFromDaa'] >= identity['retirementActivationDaa']:
        raise P.PlanError('window.scanFromDaa must start below the fence')
    P.check_legacy_window(identity['retirementActivationDaa'], out['window']['legacySettlementWindowDaa'],
                          out['window']['observeAfterFenceDaa'], out['window']['dnsCadence'], 'window')
    if out['scan']['bondPageLimit'] > 1000:
        raise P.PlanError('scan.bondPageLimit is at most 1000')
    return out


def keyring_addresses(manifest, groups):
    """PALW payout addresses of the named drill keyring groups (`kaspad --palw-drill-write-keyring` manifest)."""
    addresses = set()
    for group in groups:
        rows = manifest.get(group)
        if rows is None:
            raise J.NotObservable('the keyring manifest has no %s group' % group)
        for row in ([rows] if isinstance(rows, dict) else rows):
            if not isinstance(row, dict) or not isinstance(row.get('address'), str):
                raise J.NotObservable('keyring %s row without an address' % group)
            addresses.add(row['address'])
    return addresses


def shipped_retirement(row, identity):
    """The shipped ruleset arms the retirement at the pinned height and serves no DNS authority past it."""
    J.require_real([row])
    settlement = row['value']['settlement'] or {}
    if settlement.get('dnsRetiredAt') != identity['retirementActivationDaa']:
        raise J.NotObservable('%s does not report retirement at the pinned DAA %d (reports %r): the shipped ruleset does not '
                              'arm it there' % (row['role'], identity['retirementActivationDaa'], settlement.get('dnsRetiredAt')))
    if (row['value']['dns'] or {}).get('available'):
        raise J.Violation('%s still serves DNS finality authority after the fence' % row['role'])
    return dict(node=row['role'], dnsRetiredAt=settlement['dnsRetiredAt'])


def classifications_agree(per_node):
    """Archival nodes classify every block they both scanned identically."""
    seen = {}
    for name, blocks in per_node.items():
        for block in blocks:
            key = tuple((output['index'], output['kind'], output['bond']) for output in block['outputs'])
            if seen.setdefault(block['hash'], (name, key))[1] != key:
                raise J.Violation('nodes %s and %s classify block %s differently' % (seen[block['hash']][0], name, block['hash'][:16]))
    if len(per_node) < 2:
        raise J.NotObservable('one archival node: cross-node classification agreement is not exercised')
    return dict(blocks=len(seen), nodes=sorted(per_node))


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as source:
        for chunk in iter(lambda: source.read(1 << 20), b''):
            digest.update(chunk)
    return digest.hexdigest()


class DrillReader:
    def __init__(self, plan, out):
        self.plan, self.out, self.findings = plan, out, []
        self.deadline = time.monotonic() + plan['bounds']['workflowTimeoutSeconds']
        self.evidence = dict(kind=EVIDENCE_KIND, schemaVersion=1, planSha256=plan['sha256'], plan=plan, status=J.BLOCKED,
                             passLabelUsed=False, releaseApproval=False, nodes={})

    def rpc(self, node):
        host, port = node['wrpc'].split(':')
        return WsRpc(host=host, port=int(port), timeout=self.plan['bounds']['probeTimeoutSeconds'])

    def judge(self, predicate, function, *args):
        item = J.judge(7, predicate, function, *args)
        self.findings.append(item)
        return item['facts'] if item['verdict'] == J.OBSERVED else None

    def identity(self):
        binary = self.plan['shippedBinary']
        actual = sha256_file(binary['path'])
        self.evidence['shippedBinarySha256'] = actual
        if actual != binary['sha256']:
            raise J.NotObservable('the shipped binary at %s is not the pinned build' % binary['path'])
        keyring = self.plan['keyringManifest']
        if sha256_file(keyring['path']) != keyring['sha256']:
            raise J.NotObservable('the keyring manifest is not the pinned file')
        repository = Path(__file__).resolve().parents[2]
        head = subprocess.run(['git', '-C', str(repository), 'rev-parse', 'HEAD'], capture_output=True, text=True,
                              timeout=30).stdout.strip()
        self.evidence['harnessCommit'] = head
        if head != self.plan['commit']:
            raise J.NotObservable('the harness checkout %s is not the pinned commit %s' % (head, self.plan['commit']))
        manifest = json.loads(Path(keyring['path']).read_bytes())
        if J.norm(manifest.get('genesis_hash')) != self.plan['identity']['genesis']:
            raise J.NotObservable('the keyring belongs to another drill genesis')
        return manifest

    def node_identity(self, node):
        rpc = self.rpc(node)
        try:
            status = rpc.call('getPalwNodeStatus')
        finally:
            rpc.close()
        if (J.norm(status.get('genesisHash')), J.norm(status.get('consensusParamsId'))) != \
                (self.plan['identity']['genesis'], self.plan['identity']['consensusParamsId']):
            raise J.NotObservable('%s runs another genesis or ruleset than the pinned drill identity' % node['name'])

    def wait_past_fence(self, node):
        target = self.plan['identity']['retirementActivationDaa'] + self.plan['window']['observeAfterFenceDaa']
        while True:
            rpc = self.rpc(node)
            try:
                daa = rpc.call('getBlockDagInfo')['virtualDaaScore']
            finally:
                rpc.close()
            if daa >= target:
                return daa
            if time.monotonic() >= self.deadline:
                raise J.NotObservable('%s did not pass DAA %d within the plan bound (at %d)' % (node['name'], target, daa))
            time.sleep(self.plan['bounds']['pollSeconds'])

    def settlement_row(self, node):
        rpc = self.rpc(node)
        try:
            dag = rpc.call('getBlockDagInfo')
            settlement = rpc.call('getPalwSettlement', {'daaScore': dag['virtualDaaScore']})
            dns = rpc.call('getDnsConfirmation')
        finally:
            rpc.close()
        return dict(source='node-rpc', role=node['name'], dag=dag, value=dict(settlement=settlement, dns=dns))

    def reconcile(self, node, palw):
        fence, window, scan = self.plan['identity']['retirementActivationDaa'], self.plan['window'], self.plan['scan']
        rpc = self.rpc(node)
        try:
            to_daa = rpc.call('getBlockDagInfo')['virtualDaaScore']
            result = R.scan_window(rpc, self.plan['identity']['genesis'], window['scanFromDaa'], to_daa, scan['maximumChainBlocks'],
                                   R.ReadBudget(scan['maximumBlockReads']))
            budget = R.ReadBudget(scan['maximumStatusReads'])
            registry = R.bond_registry(rpc, scan['bondPageLimit'], scan['maximumBondPages'], budget)
            try:
                dns_scripts, unread = R.bond_scripts(result['bondTxs'], registry)
            except J.NotObservable as error:
                dns_scripts, unread = {}, [str(error)]
            blocks = R.classify_blocks(result['blocks'], dns_scripts, set(result['minerScripts']), palw)
            daas = {block['daa'] for block in blocks if block['daa'] < fence and any(o['kind'] == R.DNS for o in block['outputs'])}
            status = R.statuses_at(rpc, daas | {fence - 1}, scan['bondPageLimit'], scan['maximumBondPages'], budget)
        finally:
            rpc.close()
        name = node['name']
        self.judge('%s: coinbase outputs classified from node reads' % name, R.classification_complete, blocks)
        self.judge('%s: DNS share before the fence pays active bonds' % name, R.pre_fence_dns_share, blocks, fence, status)
        entitled = {bond for (bond, daa), value in status.items() if daa == fence - 1 and value == 'active'}
        entitled |= {o['bond'] for block in blocks if block['daa'] < fence for o in block['outputs'] if o['kind'] == R.DNS}
        self.judge('%s: no new DNS share or retired kind from the fence' % name, R.post_fence_dns_share, blocks,
                   result['bodies'], fence, window['legacySettlementWindowDaa'], entitled, window['dnsCadence'])
        self.evidence['nodes'][name] = dict(scanToDaa=to_daa, readsUsed=result['readsUsed'], registry=registry,
                                            dnsScripts=dns_scripts, bondsWithoutReadTransaction=unread, blocks=blocks,
                                            statusReads={'%s@%d' % key: value for key, value in status.items()})
        return blocks

    def run(self):
        try:
            manifest = self.identity()
            palw = keyring_addresses(manifest, self.plan['palwPayoutGroups'])
            for node in self.plan['nodes']:
                self.node_identity(node)
            per_node = {}
            for node in self.plan['nodes']:
                self.wait_past_fence(node)
                self.judge('%s: shipped ruleset retires DNS at the pinned DAA' % node['name'], shipped_retirement,
                           self.settlement_row(node), self.plan['identity'])
                if node['archival']:
                    per_node[node['name']] = self.reconcile(node, palw)
            self.judge('archival nodes classify shared blocks identically', classifications_agree, per_node)
            self.findings.append(J.judge(7, 'DNS reserve balance', R.reserve_balance, None))
        except (J.NotObservable, P.PlanError, OSError, RuntimeError) as error:
            self.findings.append(J.finding(7, 'drill reader', J.BLOCKED, str(error)))
        except J.Violation as error:
            self.findings.append(J.finding(7, 'drill reader', J.FAIL, str(error)))
        verdict = J.aggregate(self.findings, (7,))
        self.evidence.update(status=verdict['status'], checkboxes=verdict['checkboxes'])
        Path(self.out).write_text(json.dumps(self.evidence, indent=2, default=str) + '\n')
        return verdict['status']


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--plan', required=True)
    parser.add_argument('--out', required=True)
    args = parser.parse_args(argv)
    try:
        plan = parse_drill_plan(Path(args.plan).read_bytes())
    except (P.PlanError, OSError) as error:
        print(json.dumps(dict(status=J.BLOCKED, reason='plan refused: %s' % error, passLabelUsed=False)))
        return 2
    status = DrillReader(plan, args.out).run()
    print(json.dumps(dict(status=status, out=args.out, passLabelUsed=False)))
    return 0 if status == J.OBSERVED else 1


if __name__ == '__main__':
    raise SystemExit(main())
