#!/usr/bin/env python3
"""G14 collusion drill — real processes on a private salted testnet-12 drill chain.

The claim under test (docs/design/palw/g14-completeness-proof-2026-10-16.md §0): even when the producer and EVERY Panel seat collude,
ONE public bonded verifier outside the Panel reaches an objective conviction from public authenticated material alone, never the
producer's secret state; the conviction slashes the producer and every Valid signer, refuses the claim's reward / weight / consensus
credit, and stops the producer's further attempts.

Roles (every role its own process, its own fresh app dir under --work; loopback only; no DNS seeders; peers only by the driver's own
`addPeer` calls to the other drill nodes; a private network id — the drill genesis the salt moves):

  P    the colluding producer — genesis card 0 (judges nothing), `--palw-produce --palw-drill-tamper-leaf=L`: every attempt it makes
       commits an execution corrupted at one step leaf, its roots re-derived so the lie is self-consistent; heartbeat miner.
  S1.. the colluding Panel — genesis cards 1..seats, run INSIDE P's process as extra seat identities (`--palw-drill-extra-seat`,
       each its own Panel service, key, bond, fee float and state dir) with `--palw-drill-extra-seat-collude-with=<P's bonds>`: every
       seat signs Valid on P's claims without judging them and files nothing, while P's own service still answers what P owes.
       testnet-12 arms R-core+ at genesis and `validate_palw_v2` fixes the panel quorum at its colluding quorum (3), so the smallest
       Panel the shipped rules allow is 3-of-3; co-hosting keeps the drill within the host's limit of THREE node processes at once
       (P, O and one of P3 / F).
  O    the outside verifier — drill bond key 8, funded by the drill main wallet and registered by an ordinary BondRegistered AFTER
       genesis (`--palw-register-bond`), declaring no capability (never drawn as a seat); then started with that bond as an ordinary
       node. It runs only the node's own G14 automation (the common fraud filer, LG14-A/B) on public chain material and its own
       copy of the floor model.
  P3   (D-11) a second colluding producer — genesis card seats+1, `--palw-drill-tamper-auto-leaf --palw-drill-withhold-held-da`: it
       never answers a legacy held DA unit, so the public descent's demand ends in the objective DA default (P keeps running: it
       hosts the Panel).
  F    (IBD) a fresh relay with no bond, empty app dir, synced from the others (P3 stops first: three processes at most).

Injection phases (`--inject`, G14 audit G-17): P is restarted with one lie at a time — `--palw-drill-fault=<kind>` (output,
selection:<pos>:<lane>, oov:<pos>, tokenizer, prompt-relabel, short-prefill:<n>, garbage-prompt, skip-all, activation, interval:<n>,
bend:<row>[:same], leaf:<n>), `sparse:<seed>` (`--palw-drill-sparse-fault-seed`), or `withhold-all` (`--palw-drill-withhold-all-da`
with the auto lie) — and every lying claim accepted in the phase is followed through the metric chain
eligible → checked → detected → localized → filed → accepted → convicted / defaulted → collected → invalidated (audit §7), with
capacity refusals, unjudged cases and fused-leaf hand-offs counted separately. A lying claim that reaches Final with its reward is a
G14 miss and makes the phase FAIL.

Acceptance tests (audit §8): A, D, E, F, G run as node-level Rust tests (`--run-acceptance` runs them with cargo, targeted, -j 3);
B (per-family detection policy) and C (court capacity / TIR owner admission) are other lanes' — placeholder rows run their tests
once the lane has landed in HEAD, else BLOCKED naming the lane.

Rulesets (`misaka-rfc12-private-node --private-probe-profile=g14-v1`, feature `private-integration-config`; the shipped validation
still refuses both, and every gated fence is armed through drill arming records naming THIS drill genesis and height, never pinned):

  genesis   --mode genesis: every G14 fence at DAA 0 ("G14 from genesis on future networks", user decision 2026-10-16).
  cross     --mode cross --at H: testnet-12's shipped rules below H; at H the DAA-14,000 candidate set (option (b)
            palw_g14_route_required_v1, LG14-A, LG14-B, palw_tir_public_conviction_v1, the staged court with its prerequisites and
            extensions, N4, palw_reward_liability_horizon_v1).
  cross-b   --mode cross --at H --set route-required-only: option (b) alone at H, no route armed — every legacy-route family is
            refused past H, so the heartbeat lane alone must carry block production.

--family positive (user decision 2026-10-16, the fence moves past DAA 14,000): the same rulesets with an HONEST producer and an
HONEST Panel (no tamper, no collusion) and the outside verifier present — P-1 honest claims accepted on routed families, P-2 Final
and settlement / safe frontier growth, P-3 the reward paid after the liability-horizon hold (the delay in DAA recorded), P-4 block
production never stalls (heartbeat and PALW claims), P-5 rent per claim and time to Final. A row whose G14 route has not landed in
the build (the court-capacity lane for the V2 Attempt family; the IR Free Prompt lane and frame pricing for FP) is BLOCKED, with
what the build was observed to do attached as evidence.

Every verdict is read from public wRPC (JSON) of the running nodes and from their logs; nothing is approved here. A scenario row is
PASS only when every assertion of the row was OBSERVED; FAIL names what was observed instead; BLOCKED names the missing lane or the
step that could not be reached; NOT-INJECTABLE names the missing injection mechanism. Output: --out (JSON), one row per scenario.

No cryptographic computation proof of any kind is used or claimed (PRINCIPLES §8).
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from misaka_wrpc_json import WsRpc  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
SOMPI = 100_000_000
# Public testnet-12 listeners and the deploy kit's — a drill never binds one (the rcore drill's list).
PUBLIC_T12_PORTS = {26311, 26210, 27210, 28210, 8545, 26312, 26313, 26314, 26321, 26323, 26324, 26331, 26333, 26334, 26341,
                    26343, 26344, 26351, 26353, 26354}
TIR_FP_CLOSED_SENTINEL = 'TIR_FREE_PROMPT_CLOSED_V1'
# kaspad::private_integration::g14_drill_factory::PRIVATE_G14_DRILL_FLAG_DAYS_V1's last entry.
PRIVATE_G14_LAST_FLAG_DAY = 28


class Blocked(RuntimeError):
    pass


def now():
    return datetime.datetime.now().isoformat(timespec='seconds')


def log(msg):
    print('[g14-drill %s] %s' % (now(), msg), flush=True)


def norm(x):
    return x.removeprefix('0x').lower() if isinstance(x, str) else x


# =====================================================================================================================================
# Nodes
# =====================================================================================================================================

class Node:
    def __init__(self, drill, role, index, extra, bonded=True):
        self.drill, self.role, self.index, self.extra = drill, role, index, extra
        base = drill.args.port_base + 10 * index
        self.p2p, self.grpc, self.borsh, self.json, self.evm = base, base + 1, base + 2, base + 3, base + 4
        for port in (self.p2p, self.grpc, self.borsh, self.json, self.evm):
            assert port not in PUBLIC_T12_PORTS, 'a public testnet-12 port'
        self.dir = drill.work / role
        self.appdir = self.dir / 'app'
        self.logfile = self.dir / 'node.log'
        self.proc = None
        self.starts = 0
        self.bonded = bonded

    def argv(self):
        a = self.drill.args
        argv = [
            str(a.private_node),
            '--private-probe-profile=g14-v1', '--private-probe-salt=%s' % a.salt,
            '--private-probe-g14-at=%d' % self.drill.at_daa, '--private-probe-g14-set=%s' % a.set,
            '--private-probe-panel-seats=%d' % a.seats,
            '--testnet', '--netsuffix=12', '--appdir=%s' % self.appdir,
            '--nodnsseed', '--outpeers=0', '--disable-upnp',
            '--listen=127.0.0.1:%d' % self.p2p, '--rpclisten=127.0.0.1:%d' % self.grpc,
            '--rpclisten-borsh=127.0.0.1:%d' % self.borsh, '--rpclisten-json=127.0.0.1:%d' % self.json,
            '--evm-rpc-listen=127.0.0.1:%d' % self.evm,
            '--utxoindex', '--enable-unsynced-mining', '--unsaferpc', '--yes',
        ]
        return argv + list(self.extra)

    def start(self):
        self.dir.mkdir(parents=True, exist_ok=True)
        with_marker = (self.appdir / 'rfc12-private-probe-v1.json').exists()
        if not with_marker and self.appdir.exists() and any(self.appdir.iterdir()):
            raise Blocked('%s: app dir not empty and not a pinned private probe' % self.role)
        out = open(self.logfile, 'a')
        out.write('\n===== start %d at %s: %s\n' % (self.starts, now(), ' '.join(self.argv())))
        out.flush()
        self.proc = subprocess.Popen(self.argv(), stdout=out, stderr=subprocess.STDOUT, cwd=str(self.dir),
                                     start_new_session=True)
        self.starts += 1
        log('%s started pid %d (p2p %d, json %d)' % (self.role, self.proc.pid, self.p2p, self.json))

    def stop(self, timeout=60):
        if not self.proc:
            return
        if self.proc.poll() is None:
            try:
                os.killpg(self.proc.pid, signal.SIGINT)
            except ProcessLookupError:
                pass
            try:
                self.proc.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                os.killpg(self.proc.pid, signal.SIGKILL)
                self.proc.wait(timeout=30)
        log('%s stopped (exit %s)' % (self.role, self.proc.returncode))
        self.proc = None

    def alive(self):
        return self.proc is not None and self.proc.poll() is None

    def rpc(self, method, params=None, timeout=60):
        c = WsRpc(port=self.json, timeout=timeout)
        try:
            return c.call(method, params or {})
        finally:
            c.close()

    def log_text(self):
        try:
            return self.logfile.read_text(errors='replace')
        except FileNotFoundError:
            return ''

    def log_count(self, pattern):
        return len(re.findall(pattern, self.log_text()))


# =====================================================================================================================================
# The drill
# =====================================================================================================================================

class Drill:
    def __init__(self, args):
        self.args = args
        self.work = Path(args.work).resolve()
        self.at_daa = 0 if args.mode == 'genesis' else args.at
        self.nodes = {}
        self.rows = []
        self.facts = {'startedAt': now(), 'mode': args.mode, 'atDaa': self.at_daa, 'set': args.set, 'seats': args.seats,
                      'tamperLeaf': args.tamper_leaf, 'observations': []}
        self.keyring = None

    # ---- bookkeeping -------------------------------------------------------------------------------------------------------------

    def row(self, scenario, status, evidence, note=''):
        assert status in ('PASS', 'FAIL', 'BLOCKED', 'NOT-INJECTABLE', 'NOT-RUN', 'OBSERVED'), status
        r = {'scenario': scenario, 'status': status, 'evidence': evidence, 'note': note, 'at': now()}
        self.rows.append(r)
        log('ROW %s %s — %s' % (scenario, status, note or json.dumps(evidence)[:300]))
        self.flush()
        return r

    def observe(self, label, value):
        self.facts['observations'].append({'label': label, 'at': now(), 'value': value})
        self.flush()

    def flush(self):
        out = {'kind': 'misaka-g14-collusion-drill/v1', 'facts': self.facts, 'rows': self.rows,
               'releaseApproval': False, 'note': 'an observation record of a private drill chain, not an approval'}
        Path(self.args.out).write_text(json.dumps(out, indent=2, default=str) + '\n')

    # ---- preflight ---------------------------------------------------------------------------------------------------------------

    def preflight(self):
        a = self.args
        for b in (a.private_node, a.kaspad, a.cli):
            if not Path(b).is_file() or not os.access(b, os.X_OK):
                raise Blocked('missing binary %s (cargo build --release -p kaspad -p misaka-cli --features '
                              'kaspad/private-integration-config --bin kaspad --bin misaka-rfc12-private-node --bin misaka)' % b)
        helptext = subprocess.run([a.private_node, '--help'], capture_output=True, text=True).stdout
        if 'g14-v1' not in helptext:
            raise Blocked('this private node has no g14-v1 profile')
        khelp = subprocess.run([a.kaspad, '--help'], capture_output=True, text=True).stdout
        for flag in ('--palw-drill-collude-with', '--palw-drill-withhold-held-da', '--palw-drill-tamper-leaf', '--palw-drill-tamper-auto-leaf',
                     '--palw-drill-extra-seat', '--palw-drill-extra-seat-collude-with', '--palw-drill-fault',
                     '--palw-drill-sparse-fault-seed', '--palw-drill-withhold-all-da', '--palw-drill-fp-fault'):
            if flag not in khelp:
                raise Blocked('this kaspad has no %s' % flag)
        self.facts['binaries'] = {str(b): hashlib.sha256(Path(b).read_bytes()).hexdigest() for b in (a.private_node, a.kaspad, a.cli)}
        self.facts['sourceCommit'] = subprocess.run(['git', '-C', str(REPO), 'rev-parse', 'HEAD'], capture_output=True,
                                                    text=True).stdout.strip()
        self.facts['sourceDirty'] = bool(subprocess.run(['git', '-C', str(REPO), 'status', '--porcelain'], capture_output=True,
                                                        text=True).stdout.strip())
        # The IR (TIR) class Free Prompt lane: frames (D-05/06/07/18) exist only once it is open (g14 proof §7.3).
        backend = REPO / 'misaka/palw/tir/exec/src/node/backend.rs'
        closed = backend.exists() and TIR_FP_CLOSED_SENTINEL in backend.read_text(errors='replace')
        branch = subprocess.run(['git', '-C', str(REPO), 'branch', '-a', '--list', '*rfc02-ir-free-prompt*'], capture_output=True,
                                text=True).stdout.strip()
        self.facts['irFreePromptLane'] = {'tirBackendClosedSentinel': closed, 'branchesSeen': branch}
        self.ir_fp_open = not closed
        if self.work.exists() and any(self.work.iterdir()):
            raise Blocked('--work %s is not empty: a drill starts from fresh app dirs' % self.work)
        self.work.mkdir(parents=True, exist_ok=True)
        free = shutil.disk_usage(self.work).free
        self.facts['diskFreeGiB'] = round(free / 2**30, 1)
        for i in range(10):
            for port in range(a.port_base + 10 * i, a.port_base + 10 * i + 5):
                with socket.socket() as s:
                    if s.connect_ex(('127.0.0.1', port)) == 0:
                        raise Blocked('port %d is in use' % port)

    def write_keyring(self):
        a = self.args
        d = self.work / 'keyring'
        subprocess.run([a.kaspad, '--testnet', '--netsuffix=12', '--palw-drill-genesis-salt=%s' % a.salt,
                        '--palw-drill-write-keyring=%s' % d, '--appdir=%s' % (self.work / 'keyring-app')],
                       check=True, capture_output=True, text=True)
        self.keyring = json.loads((d / 'manifest.json').read_text())
        self.kdir = d
        # The drill's own ruleset identity (print mode builds the factory ruleset; it refuses if validation fails).
        p = subprocess.run([a.private_node, '--private-probe-profile=g14-v1', '--private-probe-salt=%s' % a.salt,
                            '--private-probe-g14-at=%d' % self.at_daa, '--private-probe-g14-set=%s' % a.set,
                            '--private-probe-panel-seats=%d' % a.seats, '--print-private-probe-profile',
                            '--testnet', '--netsuffix=12'], capture_output=True, text=True)
        if p.returncode:
            raise Blocked('the g14-v1 ruleset does not build/validate: %s' % (p.stderr or p.stdout)[-2000:])
        self.profile = json.loads(p.stdout)
        if norm(self.profile['genesis']) != norm(self.keyring['genesis_hash']):
            raise Blocked('the private profile genesis is not the keyring drill genesis')
        self.observe('profile', self.profile)
        log('drill ruleset %s… genesis %s… records %s' % (
            self.profile['consensusParamsId'][:16], self.profile['genesis'][:16], self.profile.get('g14Drill', {}).get('drillRecords')))

    def seat(self, n):
        return self.keyring['seats'][n]

    def key(self, rel):
        return str(self.kdir / rel)

    # ---- node construction -------------------------------------------------------------------------------------------------------

    def producer_flags(self, card, withhold=False, canonical=False, inject=None, host_panel=False, honest=False):
        s = self.seat(card)
        flags = ['--palw-produce', '--palw-producer-key=%s' % self.key(s['seed_file']), '--palw-producer-bond=%s' % s['bond_outpoint'],
                 '--palw-producer-pay-address=%s' % s['address'], '--palw-fee-outpoint=%s' % s['fee_float_outpoint'],
                 '--palw-heartbeat-miner-address=%s' % self.keyring['heartbeat'][card]['address']]
        if self.args.family == 'collusion' and not honest:
            if inject and inject.startswith('sparse:'):
                flags.append('--palw-drill-sparse-fault-seed=%d' % int(inject.split(':', 1)[1]))
            elif inject and inject != 'withhold-all':
                flags.append('--palw-drill-fault=%s' % inject)
            else:
                flags.append(('--palw-drill-tamper-leaf=%d' % self.args.tamper_leaf) if self.args.tamper_leaf is not None
                             else '--palw-drill-tamper-auto-leaf')
            if inject == 'withhold-all':
                flags.append('--palw-drill-withhold-all-da')
        if host_panel:
            flags += self.extra_seat_flags()
        if withhold:
            flags.append('--palw-drill-withhold-held-da')
        if canonical:
            flags += ['--palw-canonical-claims', '--palw-canonical-interval-daa=%d' % self.args.canonical_interval]
        return flags

    def extra_seat_flags(self):
        """The Panel cards 1..seats as extra seat identities of P's process; colluding with P and P3 in the collusion family."""
        flags = []
        for card in range(1, self.args.seats + 1):
            s = self.seat(card)
            flags.append('--palw-drill-extra-seat=key=%s,bond=%s,fee=%s,pay=%s' % (
                self.key(s['seed_file']), s['bond_outpoint'], s['fee_float_outpoint'], s['address']))
        if self.args.family == 'collusion':
            flags.append('--palw-drill-extra-seat-collude-with=%s' % ','.join(
                self.seat(c)['bond_outpoint'] for c in self.colluding_producers()))
        return flags

    def colluding_producers(self):
        # P (card 0), P3 (D-11, card seats+1) and the injection producers (cards seats+2..): every card outside the Panel.
        return [0] + list(range(self.args.seats + 1, len(self.keyring['seats'])))

    def add(self, role, index, flags, bonded=True):
        n = Node(self, role, index, flags, bonded)
        self.nodes[role] = n
        return n

    def peer_all(self):
        live = [n for n in self.nodes.values() if n.alive()]
        for a in live:
            for b in live:
                if a is not b:
                    try:
                        a.rpc('addPeer', {'peerAddress': {'ip': '127.0.0.1', 'port': b.p2p}, 'isPermanent': True})
                    except Exception as e:  # noqa: BLE001 — a refused duplicate is fine
                        if 'already' not in str(e).lower():
                            log('addPeer %s→%s: %s' % (a.role, b.role, e))

    # ---- waiting -----------------------------------------------------------------------------------------------------------------

    def check_alive(self):
        for n in self.nodes.values():
            if n.proc is not None and n.proc.poll() is not None:
                tail = n.log_text()[-3000:]
                raise Blocked('%s exited %s; log tail: %s' % (n.role, n.proc.returncode, tail))

    def wait(self, label, predicate, timeout_s, every=10, stall_s=None):
        deadline = time.monotonic() + timeout_s
        last_err, last_daa, moved = None, None, time.monotonic()
        stall_s = stall_s or self.args.stall_s
        while time.monotonic() < deadline:
            self.check_alive()
            try:
                got = predicate()
                if got:
                    return got
            except Blocked:
                raise
            except Exception as e:  # noqa: BLE001 — retried; the last one is reported on timeout
                last_err = repr(e)[:400]
            daa = self.daa_quiet()
            if daa is not None and daa != last_daa:
                last_daa, moved = daa, time.monotonic()
            if time.monotonic() - moved > stall_s:
                raise Blocked('%s: the chain DAA did not move for %d s (DAA %s; last error %s)' % (label, stall_s, last_daa, last_err))
            time.sleep(every)
        raise Blocked('%s: timed out after %d s (DAA %s; last error %s)' % (label, timeout_s, last_daa, last_err))

    def ref(self):
        for role in ('O', 'P', 'R', 'P3', 'F'):
            n = self.nodes.get(role)
            if n and n.alive():
                return n
        raise Blocked('no live node')

    def daa_quiet(self):
        try:
            return self.ref().rpc('getBlockDagInfo', timeout=15)['virtualDaaScore']
        except Exception:  # noqa: BLE001
            return None

    def dag(self, node=None):
        return (node or self.ref()).rpc('getBlockDagInfo')

    def claims(self, bond, role='executor', node=None):
        return (node or self.ref()).rpc('getPalwClaims', {'bond': bond, 'role': role, 'includeTerminal': True, 'limit': 256})

    def dispute(self, claim, node=None):
        r = (node or self.ref()).rpc('getPalwLegacyDispute', {'claimId': claim})
        return json.loads(r['json']) if r.get('available') and r.get('json') else None

    def status(self, node):
        return node.rpc('getPalwNodeStatus')

    def bond_row(self, bond, node=None):
        c = self.claims(bond, node=node)
        return {k: c.get(k) for k in ('bondKnown', 'bondCollateral', 'bondSlashed', 'bondRetiringSinceDaa', 'bondLiveLockCount',
                                       'bondLiveLockedSompi', 'bondRetireRefusedWhileLocked', 'bondCapableClasses', 'bondRegisteredDaa')}

    def claim_row(self, bond, claim, node=None):
        for r in self.claims(bond, node=node).get('claims', []):
            if norm(r['claimId']) == norm(claim):
                return r
        return None

    def wait_for_chain(self, nodes, min_daa=1, timeout_s=3600):
        def ok():
            daas = [n.rpc('getBlockDagInfo')['virtualDaaScore'] for n in nodes]
            return min(daas) >= min_daa and daas
        return self.wait('the chain advances to DAA %d on %s' % (min_daa, [n.role for n in nodes]), ok, timeout_s,
                         stall_s=timeout_s)

    def wait_rpc(self, node, timeout_s=600):
        return self.wait('%s RPC' % node.role, lambda: node.rpc('getBlockDagInfo', timeout=10), timeout_s, every=3,
                         stall_s=timeout_s)

    def wait_synced(self, nodes, timeout_s=1800):
        def same():
            sinks = {n.rpc('getBlockDagInfo')['sink'] for n in nodes}
            return len(sinks) == 1 and sinks.pop()
        return self.wait('one sink on %s' % [n.role for n in nodes], same, timeout_s, every=5)

    # ---- the outside verifier ----------------------------------------------------------------------------------------------------

    def fund_and_register_outsider(self):
        """Fund drill bond key 8 from the drill main wallet (CLI, salted), then register its bond by an ordinary BondRegistered."""
        o_key = self.keyring['bonds'][0]
        assert o_key['n'] == len(self.keyring['seats']), 'the first drill-only bond key follows the cards'
        p = self.nodes['P']
        cli = [self.args.cli, '--network', 'testnet-12', '--rpc', '127.0.0.1:%d' % p.borsh, '--palw-drill-genesis-salt=%s' % self.args.salt]
        send = subprocess.run(cli + ['wallet', 'send', '--to', o_key['address'], '--amount', str(self.args.outsider_funding_bili),
                                     '--yes', '--key-file', self.key(self.keyring['main']['seed_file'])],
                              capture_output=True, text=True, timeout=300)
        self.observe('outsider funding (CLI wallet send)', {'rc': send.returncode, 'out': (send.stdout + send.stderr)[-2000:]})
        if send.returncode:
            raise Blocked('funding the outside verifier failed: %s' % (send.stdout + send.stderr)[-1500:])

        def funded():
            r = p.rpc('getBalanceByAddress', {'address': o_key['address']})
            return r.get('balance', 0) > 0 and r

        self.wait('the outside verifier funding confirms', funded, 3600)
        o = self.add('O', 7, ['--palw-producer-key=%s' % self.key(o_key['seed_file']), '--palw-producer-pay-address=%s' % o_key['address'],
                              '--palw-register-bond'] + (['--palw-bond-collateral=%d' % (self.args.outsider_collateral_bili * SOMPI)]
                                                         if self.args.outsider_collateral_bili else []))
        o.start()
        self.wait_rpc(o)
        self.peer_all()
        m = self.wait('the outsider bond registers (BondRegistered after genesis)',
                      lambda: re.search(r'registered bond ([0-9a-f]{128}):(\d+) with (\d+) sompi of collateral, in tx ([0-9a-f]{128})',
                                        o.log_text()), 3600)
        bond = '%s:%s' % (m.group(1), m.group(2))
        fee = '%s:1' % m.group(4)
        o.stop()
        o.extra = ['--palw-producer-key=%s' % self.key(o_key['seed_file']), '--palw-producer-pay-address=%s' % o_key['address'],
                   '--palw-producer-bond=%s' % bond, '--palw-fee-outpoint=%s' % fee]
        o.start()
        self.wait_rpc(o)
        self.peer_all()
        if 'R' in self.nodes and self.nodes['R'].alive():
            self.nodes['R'].stop()
        row = self.bond_row(bond)
        self.facts['outsider'] = {'bond': bond, 'address': o_key['address'], 'collateralSompi': int(m.group(3)),
                                  'registeredDaa': row.get('bondRegisteredDaa'), 'capableClasses': row.get('bondCapableClasses')}
        self.observe('outsider bond', self.facts['outsider'])
        return bond

    # ---- the collusion scenario --------------------------------------------------------------------------------------------------

    def lying_claim(self, producer_bond, after_daa, label, timeout_s):
        """The earliest claim of `producer_bond` accepted at or after `after_daa` that the outside verifier has RESERVED (LG14-A) — the
        producer lies on every draw, so the outsider's own choice decides which claim is pursued first; the driver follows it."""
        obond = self.facts['outsider']['bond']

        def first():
            reserved = self.ref().rpc('getPalwLegacyDisputes', {'reserver': obond, 'limit': 0})
            ids = {norm(x) for x in reserved.get('claimIds') or []}
            rows = [r for r in self.claims(producer_bond).get('claims', [])
                    if norm(r['claimId']) in ids and r['acceptedDaa'] >= after_daa and not r['isFreePrompt']]
            rows.sort(key=lambda r: r['acceptedDaa'])
            return rows[0] if rows else None
        return self.wait(label, first, timeout_s)

    def run_conviction(self, scenario, producer_role, producer_card, after_daa, expect='court'):
        """D-02 (expect='court') or D-11 (expect='da-default'): observe the full collusion path on one lying claim."""
        pbond = self.seat(producer_card)['bond_outpoint']
        seats = [self.seat(c)['bond_outpoint'] for c in range(1, self.args.seats + 1)]
        obond = self.facts['outsider']['bond']
        ev = {'producer': pbond, 'colludingSeats': seats, 'outsider': obond}
        claim = self.lying_claim(pbond, after_daa, '%s: a claim of the colluding producer' % scenario, self.args.step_timeout)
        cid = claim['claimId']
        ev['claim'] = cid
        ev['acceptedDaa'] = claim['acceptedDaa']
        before = {'producer': self.bond_row(pbond), 'seats': {s: self.bond_row(s) for s in seats}, 'outsider': self.bond_row(obond)}
        ev['before'] = before
        # 1. the colluding Panel licenses it: every seat on it is a colluder, every seat signs Valid.
        lic = self.wait('%s: the colluding Panel licenses claim %s…' % (scenario, cid[:16]),
                        lambda: (lambda r: r if r and r['phase'] in ('receipt_licensed', 'final') or (r and r['phase'] == 'voided') else None)(
                            self.claim_row(pbond, cid)), self.args.step_timeout)
        ev['licence'] = {k: lic.get(k) for k in ('phase', 'phaseDaa', 'boundDaa', 'seats', 'voidReason', 'escrowSompi', 'workLeaves')}
        panel = {norm(x) for x in (lic.get('seats') or [])}
        ev['panelAllColluding'] = bool(panel) and panel <= {norm(x) for x in seats}
        ev['outsiderNotOnPanel'] = norm(obond) not in panel
        # 2. the outsider reserves (LG14-A) and the claim's Final is held while the pursuit runs.
        res = self.wait('%s: the outsider reserves the claim (tag 154)' % scenario,
                        lambda: (lambda d: d if d and (d.get('reservations') or d.get('courtConvicted') or d.get('daDefaulted')) else None)(
                            self.dispute(cid)), self.args.step_timeout)
        ev['reservationFirstSeen'] = {k: res.get(k) for k in ('phase', 'held', 'reservations', 'deadlineDaa', 'hardDeadlineDaa')}
        # D-16 (exit before the sanction): once licensed and before the end, a Valid signer (D-02) or the producer (D-11) asks to
        # retire. D-02's producer does not, so its own "further attempts refused" can only be the freeze, never a retirement.
        if self.args.d16 and scenario == 'D-02':
            self.d16_exit_attempt(scenario, 'seat-1', 1, seats[0], cid, pbond)
        if self.args.d16 and scenario == 'D-11':
            self.d16_exit_attempt(scenario, 'producer', producer_card, pbond, cid, pbond)
        # 3. the end: an objective conviction (court) or the objective DA default.
        def ended():
            d = self.dispute(cid)
            r = self.claim_row(pbond, cid)
            if r and r['phase'] == 'voided':
                return {'dispute': d, 'row': r}
            if r and r['phase'] == 'final':
                return {'dispute': d, 'row': r, 'final': True}
            return None
        held_samples = []
        seen = {'reservers': set(), 'sessionAccusers': set(), 'sessionUnits': set(), 'closed': set(), 'samples': 0}
        for (bond, *_rest) in res.get('reservations') or []:
            seen['reservers'].add(norm(bond))

        def ended_sampling():
            d = self.dispute(cid)
            if d:
                seen['samples'] += 1
                for (bond, *_rest) in d.get('reservations') or []:
                    seen['reservers'].add(norm(bond))
                for (accuser, _o, _dl, is_seat, _x, units) in d.get('sessions') or []:
                    seen['sessionAccusers'].add((norm(accuser), bool(is_seat)))
                    seen['sessionUnits'].update(units)
                seen['closed'].update(norm(b) for b in d.get('closed') or [])
            got = ended()
            if not got:
                r = self.claim_row(pbond, cid)
                daa = self.daa_quiet()
                if r and r['phase'] == 'receipt_licensed' and daa is not None:
                    held_samples.append((daa, r['phaseDaa'], r.get('deadlineDaa')))
            return got
        end = self.wait('%s: the claim ends (conviction / default)' % scenario, ended_sampling, self.args.conviction_timeout, every=30)
        ev['end'] = end
        ev['heldWhileLicensedSamples'] = held_samples[-5:]
        if end.get('final'):
            ev['finalReachedWithoutConviction'] = True
            return self.row(scenario, 'FAIL', ev, 'the claim reached Final with no conviction/default: the outsider did not complete')
        r, d = end['row'], end['dispute'] or {}
        ev['voidReason'] = r['voidReason']
        ev['disputeEnd'] = {k: d.get(k) for k in ('courtConvicted', 'daDefaulted', 'executorRefuted', 'reservations', 'sessions', 'closed')}
        # 4. slashes: the producer, and (court) every Valid signer through the kind-3 filings that follow.
        time.sleep(5)
        settle_deadline = time.monotonic() + self.args.filing_timeout

        def charged():
            p = self.bond_row(pbond)
            s = {x: self.bond_row(x) for x in seats}
            done = p['bondSlashed'] > before['producer']['bondSlashed'] and (
                expect != 'court' or all(s[x]['bondSlashed'] > before['seats'][x]['bondSlashed'] for x in seats))
            return (p, s) if done else None
        try:
            p_after, s_after = self.wait('%s: the producer%s are charged' % (scenario, ' and every Valid signer' if expect == 'court' else ''),
                                         charged, max(60, settle_deadline - time.monotonic()), every=20)
        except Blocked as e:
            p_after, s_after = self.bond_row(pbond), {x: self.bond_row(x) for x in seats}
            ev['chargeWaitEnded'] = str(e)
        o_after = self.bond_row(obond)
        ev['after'] = {'producer': p_after, 'seats': s_after, 'outsider': o_after}
        ev['slashedSompi'] = {'producer': p_after['bondSlashed'] - before['producer']['bondSlashed'],
                              'seats': {x: s_after[x]['bondSlashed'] - before['seats'][x]['bondSlashed'] for x in seats},
                              'outsiderSlashed': o_after['bondSlashed'] - before['outsider']['bondSlashed']}
        # 5. who filed: the outsider alone (its reservations / sessions, sampled through the pursuit), and no colluding seat
        #    submitted anything against the claim.
        accusers = seen['reservers'] | {a for a, _ in seen['sessionAccusers']} | seen['closed']
        ev['accusers'] = sorted(accusers)
        ev['pursuit'] = {'disputeSamples': seen['samples'], 'sessionAccusers': sorted(seen['sessionAccusers']),
                         'units': sorted(seen['sessionUnits'])[:64], 'unitCount': len(seen['sessionUnits'])}
        try:
            ev['outsiderFilerStatus'] = json.loads(self.nodes['O'].rpc('getPalwFraudFilerStatus', {'bond': obond}).get('json') or 'null')
        except Exception as e:  # noqa: BLE001
            ev['outsiderFilerStatus'] = repr(e)
        seat_filings = {role: self.nodes[role].log_count(r'submitted .*(ShardCourtAccused|DefaultAccused|PanelFalseValid|ReporterCommitted)')
                        for role in self.seat_roles()}
        seat_filtered = {role: self.nodes[role].log_count(r'PALW DRILL \(G14 collusion\): this colluding seat files no') for role in self.seat_roles()}
        ev['seatFilingLogLines'] = seat_filings
        ev['seatFilterLogLines'] = seat_filtered
        # 6. reward / weight refused.
        final_row = self.claim_row(pbond, cid)
        ev['claimAfter'] = {k: final_row.get(k) for k in ('phase', 'voidReason', 'escrowSompi', 'payoutPendingSompi', 'vestingStage',
                                                         'vestingSompi', 'committedSompi', 'workLeaves', 'quantaSpent')} if final_row else None
        # 7. the producer stops: a frozen bond produces nothing further (ProducerFrozen).
        pnode = self.nodes[producer_role]
        end_daa = (final_row or {}).get('phaseDaa', 1 << 62)

        def frozen_seen():
            st = self.status(pnode)
            reason = '%s %s' % (st.get('producerState'), st.get('producerReason'))
            logs = pnode.log_count(r'frozen by a conviction|ProducerFrozen')
            return (st, logs) if ('frozen by a conviction' in reason or logs) else None
        if expect == 'court':
            try:
                st, logs = self.wait('%s: the producer is refused as frozen (ProducerFrozen)' % scenario, frozen_seen,
                                     self.args.frozen_timeout, every=20)
                ev['producerFrozenObserved'] = True
            except Blocked as e:
                st, logs = self.status(pnode), pnode.log_count(r'frozen by a conviction|ProducerFrozen')
                ev['producerFrozenObserved'] = False
                ev['producerFrozenWaitEnded'] = str(e)
        else:
            st, logs = self.status(pnode), pnode.log_count(r'frozen by a conviction|ProducerFrozen')
            ev['producerFrozenObserved'] = bool(logs)
        ev['producerStatus'] = {k: st.get(k) for k in ('producerState', 'producerReason', 'producedBlocks', 'draws')}
        ev['producerFrozenLogLines'] = logs
        later = [x for x in self.claims(pbond).get('claims', []) if x['acceptedDaa'] > end_daa]
        ev['producerClaimsAcceptedAfterTheEnd'] = len(later)
        # verdict
        failures = []
        if not ev['panelAllColluding']:
            failures.append('a non-colluding seat sat on the Panel')
        if not ev['outsiderNotOnPanel']:
            failures.append('the outsider was on the Panel')
        if expect == 'court':
            if not (r['voidReason'] in ('court_fraud', 'court_held_verdict', 'court_default') and ev['disputeEnd'].get('courtConvicted')):
                failures.append('not an objective court conviction (void %s, convicted %s)' % (r['voidReason'], ev['disputeEnd'].get('courtConvicted')))
            if ev['slashedSompi']['producer'] <= 0:
                failures.append('the producer was not slashed')
            unslashed = [x for x, v in ev['slashedSompi']['seats'].items() if v <= 0]
            if unslashed:
                failures.append('Valid signers not slashed: %s' % unslashed)
        else:
            if not (r['voidReason'] == 'producer_withholding' and ev['disputeEnd'].get('daDefaulted') and not ev['disputeEnd'].get('courtConvicted')):
                failures.append('not the objective DA default (void %s, defaulted %s, convicted %s)' % (
                    r['voidReason'], ev['disputeEnd'].get('daDefaulted'), ev['disputeEnd'].get('courtConvicted')))
            if ev['slashedSompi']['producer'] <= 0:
                failures.append('the withholding producer was not charged')
            charged_seats = [x for x, v in ev['slashedSompi']['seats'].items() if v > 0]
            if charged_seats:
                failures.append('seats charged for a held unit they never owed: %s' % charged_seats)
        if not accusers:
            failures.append('no reservation or session of the outsider was observed')
        elif norm(obond) not in accusers:
            failures.append('the outsider never reserved / demanded: %s' % sorted(accusers))
        elif accusers != {norm(obond)}:
            # P's own (honest-code) service may pursue P3's claims in D-11 (it shares no Panel seat with them); any other party fails.
            others = accusers - {norm(obond)}
            allowed = {norm(self.seat(0)['bond_outpoint'])} if scenario == 'D-11' else set()
            if others - allowed:
                failures.append('a party other than the outsider reserved / demanded: %s' % sorted(accusers))
            else:
                ev['alsoPursuedBy'] = sorted(others)
        if any(seat_filings.values()):
            failures.append('a colluding seat submitted a filing: %s' % seat_filings)
        if ev['slashedSompi']['outsiderSlashed'] > 0:
            failures.append('the honest outsider was charged %d sompi' % ev['slashedSompi']['outsiderSlashed'])
        if ev['claimAfter'] and (ev['claimAfter'].get('payoutPendingSompi') or ev['claimAfter'].get('vestingSompi')):
            failures.append('the void claim still carries a payout / vesting leg')
        if expect == 'court' and ev['producerClaimsAcceptedAfterTheEnd']:
            failures.append('the producer had %d claims accepted after the conviction' % ev['producerClaimsAcceptedAfterTheEnd'])
        if expect == 'court' and not ev['producerFrozenObserved']:
            failures.append('ProducerFrozen was not observed after the conviction')
        status = 'FAIL' if failures else 'PASS'
        return self.row(scenario, status, ev, '; '.join(failures) if failures else
                        '%s by the outsider alone; producer slashed %d sompi; seats %s' % (
                            'convicted' if expect == 'court' else 'DA-defaulted', ev['slashedSompi']['producer'], ev['slashedSompi']['seats']))

    def seat_roles(self):
        # The colluding seats run inside P's process (`--palw-drill-extra-seat`): their filter lines and any filing are in P's log.
        return ['P']

    def d16_exit_attempt(self, scenario, who, card, bond, cid, pbond):
        """D-16: a liable bond asks to retire after the licence, before the sanction (CLI `bond retire`, salted)."""
        self.d16 = getattr(self, 'd16', None) or {'attempts': []}
        ref = self.nodes['O']
        class_id = self.claim_row(pbond, cid)['classId']
        cli = [self.args.cli, '--network', 'testnet-12', '--rpc', '127.0.0.1:%d' % ref.borsh,
               '--palw-drill-genesis-salt=%s' % self.args.salt, 'bond', 'retire', '--key-file', self.key(self.seat(card)['seed_file']),
               '--bond', bond, '--class-id', class_id, '--yes']
        before = self.bond_row(bond)
        p = subprocess.run(cli, capture_output=True, text=True, timeout=300)
        time.sleep(30)
        self.d16['attempts'].append({'scenario': scenario, 'who': who, 'bond': bond, 'claim': cid, 'rc': p.returncode,
                                     'out': (p.stdout + p.stderr)[-1500:], 'bondBefore': before, 'bondAfterRequest': self.bond_row(bond)})
        self.observe('D-16 retirement request', self.d16['attempts'][-1])

    def d16_row(self, results):
        if not getattr(self, 'd16', None) or not self.d16['attempts']:
            return self.row('D-16', 'BLOCKED', {}, 'no exit attempt ran (no claim reached its licence)')
        ev = {'attempts': self.d16['attempts'], 'bondsNow': {a['bond']: self.bond_row(a['bond']) for a in self.d16['attempts']}}
        failures, notes = [], []
        for a in self.d16['attempts']:
            r = results.get(a['scenario'])
            if not r:
                failures.append('%s: its scenario %s has no result' % (a['who'], a['scenario']))
                continue
            slashed = r['evidence'].get('slashedSompi') or {}
            amount = slashed.get('producer') if a['who'] == 'producer' else (slashed.get('seats') or {}).get(a['bond'])
            if not amount or amount <= 0:
                failures.append('%s (%s) escaped the sanction after asking to retire (charged %s)' % (a['who'], a['scenario'], amount))
            else:
                notes.append('%s asked to retire before the sanction and was still charged %d sompi (%s)' % (a['who'], amount, a['scenario']))
        return self.row('D-16', 'FAIL' if failures else 'PASS', ev, '; '.join(failures or notes))

    def d15_row(self, d02):
        """D-15: a self-accusation (the producer's Sybil as the filer) nets negative: the finder's share is at most 490 per mille of
        the slash. Read from the D-02 conviction: the outsider's collateral / balance gain against the producer's slash."""
        if not d02 or d02['status'] != 'PASS':
            return self.row('D-15', 'BLOCKED', {}, 'needs a D-02 conviction to measure the finder share')
        ev = d02['evidence']
        slashed = ev['slashedSompi']['producer']
        o_before, o_after = ev['before']['outsider'], ev['after']['outsider']
        gain_collateral = o_after['bondCollateral'] - o_before['bondCollateral']
        try:
            bal = self.ref().rpc('getBalanceByAddress', {'address': self.facts['outsider']['address']}).get('balance')
        except Exception as e:  # noqa: BLE001
            bal = repr(e)
        out = {'producerSlashedSompi': slashed, 'outsiderCollateralGainSompi': gain_collateral, 'outsiderAddressBalanceNow': bal,
               'boundPermille': 490}
        if slashed <= 0:
            return self.row('D-15', 'BLOCKED', out, 'no slash observed')
        share = gain_collateral / slashed if gain_collateral > 0 else 0.0
        out['finderShareObservedPermille'] = round(1000 * share, 1)
        if gain_collateral <= 0:
            return self.row('D-15', 'OBSERVED', out, 'the finder share is not paid into the bond collateral; the payout leg was not '
                            'isolated from RPC — net of a self-accusation = -(slash) + share, share unmeasured here')
        return self.row('D-15', 'PASS' if share <= 0.490 else 'FAIL', out,
                        'finder share %.1f permille of the slash (bound 490)' % (1000 * share))

    # ---- restart / IBD ------------------------------------------------------------------------------------------------------------

    def snapshot(self, node, bonds, claims):
        return {'sink': node.rpc('getBlockDagInfo')['sink'], 'bonds': {b: self.bond_row(b, node=node) for b in bonds},
                'claims': {c: (lambda r: {k: r.get(k) for k in ('phase', 'voidReason', 'phaseDaa', 'escrowSompi', 'payoutPendingSompi',
                                                                 'vestingStage')} if r else None)(self.claim_row(b, c, node=node))
                           for b, c in claims}}

    def compare_nodes(self, label, nodes, bonds, claims):
        sink = self.wait_synced(nodes)
        snaps = {}
        for _ in range(5):
            snaps = {n.role: self.snapshot(n, bonds, claims) for n in nodes}
            if len({s['sink'] for s in snaps.values()}) == 1:
                break
            time.sleep(5)
        bodies = {json.dumps({k: v for k, v in s.items() if k != 'sink'}, sort_keys=True) for s in snaps.values()}
        return {'label': label, 'sink': sink, 'same': len(bodies) == 1, 'snapshots': snaps}

    def restart_and_ibd(self, bonds, claims):
        out = {}
        victim = self.nodes['O']
        victim.stop()
        time.sleep(5)
        victim.start()
        self.wait_rpc(victim)
        self.peer_all()
        out['restart'] = self.compare_nodes('restart of %s' % victim.role, [n for n in self.nodes.values() if n.alive()], bonds, claims)
        # IBD: a fresh relay with no bond as the third process (P3 stops first: the host runs three node processes at most).
        if 'P3' in self.nodes and self.nodes['P3'].alive():
            self.nodes['P3'].stop()
        f = self.nodes.get('F') or self.add('F', 9, [], bonded=False)
        if f.appdir.exists():
            shutil.rmtree(f.appdir)
        f.start()
        self.wait_rpc(f)
        self.peer_all()
        out['ibd'] = self.compare_nodes('fresh IBD node F', [n for n in self.nodes.values() if n.alive()], bonds, claims)
        f.stop()
        return out

    # ---- the scenario list ---------------------------------------------------------------------------------------------------------

    def static_rows(self):
        ir = 'the IR-class Free Prompt lane (RFC-0002 Phase H, integ/rfc02-ir-free-prompt) is not in this build: the TIR backend ' \
             'returns %s, so no TIR FP claim — and so no TFP1 frame — can exist (g14 proof §7.3)' % TIR_FP_CLOSED_SENTINEL
        for d in ('D-05', 'D-06', 'D-07', 'D-18'):
            if not self.ir_fp_open:
                self.row(d, 'BLOCKED', {'irFreePromptLane': self.facts['irFreePromptLane']}, ir)
        self.row('D-03', 'NOT-INJECTABLE', {},
                 '--palw-drill-tamper-boundary exists, but a shard boundary lie needs a registered IR (TIR) class with a layer-shard plan '
                 'and palw_tir_shard_v1 armed (RFC-0006, in no release and not in the DAA-14,000 candidate set); the drill ruleset arms '
                 'neither and the floor (BASE-0) class has no shards')
        self.row('D-04', 'NOT-INJECTABLE', {},
                 '--palw-drill-tamper-eval exists, but an evaluation claim needs a governed model line with an open evaluation epoch '
                 '(RFC-0004: a founded line, a published candidate, its evaluation jobs and their artifacts); the drill has no line '
                 'fixture, so no evaluation claim exists to lie in')
        # G-17 (2026-10-17): these are injectable now; a phase runs only when named with --inject.
        for d, kinds, what in (('D-06', ('selection',), 'selection violation (attempt lane: --palw-drill-fault=selection:<pos>:<lane>)'),
                               ('D-07', ('output',), 'output substitution (attempt lane: --palw-drill-fault=output)'),
                               ('D-08', ('prompt-relabel', 'short-prefill', 'garbage-prompt'),
                                'prompt / input substitution (--palw-drill-fault=prompt-relabel | short-prefill:<n> | garbage-prompt)'),
                               ('D-10', ('tokenizer',), 'tokenizer field in the context (--palw-drill-fault=tokenizer)'),
                               ('sparse', ('sparse',), 'sparse single-interval fault (--palw-drill-sparse-fault-seed)'),
                               ('skip-all', ('skip-all',), 'skip-all-compute (--palw-drill-fault=skip-all)')):
            if not any(i.split(':', 1)[0] in kinds for i in self.args.inject):
                self.row(d, 'NOT-RUN', {}, '%s is injectable; not selected in this run (--inject)' % what)
        self.row('D-09', 'BLOCKED', {}, 'KV / prefix-state forgery is injectable on canonical FP claims (--palw-drill-fp-fault=kv:...) '
                 'with a backend that has the FP drill verb (Qwen2.5 A16); frameless FP earns nothing past the fence under option (b) '
                 'and the framed (TFP1) FP lane is the IR Free Prompt lane / frame pricing (integ/g14-tir-pricing)')
        self.row('D-12', 'NOT-INJECTABLE', {}, 'no tampered-carrier / borrowed-trace fixture exists (D-12)')
        self.row('D-13', 'NOT-INJECTABLE', {}, 'no Sybil slot-squatting fixture exists (reservation / session / decoy-court fill, D-13); '
                 'the per-reservation budget is covered by fold tests only (g07_a_coalition_cannot_spend_the_outsiders_budget)')
        self.row('D-14', 'NOT-INJECTABLE', {}, 'a private-branch Final carried in by reorg needs a partitioned colluding miner set and '
                 'Gate C (not armable); not run here')

    # ---- the court-capacity / Panel-collateral pass conditions (release plan "court の全体容量と Panel 担保" §3, 2026-10-16) -----------

    COURT_CAPACITY_BRANCH = 'integ/g14-court-capacity'
    COURT_CAPACITY_ROWS = [
        ('CC-1 42-slot saturation', 'a coalition fills every court slot; a 43rd honest independent verifier still reaches a verdict '
                                    'before its deadline — and under a CONTINUOUS refill attack that re-takes slots as they free'),
        ('CC-2 court processing budget', 'at maximum load, node verification time and resources stay within the measured cap'),
        ('CC-3 time fee', 'splitting bonds does not avoid the occupancy cost (rho_court x occupied DAA x resource weight)'),
        ('CC-4 late conviction', 'after Final+1,000 but within the liability horizon, every Valid signer is slashed in full'),
        ('CC-5 collateral reuse', 'Active plus Resolved liability never exceeds the actual collateral'),
        ('CC-6 capital efficiency', 'collateral x lock time, claims processed and the number of Panels that can participate, '
                                    'against the old rule'),
        ('CC-7 restart and reorg', 'court reservations, deadlines, fees and collateral releases match after both'),
    ]

    def court_capacity_landed(self):
        """Has the court-capacity / Panel-collateral lane landed in the build under test? Published and an ancestor of HEAD."""
        ref = 'origin/' + self.COURT_CAPACITY_BRANCH
        published = subprocess.run(['git', '-C', str(REPO), 'rev-parse', '--verify', '--quiet', ref], capture_output=True,
                                   text=True).returncode == 0
        landed = published and subprocess.run(['git', '-C', str(REPO), 'merge-base', '--is-ancestor', ref, 'HEAD'],
                                              capture_output=True).returncode == 0
        self.facts['courtCapacityLane'] = {'branch': self.COURT_CAPACITY_BRANCH, 'published': published, 'landedInHead': landed}
        return landed

    def court_capacity_rows(self):
        if self.court_capacity_landed():
            for name, cond in self.COURT_CAPACITY_ROWS:
                self.row(name, 'NOT-INJECTABLE', {'condition': cond},
                         'the lane has landed but this driver has no injector for it yet: add the coalition / refill / budget fixtures')
            return
        why = ('the court-capacity and Panel-collateral lane (%s: measured court block budget, time-priced occupancy, deadline-aware '
               'scheduler, Active/Resolved liability split) has not landed in this build (%s); its fences and fixtures do not exist '
               'here, so the condition cannot be exercised' % (self.COURT_CAPACITY_BRANCH, self.facts['courtCapacityLane']))
        for name, cond in self.COURT_CAPACITY_ROWS:
            self.row(name, 'BLOCKED', {'condition': cond}, why)

    # ---- modes ---------------------------------------------------------------------------------------------------------------------

    def start_core(self, canonical, inject=None):
        a = self.args
        # With --lie-claims, P starts honest and lies only once the outside verifier exists (stop_lying_after).
        self.add('P', 0, self.producer_flags(0, canonical=canonical, inject=inject, host_panel=True,
                                             honest=bool(self.args.lie_claims)))
        # The producer's mining gate needs one peer: a plain relay (no bond) until the outsider runs (three processes at most).
        self.add('R', 6, [], bonded=False)
        for n in list(self.nodes.values()):
            n.start()
        for n in list(self.nodes.values()):
            self.wait_rpc(n)
        self.peer_all()
        st = self.status(self.nodes['P'])
        self.observe('producer node status at start', st)
        if norm(st.get('consensusParamsId')) != norm(self.profile['consensusParamsId']):
            raise Blocked('the producer runs a ruleset other than the drill profile')
        self.wait_for_chain(list(self.nodes.values()), min_daa=2, timeout_s=a.step_timeout)
        self.observe('node RSS MiB after the chain moves', self.rss())

    def rss(self):
        out = {}
        for n in self.nodes.values():
            if n.alive():
                r = subprocess.run(['ps', '-o', 'rss=', '-p', str(n.proc.pid)], capture_output=True, text=True).stdout.strip()
                out[n.role] = int(r) // 1024 if r.isdigit() else None
        return out

    def lane_mix(self):
        out = {}
        for n in self.nodes.values():
            if n.alive():
                try:
                    st = self.status(n)
                    out[n.role] = {k: st.get(k) for k in ('laneWindowBlocks', 'laneWorkBlocks', 'laneHeartbeatBlocks', 'laneLastWorkDaa',
                                                          'laneMix', 'laneAlarm', 'producerState', 'producerReason', 'producedBlocks')}
                except Exception as e:  # noqa: BLE001
                    out[n.role] = repr(e)
        return out

    def run_genesis(self):
        self.start_core(canonical=True)
        obond = self.fund_and_register_outsider()
        o_daa = self.facts['outsider']['registeredDaa'] or 0
        # Past the compressed testnet-12 flag days (the freeze — ADR-0160 F-L — arms with them), and after the outsider exists.
        before02 = self.bond_row(self.seat(0)['bond_outpoint'])['bondSlashed']
        cut02 = None
        d02 = {'evidence': {}}
        if not self.args.skip_d02:
            if self.args.lie_claims:
                _, cut02 = self.stop_lying_after(self.args.lie_claims, self.args.after_daa, 'D-02')
            d02 = self.run_conviction('D-02', 'P', 0, self.args.after_daa)
            self.metric_chain('D-02 (auto lie leaf)', self.seat(0)['bond_outpoint'], self.args.after_daa, before02, until_daa=cut02)
            self.d15_row(d02)
        for inject in self.args.inject:
            self.run_injection(inject, self.daa_quiet() or 0)
        self.fp_refusal_row('D-01', 'P', 0, governs_from=0)
        self.d17_row('P', 0, governs_from=0)
        self.fused_route_row('genesis: fused leaf is not a G14 route (10-16)', self.seat(0)['bond_outpoint'], 0)
        pbond, seats = self.seat(0)['bond_outpoint'], [self.seat(c)['bond_outpoint'] for c in range(1, self.args.seats + 1)]
        bonds = [pbond, obond] + seats
        claims = [(pbond, d02['evidence'].get('claim'))] if d02['evidence'].get('claim') else []
        rs = self.restart_and_ibd(bonds, claims)
        self.row('restart+IBD (D-02)', 'PASS' if rs['restart']['same'] and rs['ibd']['same'] else 'FAIL', rs,
                 'restart same=%s, fresh IBD same=%s' % (rs['restart']['same'], rs['ibd']['same']))
        # D-11: a second colluding producer that never answers a held unit (P keeps running: it hosts the colluding Panel).
        if self.args.d11:
            p3 = self.add('P3', 8, self.producer_flags(self.args.seats + 1, withhold=True))
            p3.start()
            self.wait_rpc(p3)
            self.peer_all()
            d11 = self.run_conviction('D-11', 'P3', self.args.seats + 1, self.daa_quiet() or 0, expect='da-default')
            self.observe('lane mix after D-11', self.lane_mix())
            self.d16_row({'D-02': d02, 'D-11': d11})
            return d02, d11
        self.d16_row({'D-02': d02})
        return d02, None

    def fp_refusal_row(self, scenario, role, card, governs_from):
        """D-01 under option (b): the colluding producer's canonical FP claims (no TFP1 frame) earn nothing past the fence."""
        pbond = self.seat(card)['bond_outpoint']
        node = self.nodes[role]
        fp = [r for r in self.claims(pbond).get('claims', []) if r['isFreePrompt'] and r['acceptedDaa'] >= governs_from]
        refusals = node.log_count(r'G14_ROUTE_ABSENT|G14_JOB_VERSION_UNJUDGEABLE|palw_g14_route_required_v1')
        commits = node.log_count(r'committed canonical claim')
        ev = {'fpClaimsAcceptedPastFence': len(fp), 'routeRefusalLogLines': refusals, 'canonicalCommitLogLines': commits}
        if fp:
            return self.row(scenario, 'FAIL', ev, '%d frameless FP claims were accepted past the fence' % len(fp))
        if refusals == 0 and commits == 0:
            return self.row(scenario, 'BLOCKED', ev, 'no canonical FP claim was attempted in the window (nothing to refuse)')
        return self.row(scenario, 'PASS', ev, 'no frameless FP claim accepted past the fence (option (b): FP selection has no route '
                        'without TFP1); the FP-leaf conviction path itself is BLOCKED with the IR FP lane')

    def d17_row(self, role, card, governs_from):
        pbond = self.seat(card)['bond_outpoint']
        fp = [r for r in self.claims(pbond).get('claims', []) if r['isFreePrompt'] and r['acceptedDaa'] >= governs_from]
        return self.row('D-17', 'PASS' if not fp else 'FAIL',
                        {'fpClaimsAcceptedPastFence': len(fp),
                         'scope': 'frameless FP (no G14 route) only; V5 image / PanelDa / seeded Float32 / job tail / flat / Kimi '
                                  'have no fixture in this drill'},
                        'out-of-scope (route-less) FP work admitted nothing past the fence' if not fp else 'route-less FP admitted')

    def fused_route_row(self, label, pbond, governs_from):
        """User decision 2026-10-16: under option (b), a claim whose adjudication depends on a fused leaf (KernelWitness ->
        ShardCourtAccused -> held dissection) does not count as having a G14 route until the court-capacity fence is armed. The
        floor class has fused-attention leaves, so every V2 Attempt of it depends on fused-leaf adjudication for a lie at such a
        leaf: past the fence, without the court-capacity fence, the expected count of admitted V2 Attempt claims is zero."""
        landed = self.facts.get('courtCapacityLane', {}).get('landedInHead')
        admitted = [r for r in self.claims(pbond).get('claims', []) if not r['isFreePrompt'] and r['acceptedDaa'] >= governs_from]
        ev = {'v2AttemptClaimsAdmittedPastFence': len(admitted), 'governsFrom': governs_from, 'courtCapacityLane': landed,
              'expected': 'zero (fused-leaf adjudication is not a G14 route until the court-capacity fence is armed)'}
        if not admitted:
            return self.row(label, 'PASS', ev, 'no V2 Attempt claim admitted past the fence')
        return self.row(label, 'FAIL', ev,
                        '%d V2 Attempt claims admitted past the fence: this build (option (b) mirror = LG14-A + LG14-B) still counts the '
                        'legacy Attempt family as routed although its fused-leaf adjudication has no court-capacity guarantee; the '
                        'rule lands with %s' % (len(admitted), self.COURT_CAPACITY_BRANCH))

    def run_cross(self):
        a = self.args
        self.start_core(canonical=True)
        pbond = self.seat(0)['bond_outpoint']
        # Below H: shipped rules. The outsider registers below H too; LG14-A is not in force, so it cannot reserve.
        obond = self.fund_and_register_outsider()
        pre = self.wait('claims accepted below H licensed', lambda: [r for r in self.claims(pbond).get('claims', [])
                                                                       if r['acceptedDaa'] < self.at_daa and r['phase'] in
                                                                       ('receipt_licensed', 'final')] or None, a.step_timeout)
        self.observe('pre-H claims licensed', pre)
        # Cross H with the chain moving.
        self.wait('the chain crosses H=%d' % self.at_daa, lambda: (self.daa_quiet() or 0) >= self.at_daa + 2, a.step_timeout)
        self.observe('lane mix at H', self.lane_mix())
        results = {}
        if a.set == 'full':
            d02 = self.run_conviction('D-02', 'P', 0, self.at_daa)
            results['d02'] = d02
            self.d16_row({'D-02': d02})
            self.d15_row(d02)
        self.fp_refusal_row('D-01', 'P', 0, governs_from=self.at_daa)
        self.d17_row('P', 0, governs_from=self.at_daa)
        if a.set == 'full':
            self.fused_route_row('cross: fused leaf is not a G14 route (10-16)', pbond, self.at_daa)
        # Claims accepted before H settle under the old rules.
        def pre_settled():
            rows = [r for r in self.claims(pbond).get('claims', []) if r['acceptedDaa'] < self.at_daa]
            fin = [r for r in rows if r['phase'] == 'final']
            return fin and rows
        try:
            rows = self.wait('claims accepted before H reach Final under the old rules', pre_settled, a.settle_timeout, every=60)
            fin = [r for r in rows if r['phase'] == 'final']
            self.row('cross: pre-H claims settle', 'PASS', {'preH': [{k: r.get(k) for k in ('claimId', 'isFreePrompt', 'acceptedDaa', 'phase',
                                                                                          'voidReason', 'phaseDaa', 'vestingStage',
                                                                                          'vestingSompi')} for r in rows]},
                     '%d of %d pre-H claims Final under the old rules (lying attempts among them: below H no G14 route exists — '
                     'the G-01 gap option (b) closes only for claims accepted past the fence)' % (len(fin), len(rows)))
        except Blocked as e:
            self.row('cross: pre-H claims settle', 'BLOCKED', {'reason': str(e)}, 'no pre-H claim reached Final within the window')
        # Block production after H.
        mix = self.lane_mix()
        daa = self.daa_quiet() or 0
        post_claims = [r for r in self.claims(pbond).get('claims', []) if r['acceptedDaa'] >= self.at_daa]
        self.row('cross: block production past H', 'PASS' if daa > self.at_daa + 10 else 'FAIL',
                 {'daaNow': daa, 'H': self.at_daa, 'laneMix': mix, 'claimsAcceptedPastH': len(post_claims), 'set': a.set},
                 'DAA %d past H=%d; set %s; %d claims accepted past H' % (daa, self.at_daa, a.set, len(post_claims)))
        if a.set == 'full' and results.get('d02'):
            seats = [self.seat(c)['bond_outpoint'] for c in range(1, a.seats + 1)]
            claims = [(pbond, results['d02']['evidence'].get('claim'))] + [(pbond, r['claimId']) for r in post_claims[:1]]
            rs = self.restart_and_ibd([pbond, obond] + seats, [c for c in claims if c[1]])
            self.row('restart+IBD (cross)', 'PASS' if rs['restart']['same'] and rs['ibd']['same'] else 'FAIL', rs,
                     'restart same=%s, fresh IBD same=%s' % (rs['restart']['same'], rs['ibd']['same']))
        return results

    # ---- the positive family (P-1..P-5) ---------------------------------------------------------------------------------------

    def run_positive(self):
        a = self.args
        self.start_core(canonical=True)
        pbond = self.seat(0)['bond_outpoint']
        self.fund_and_register_outsider()
        fence = self.at_daa
        landed = self.facts.get('courtCapacityLane', {}).get('landedInHead')
        blocked_attempt = None if landed else ('the V2 Attempt family counts as routed only with LG14-A/B AND the court-capacity fence '
                                               '(user decision 2026-10-16: fused-leaf adjudication is not a G14 route until starvation '
                                               'resistance is shown); %s has not landed in this build' % self.COURT_CAPACITY_BRANCH)
        blocked_fp = None if self.ir_fp_open else ('the IR Free Prompt lane (TIR frame) is not in this build (%s), and frame pricing '
                                                   '(audit §9: minimum rent >= 2e7 MSK, Final after 6e7 DAA at t12 prices) is unresolved'
                                                   % TIR_FP_CLOSED_SENTINEL)
        samples = []
        t0 = time.monotonic()

        def sample():
            dag = self.dag()
            rows = self.claims(pbond).get('claims', [])
            st = {}
            try:
                st = self.ref().rpc('getPalwSettlement', {'daaScore': 0})
            except Exception:  # noqa: BLE001
                pass
            mix = self.lane_mix().get('P', {})
            row = {'wallS': round(time.monotonic() - t0), 'daa': dag['virtualDaaScore'], 'blocks': dag['blockCount'],
                   'claims': len(rows), 'pastFence': sum(1 for r in rows if r['acceptedDaa'] >= fence),
                   'final': sum(1 for r in rows if r['phase'] == 'final'),
                   'finalPastFence': sum(1 for r in rows if r['phase'] == 'final' and r['acceptedDaa'] >= fence),
                   'voided': sum(1 for r in rows if r['phase'] == 'voided'),
                   'settlementDepth': st.get('depth'), 'safeFrontierDaa': st.get('safeFrontierDaa'),
                   'safeFrontierBlueScore': st.get('safeFrontierBlueScore'),
                   'laneWork': mix.get('laneWorkBlocks') if isinstance(mix, dict) else None,
                   'laneHeartbeat': mix.get('laneHeartbeatBlocks') if isinstance(mix, dict) else None}
            samples.append(row)
            self.facts['positiveSamples'] = samples[-200:]
            return row

        # Watch until a claim accepted past the fence is Final (or the window ends), sampling every few minutes.
        deadline = time.monotonic() + a.settle_timeout
        last_daa, moved = None, time.monotonic()
        stall = None
        while time.monotonic() < deadline:
            self.check_alive()
            try:
                row = sample()
            except Exception as e:  # noqa: BLE001
                log('positive sample failed: %r' % e)
                time.sleep(30)
                continue
            if row['daa'] != last_daa:
                last_daa, moved = row['daa'], time.monotonic()
            if time.monotonic() - moved > a.stall_s:
                stall = 'DAA %s did not move for %d s' % (last_daa, a.stall_s)
                break
            if row['finalPastFence'] >= a.positive_finals:
                break
            time.sleep(a.positive_every)
        rows = self.claims(pbond).get('claims', [])
        past = [r for r in rows if r['acceptedDaa'] >= fence and not r['isFreePrompt']]
        fp_past = [r for r in rows if r['acceptedDaa'] >= fence and r['isFreePrompt']]
        finals = [r for r in past if r['phase'] == 'final']
        first, last = (samples[0] if samples else {}), (samples[-1] if samples else {})
        ev1 = {'v2AttemptClaimsPastFence': len(past), 'fpClaimsPastFence': len(fp_past),
               'phases': {k: sum(1 for r in past if r['phase'] == k) for k in {r['phase'] for r in past}}}
        # P-1
        self.row('P-1a honest V2 Attempt accepted on a routed family', 'BLOCKED' if blocked_attempt else ('PASS' if past else 'FAIL'),
                 ev1, (blocked_attempt + ' — observed in this build: %d honest V2 Attempt claims admitted past the fence' % len(past))
                 if blocked_attempt else '%d honest V2 Attempt claims admitted past the fence' % len(past))
        self.row('P-1b honest IR Free Prompt via the TIR frame', 'BLOCKED' if blocked_fp else ('PASS' if fp_past else 'FAIL'),
                 {'fpClaimsPastFence': len(fp_past)},
                 (blocked_fp + ' — observed: %d FP claims admitted past the fence (frameless FP is refused by option (b))' % len(fp_past))
                 if blocked_fp else '%d framed FP claims' % len(fp_past))
        # P-2
        ev2 = {'finalPastFence': len(finals), 'first': first, 'last': last,
               'timeToFinalDaa': sorted(r['phaseDaa'] - r['acceptedDaa'] for r in finals)[:10]}
        grew = bool(finals) and (last.get('settlementDepth') or 0) > (first.get('settlementDepth') or 0) and \
            (last.get('safeFrontierDaa') or 0) > (first.get('safeFrontierDaa') or 0)
        verdict2 = 'PASS' if grew else 'FAIL'
        self.row('P-2 Final, settlement depth and safe frontier grow', 'BLOCKED' if blocked_attempt else verdict2, ev2,
                 ('%s — observed: %d claims past the fence Final; depth %s→%s; safe frontier DAA %s→%s (would be %s)' % (
                     blocked_attempt, len(finals), first.get('settlementDepth'), last.get('settlementDepth'), first.get('safeFrontierDaa'),
                     last.get('safeFrontierDaa'), verdict2)) if blocked_attempt else
                 '%d Final; depth %s→%s; safe frontier DAA %s→%s' % (len(finals), first.get('settlementDepth'), last.get('settlementDepth'),
                                                                   first.get('safeFrontierDaa'), last.get('safeFrontierDaa')))
        # P-3: the vesting hold on each Final claim (the reward is paid only after its liability horizon).
        holds = [{'claim': r['claimId'][:16], 'finalDaa': r['phaseDaa'], 'vestingStage': r.get('vestingStage'),
                  'vestingSompi': r.get('vestingSompi'), 'vestingExpiryDaa': r.get('vestingExpiryDaa'),
                  'vestingMaturedAt': r.get('vestingMaturedAt'), 'vestingEtaDaa': r.get('vestingEtaDaa'),
                  'holdDaa': (r.get('vestingExpiryDaa') - r['phaseDaa']) if r.get('vestingExpiryDaa') else None} for r in finals[:10]]
        paid = [h for h in holds if h['vestingMaturedAt']]
        ev3 = {'holds': holds, 'daaNow': last.get('daa')}
        if not finals:
            self.row('P-3 reward paid after the liability-horizon hold', 'BLOCKED', ev3,
                     (blocked_attempt + '; ' if blocked_attempt else '') + 'no claim past the fence reached Final in the window')
        elif not paid:
            self.row('P-3 reward paid after the liability-horizon hold', 'BLOCKED', ev3,
                     (blocked_attempt + '; ' if blocked_attempt else '') + 'Final observed; the payout is held to its horizon (hold %s DAA) '
                     'and the horizon was not reached in the window (DAA now %s) — the payment itself is unobserved' % (
                         sorted({h['holdDaa'] for h in holds if h['holdDaa'] is not None}), last.get('daa')))
        else:
            self.row('P-3 reward paid after the liability-horizon hold', 'BLOCKED' if blocked_attempt else 'PASS', ev3,
                     '%d rewards matured after their hold' % len(paid))
        # P-4: block production across the fence — heartbeat and PALW both.
        across = [x for x in samples if x['daa'] >= fence]
        hb = (last.get('laneHeartbeat') or 0) > 0
        work_after = any(x['pastFence'] > 0 for x in samples)
        ok4 = stall is None and len(across) >= 2 and across[-1]['daa'] > across[0]['daa'] and hb and (work_after or blocked_attempt)
        self.row('P-4 block production never stalls across the fence', 'PASS' if ok4 else 'FAIL',
                 {'stall': stall, 'daaFirst': first.get('daa'), 'daaLast': last.get('daa'), 'fence': fence, 'samples': len(samples),
                  'heartbeatInWindow': last.get('laneHeartbeat'), 'workInWindow': last.get('laneWork'),
                  'palwClaimsPastFence': len(past)},
                 'DAA %s→%s across fence %d; heartbeat blocks in window %s; PALW claims past the fence %d%s' % (
                     first.get('daa'), last.get('daa'), fence, last.get('laneHeartbeat'), len(past), '; stall: %s' % stall if stall else ''))
        # P-5: economics — what a claim holds and costs, and how long to Final.
        if rows:
            esc = sorted(int(r.get('escrowSompi') or 0) for r in past or rows)
            res = sorted(int(r.get('reservedSompi') or 0) for r in past or rows)
        else:
            esc = res = []
        try:
            bal = self.ref().rpc('getBalanceByAddress', {'address': self.seat(0)['address']}).get('balance')
        except Exception as e:  # noqa: BLE001
            bal = repr(e)
        ttf = sorted(r['phaseDaa'] - r['acceptedDaa'] for r in finals)
        ev5 = {'claimsObserved': len(past), 'escrowSompiMedian': esc[len(esc) // 2] if esc else None,
               'reservedSompiMedian': res[len(res) // 2] if res else None, 'timeToFinalDaaMedian': ttf[len(ttf) // 2] if ttf else None,
               'producerAddressBalanceNow': bal, 'wallSecondsPerDaa': round(last['wallS'] / max(1, last['daa'] - first['daa']), 1)
               if last and first and last.get('daa') != first.get('daa') else None,
               'note': 'attempt claims ride their own blocks (no carrier rent); the per-claim figures are the bond reservation and '
                       'escrow; the FP frame rent is unmeasurable here (no IR FP lane)'}
        self.row('P-5 economics (rent per claim, time to Final)', 'BLOCKED', ev5,
                 (blocked_attempt or blocked_fp or 'recorded') + ' — recorded: escrow median %s sompi, reserved median %s sompi, time to Final median %s '
                 'DAA, %s s per DAA' % (ev5['escrowSompiMedian'], ev5['reservedSompiMedian'], ev5['timeToFinalDaaMedian'],
                                        ev5['wallSecondsPerDaa']))

    def run_cross_b(self):
        """Option (b) alone at H: no route — the legacy families earn nothing past H and the heartbeat lane carries the chain."""
        a = self.args
        self.start_core(canonical=False)
        pbond = self.seat(0)['bond_outpoint']
        self.wait('the chain crosses H=%d' % self.at_daa, lambda: (self.daa_quiet() or 0) >= self.at_daa, a.step_timeout)
        at_h = self.lane_mix()
        target = self.at_daa + a.liveness_span
        self.wait('the chain advances %d DAA past H with no PALW route armed' % a.liveness_span,
                  lambda: (self.daa_quiet() or 0) >= target, a.step_timeout + 240 * a.liveness_span)
        post = [r for r in self.claims(pbond).get('claims', []) if r['acceptedDaa'] >= self.at_daa]
        refusals = self.nodes['P'].log_count(r'G14_ROUTE_ABSENT|palw_g14_route_required_v1')
        mix = self.lane_mix()
        ok = not post and (self.daa_quiet() or 0) >= target
        self.row('cross-b: liveness with no route armed', 'PASS' if ok else 'FAIL',
                 {'H': self.at_daa, 'span': a.liveness_span, 'claimsAcceptedPastH': len(post), 'routeRefusalLogLines': refusals,
                  'laneMixAtH': at_h, 'laneMixAfter': mix},
                 'DAA advanced %d past H with %d claims admitted (heartbeat lane carried the chain)' % (a.liveness_span, len(post)))


    # ---- the metric chain (audit §7) and the G-17 injection phases ---------------------------------------------------------------

    def metric_chain(self, label, pbond, after_daa, before_slashed, lane='attempt', until_daa=None):
        """eligible → checked → detected → localized → filed → accepted → convicted / defaulted → collected → invalidated, per lying
        claim of `pbond` accepted at or past `after_daa`, read from public RPC and the outside verifier's own log (its filer's lines);
        capacity refusals, unjudged cases, fused-leaf hand-offs and claims that reached Final are counted apart."""
        o = self.nodes['O']
        olog = o.log_text()
        rows = [r for r in self.claims(pbond).get('claims', [])
                if r['acceptedDaa'] >= after_daa and (until_daa is None or r['acceptedDaa'] < until_daa)
                and (r['isFreePrompt'] if lane == 'fp' else not r['isFreePrompt'])]
        chain = {k: 0 for k in ('eligible', 'checked', 'detected', 'localized', 'filed', 'accepted', 'convicted', 'defaulted',
                                'collected', 'invalidated')}
        apart = {k: 0 for k in ('capacityRefused', 'unjudged', 'fusedHandOff', 'neverLanded', 'reachedFinal', 'stillOpen')}
        per = []
        for r in rows:
            cid = norm(r['claimId'])
            mark = lambda pat: re.search(r'claim (0x)?%s: .*%s' % (cid, pat), olog) is not None  # noqa: E731
            d = self.dispute(r['claimId']) or {}
            row = {'claim': cid[:16], 'acceptedDaa': r['acceptedDaa'], 'phase': r['phase'], 'voidReason': r.get('voidReason')}
            row['checked'] = mark(r'the fraud filer replays|native TIR replay finished')
            row['detected'] = mark(r'queues Reserve') or bool(d.get('reservations'))
            row['localized'] = mark(r'queues Terminal|first divergent leaf')
            row['filed'] = mark(r'queues Terminal|public proof is filed')
            row['convicted'] = bool(d.get('courtConvicted') or d.get('executorRefuted')) or (r.get('voidReason') or '').startswith('court')
            row['defaulted'] = bool(d.get('daDefaulted')) or r.get('voidReason') == 'producer_withholding'
            row['accepted'] = row['convicted'] or row['defaulted']
            row['invalidated'] = r['phase'] == 'voided' and not (r.get('payoutPendingSompi') or r.get('vestingSompi'))
            row['capacityRefused'] = mark(r'AtCapacity|at capacity')
            row['unjudged'] = mark(r'cannot judge|ended without an objective outcome|cannot proceed')
            row['fusedHandOff'] = mark(r'fused-attention site')
            row['neverLanded'] = mark(r'never landed')
            row['reachedFinal'] = r['phase'] == 'final'
            chain['eligible'] += 1
            for k in ('checked', 'detected', 'localized', 'filed', 'accepted', 'convicted', 'defaulted', 'invalidated'):
                chain[k] += int(bool(row[k]))
            for k in ('capacityRefused', 'unjudged', 'fusedHandOff', 'neverLanded', 'reachedFinal'):
                apart[k] += int(bool(row[k]))
            apart['stillOpen'] += int(r['phase'] not in ('voided', 'final'))
            per.append(row)
        slashed = self.bond_row(pbond)['bondSlashed'] - before_slashed
        chain['collected'] = slashed
        out = {'label': label, 'chain': chain, 'apart': apart, 'claims': per[:40], 'producerSlashedSompi': slashed}
        self.observe('metric chain %s' % label, out)
        return out

    def stop_lying_after(self, n, since_daa, label, inject=None, role='P', card=0):
        """Bound the lie: once `n` lying attempt claims of P are accepted at or past `since_daa`, restart P honest (same bond, same
        retention, the colluding Panel still in its process). One public bond carries one object per block (its carrier chain), so a
        producer lying on EVERY draw outruns its pursuit (run 3, 2026-10-17: 41 reservations, 0 demands landed in 38 DAA)."""
        pbond = self.seat(card)['bond_outpoint']
        # The compressed flag days (DAA 6..28) each break the peers' fork-id handshake for a while, the nodes mine apart and the
        # reorg on reconnect can drop a carried reservation (run 4). Lie only past them.
        quiet = max(since_daa, PRIVATE_G14_LAST_FLAG_DAY + 2)
        self.wait('%s: the chain passes the compressed flag days (DAA %d)' % (label, quiet),
                  lambda: (self.daa_quiet() or 0) >= quiet, self.args.step_timeout, every=20)
        p = self.nodes[role]
        p.stop()
        p.extra = self.producer_flags(card, canonical=False, inject=inject, host_panel=(role == 'P'))
        p.start()
        self.wait_rpc(p)
        self.peer_all()
        since_daa = max(since_daa, self.daa_quiet() or 0)
        rows = self.wait('%s: %d lying claims accepted' % (label, n), lambda: (lambda r: r if len(r) >= n else None)(
            [x for x in self.claims(pbond).get('claims', []) if x['acceptedDaa'] >= since_daa and not x['isFreePrompt']]),
            self.args.step_timeout, every=15)
        p.stop()
        p.extra = self.producer_flags(card, canonical=False, host_panel=(role == 'P'), honest=True)
        p.start()
        self.wait_rpc(p)
        self.peer_all()
        cut = self.daa_quiet() or 0
        self.observe('%s: P restarted honest' % label, {'lyingClaims': [r['claimId'][:16] for r in rows], 'atDaa': cut, 'inject': inject})
        return rows, cut

    def run_injection(self, inject, after_daa):
        """One G-17 injection phase: restart P with the lie, wait for lying claims accepted in the phase to end (or the window), and
        follow each through the metric chain. PASS: every eligible claim that ended was convicted or defaulted, collected and
        invalidated, and none reached Final with its reward; FAIL: a lying claim reached Final (a G14 miss); OBSERVED: no claim ended
        in the window (the chain is recorded as is)."""
        a = self.args
        # A producer of its own per phase (D-02 may have taken card 0's whole collateral): card seats+2+i, the third process.
        idx = a.inject.index(inject)
        card = a.seats + 2 + idx
        if card >= len(self.keyring['seats']):
            return self.row('G-17 %s' % inject, 'NOT-RUN', {}, 'no genesis card left for this phase\'s producer')
        for role in ('P3', 'F') + tuple('PI%d' % j for j in range(idx)):
            if role in self.nodes and self.nodes[role].alive():
                self.nodes[role].stop()
        role = 'PI%d' % idx
        p = self.add(role, 8, self.producer_flags(card, canonical=False, inject=inject))
        p.start()
        self.wait_rpc(p)
        self.peer_all()
        pbond = self.seat(card)['bond_outpoint']
        start = max(after_daa, self.daa_quiet() or 0)
        before = self.bond_row(pbond)['bondSlashed']
        label = 'G-17 %s' % inject
        log('%s: phase from DAA %d' % (label, start))
        if a.lie_claims:
            lying, cut = self.stop_lying_after(a.inject_claims, start, label, inject, role=role, card=card)
            start = min(r['acceptedDaa'] for r in lying)

        def ended_enough():
            rows = [r for r in self.claims(pbond).get('claims', []) if r['acceptedDaa'] >= start and not r['isFreePrompt']
                    and (not a.lie_claims or r['acceptedDaa'] < cut)]
            done = [r for r in rows if r['phase'] in ('voided', 'final')]
            return (rows, done) if len(done) >= a.inject_claims else None
        try:
            self.wait('%s: %d lying claims end' % (label, a.inject_claims), ended_enough, a.inject_timeout, every=30)
        except Blocked as e:
            self.observe('%s window ended' % label, str(e))
        m = self.metric_chain(label, pbond, start, before, until_daa=cut if a.lie_claims else None)
        c, ap = m['chain'], m['apart']
        ended = c['accepted'] + ap['reachedFinal']
        if ap['reachedFinal']:
            return self.row(label, 'FAIL', m, '%d lying claims reached Final (G14 miss) — chain %s' % (ap['reachedFinal'], c))
        if not ended:
            return self.row(label, 'OBSERVED', m, 'no lying claim ended in the window — chain %s, apart %s' % (c, ap))
        if c['accepted'] and c['invalidated'] >= c['accepted'] and c['collected'] > 0:
            return self.row(label, 'PASS', m, 'chain %s' % c)
        return self.row(label, 'FAIL', m, 'ended without collection/invalidation — chain %s, apart %s' % (c, ap))

    # ---- acceptance tests A–G (audit §8) ----------------------------------------------------------------------------------------

    ACCEPTANCE = [
        ('A all Panel collude, one outside bond convicts from public reads',
         'legacy_v2_attempt_full_collusion_outside_bond_convicts'),
        ('D withheld material ends in the DA default', 'legacy_v2_attempt_held_da_withholding_ends_in_da_default'),
        ('D false accusation refused', 'g14_accept_d_a_false_accusation_of_an_honest_claim_is_refused'),
        ('D verifier implementation difference is not producer fraud',
         'g14_accept_d_a_verifier_implementation_difference_is_not_producer_fraud'),
        ('E collected once, reporter from collected, rights void', 'g14_accept_e_collected_once_reporter_from_collected_and_rights_void'),
        ('F same conviction from Final-1', 'g14_accept_f_the_same_conviction_from_final_minus_one'),
        ('F same conviction after Final inside the liability', 'g14_accept_f_the_same_conviction_after_final_inside_the_liability'),
        ('F bond exit raced against the pursuit', 'g14_accept_f_a_bond_exit_raced_against_the_pursuit_escapes_nothing'),
        ('F the pursuit window and the payout never overlap', 'g14_accept_f_the_pursuit_window_and_the_payout_never_overlap'),
        ('F reorg / restart / IBD / pruned import (parent harness)', 'g14r_'),
        ('G honest claims before / at / after the fence settle', 'g14_accept_g_honest_claims_before_at_and_after_the_fence_settle'),
    ]
    # Other lanes' acceptance tests: (row, branch, test filter). Run once the branch is an ancestor of HEAD, else BLOCKED.
    OTHER_LANES = [
        ('B per-family detection rates (single point / boundary / whole forgery)', 'integ/g14-detection-policy', 'g14_accept_b_'),
        ('C court saturation, continuous refill and TIR owners all active', 'integ/g14-court-capacity', 'g14_accept_c_'),
        ('C TIR frame price and clock (owner admission)', 'integ/g14-tir-pricing', 'g14_accept_c_tir_'),
    ]

    def cargo_test(self, filt):
        cmd = ['cargo', 'test', '-j', '3', '-p', 'kaspa-consensus', '--lib', filt, '--', '--test-threads=1']
        t0 = time.monotonic()
        p = subprocess.run(cmd, cwd=str(REPO), capture_output=True, text=True, timeout=self.args.acceptance_timeout)
        out = p.stdout + p.stderr
        passed = [m for m in re.findall(r'test (\S+) \.\.\. ok', out)]
        failed = [m for m in re.findall(r'test (\S+) \.\.\. FAILED', out)]
        return {'cmd': ' '.join(cmd), 'rc': p.returncode, 'passed': passed, 'failed': failed, 'seconds': round(time.monotonic() - t0),
                'tail': out[-3000:]}

    def run_acceptance(self):
        for name, filt in self.ACCEPTANCE:
            r = self.cargo_test(filt)
            status = 'PASS' if r['rc'] == 0 and r['passed'] and not r['failed'] else ('FAIL' if r['failed'] or r['passed'] else 'BLOCKED')
            self.row('acceptance ' + name, status, r, '%d passed, %d failed (%s)' % (len(r['passed']), len(r['failed']), filt))
        for name, branch, filt in self.OTHER_LANES:
            ref = 'origin/' + branch
            published = subprocess.run(['git', '-C', str(REPO), 'rev-parse', '--verify', '--quiet', ref], capture_output=True).returncode == 0
            landed = published and subprocess.run(['git', '-C', str(REPO), 'merge-base', '--is-ancestor', ref, 'HEAD'],
                                                  capture_output=True).returncode == 0
            if not landed:
                self.row('acceptance ' + name, 'BLOCKED', {'branch': branch, 'published': published, 'landedInHead': landed},
                         'owned by lane %s, which has %s' % (branch, 'not landed in this build' if published else 'not been published'))
                continue
            r = self.cargo_test(filt)
            status = 'PASS' if r['rc'] == 0 and r['passed'] else ('FAIL' if r['failed'] else 'BLOCKED')
            self.row('acceptance ' + name, status, r, '%d passed, %d failed (%s)' % (len(r['passed']), len(r['failed']), filt))

    # ---- run -----------------------------------------------------------------------------------------------------------------------

    def run(self):
        try:
            if self.args.acceptance_only:
                self.run_acceptance()
                self.facts['status'] = 'COMPLETED'
                return
            self.preflight()
            self.write_keyring()
            if self.args.family == 'collusion':
                self.static_rows()
                self.court_capacity_rows()
            else:
                self.court_capacity_landed()
            if self.args.run_acceptance:
                self.run_acceptance()
            if self.args.family == 'positive':
                self.run_positive()
            elif self.args.mode == 'genesis':
                self.run_genesis()
            elif self.args.set == 'full':
                self.run_cross()
            else:
                self.run_cross_b()
            self.facts['status'] = 'COMPLETED'
        except Blocked as e:
            self.facts['status'] = 'BLOCKED'
            self.facts['blockedReason'] = str(e)
            log('BLOCKED: %s' % e)
        except KeyboardInterrupt as e:
            self.facts['status'] = 'INTERRUPTED'
            self.facts['error'] = repr(e)
            log('INTERRUPTED: %r' % e)
        except Exception as e:  # noqa: BLE001 — recorded with the observations
            self.facts['status'] = 'ERROR'
            self.facts['error'] = repr(e)
            log('ERROR: %r' % e)
            import traceback
            traceback.print_exc()
        finally:
            try:
                self.facts['laneMixAtEnd'] = self.lane_mix()
                self.facts['rssMiBAtEnd'] = self.rss()
            except Exception:  # noqa: BLE001
                pass
            for n in self.nodes.values():
                n.stop()
            self.facts['endedAt'] = now()
            self.facts['logTails'] = {n.role: n.log_text()[-4000:] for n in self.nodes.values()}
            self.flush()


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--mode', choices=['genesis', 'cross'], required=True)
    ap.add_argument('--family', choices=['collusion', 'positive'], default='collusion')
    ap.add_argument('--positive-finals', type=int, default=3, help='positive: stop once this many claims past the fence are Final')
    ap.add_argument('--positive-every', type=int, default=180)
    ap.add_argument('--at', type=int, default=40, help='cross mode: the fence height H (DAA)')
    ap.add_argument('--set', choices=['full', 'route-required-only'], default='full')
    ap.add_argument('--seats', type=int, default=3)
    ap.add_argument('--salt', required=True, help='64 hex, this drill only (openssl rand -hex 32)')
    ap.add_argument('--work', required=True, help='fresh directory for app dirs, logs and the keyring (scratch, never ~/.misaka)')
    ap.add_argument('--out', required=True)
    ap.add_argument('--private-node', default=str(REPO / 'target/release/misaka-rfc12-private-node'))
    ap.add_argument('--kaspad', default=str(REPO / 'target/release/kaspad'))
    ap.add_argument('--cli', default=str(REPO / 'target/release/misaka'))
    ap.add_argument('--port-base', type=int, default=34100)
    ap.add_argument('--tamper-leaf', type=int, default=None, help='default: auto (a held, openable, non-fused leaf per draw)')
    ap.add_argument('--canonical-interval', type=int, default=20)
    ap.add_argument('--outsider-funding-bili', type=int, default=2_000_000)
    ap.add_argument('--outsider-collateral-bili', type=int, default=0, help='0: the node sizes it')
    ap.add_argument('--step-timeout', type=int, default=4 * 3600)
    ap.add_argument('--conviction-timeout', type=int, default=8 * 3600)
    ap.add_argument('--filing-timeout', type=int, default=2 * 3600)
    ap.add_argument('--settle-timeout', type=int, default=12 * 3600)
    ap.add_argument('--stall-s', type=int, default=1800)
    ap.add_argument('--liveness-span', type=int, default=15)
    ap.add_argument('--after-daa', type=int, default=0, help='genesis mode: the claim judged is accepted at or past this DAA')
    ap.add_argument('--frozen-timeout', type=int, default=3600)
    ap.add_argument('--no-d11', dest='d11', action='store_false')
    ap.add_argument('--no-d16', dest='d16', action='store_false')
    ap.add_argument('--inject', action='append', default=[],
                    help='G-17 injection phase after D-02 (repeatable): a --palw-drill-fault kind, sparse:<seed> or withhold-all')
    ap.add_argument('--inject-claims', type=int, default=1, help='lying claims to follow to their end per injection phase')
    ap.add_argument('--skip-d02', action='store_true', help='skip the D-02 phase (run the injections, restart/IBD and D-11 only)')
    ap.add_argument('--lie-claims', type=int, default=0,
                    help='bound the lie: restart P honest after this many lying claims (0: lie on every draw, as before)')
    ap.add_argument('--inject-timeout', type=int, default=3 * 3600)
    ap.add_argument('--run-acceptance', action='store_true', help='run acceptance tests A-G (cargo, targeted, -j 3) before the drill')
    ap.add_argument('--acceptance-only', action='store_true', help='only the acceptance tests; no nodes are started')
    ap.add_argument('--acceptance-timeout', type=int, default=4 * 3600)
    args = ap.parse_args()
    if len(args.salt) != 64 or set(args.salt) == {'0'}:
        ap.error('--salt: 64 hex, not all zero')
    if args.mode == 'cross' and args.at <= 0:
        ap.error('--at must be positive in cross mode')

    # A background start (nohup … &) inherits SIGINT ignored: restore it, and make SIGTERM unwind the same way, so the finally block
    # always stops every node this driver started.
    def unwind(signum, frame):
        raise KeyboardInterrupt('signal %d' % signum)
    signal.signal(signal.SIGINT, unwind)
    signal.signal(signal.SIGTERM, unwind)
    Drill(args).run()


if __name__ == '__main__':
    main()
