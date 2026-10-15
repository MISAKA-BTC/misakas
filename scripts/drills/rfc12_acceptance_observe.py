"""Generation-bracketed real-process reads for the RFC-0012 acceptance driver.

Every read set is taken between two identical public stamps (sink, pruning point,
virtual DAA, genesis, consensus params id and the durable import epoch/point/
phase/stable flag), as the existing multiprocess probe does. ETH JSON-RPC reads go
over a separate HTTP transport and are bracketed by the same wRPC stamps. A moved
stamp raises ContextMoved, which the caller's bounded wait retries; a moved stamp
is never combined into one row.

The only owned files read here are ones the owned node child writes itself (its
log, its kernel/checker outboxes, its provider store). Fixture command output is
never read as node state. Nothing here judges; see rfc12_acceptance_judgement.
"""
import hashlib
import json
import re
import time
import urllib.request
from pathlib import Path

from rfc12_acceptance_judgement import NotObservable, Pending, TAGS, norm

NETWORK_DIR = 'misaka-testnet-12'
KERNEL_OUTBOX = 'kernel-paid-producer-v4'
CHECKER_OUTBOX = 'kernel-paid-checker-v4'
PROVIDER_STORE_DIR = 'palw-native-provider-v3'
# Proposed read-only op 247 (not present in this tree): see paid_claim_read_spec().
PAID_CLAIM_METHOD = 'getPalwPaidClaim'
LOG_PATTERNS = (
    ('enrollment-accepted', re.compile(r'private kernel paid enrollment accepted: ([0-9a-f]{128})')),
    ('receipt-accepted', re.compile(r'private selected checker receipt accepted: ([0-9a-f]{128}) round (\d+)')),
    ('challenge-accepted', re.compile(r'private objective court action accepted: ([0-9a-f]{128})')),
    ('checker-stopped', re.compile(r'private selected checker stopped: (.*)')),
    ('producer-stopped', re.compile(r'private kernel paid producer stopped: (.*)')),
    ('finalized-conflict', re.compile(r'FINALIZED CONFLICT: (\S+) abandons (\S+)')),
)


class ContextMoved(AssertionError):
    """The public generation moved during one read set; retry, never merge."""


def paid_claim_read_spec():
    """The read this harness needs and the node does not serve yet (reserved op 247, proto 1312/1313)."""
    return dict(method=PAID_CLAIM_METHOD, op=247, protoFields=[1312, 1313], readOnly=True,
                request=dict(baseClaim='hex128', round='u32'),
                response=dict(available='bool', found='bool', sink='hex128 (the sink the answer stands at)',
                              tipDaa='u64', baseClaim='hex128', enrollment='hex128 (PaidClaimEnrollment id)',
                              classId='hex128', jobId='hex128', producerBond='txid:index',
                              materialConsent='hex128', state='text', round='u32',
                              seats=['txid:index (assigned live seats of the round)'], quorum='u32',
                              receipts=['txid:index (accepted receipt signers of the round)'],
                              final='bool', finalDaa='u64'))


def context(dag, node):
    if node.get('importSessionAvailable') is not True:
        raise NotObservable('this node predates durable import-session diagnostics')
    epoch = node.get('importSessionEpoch')
    if type(epoch) is not int or not 0 <= epoch < 2**64:
        raise NotObservable('durable import epoch is not an exact unsigned counter')
    daa = dag.get('virtualDaaScore')
    if type(daa) is not int or daa < 0:
        raise NotObservable('virtual DAA is not an exact integer')
    return (norm(dag['sink']), norm(dag['pruningPointHash']), daa, norm(node['genesisHash']),
            norm(node['consensusParamsId']), epoch, node['importSessionPoint'], node['importSessionPhase'],
            node['pruningImportStable'])


def bracketed(drill, role, label, body):
    """Run body(rpc, dag, node) between two identical generation stamps of one owned role."""
    began = time.monotonic()
    rpc = drill.rpc(role)
    try:
        dag, node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
        before = context(dag, node)
        value = body(rpc, dag, node)
        after = context(rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus'))
    finally:
        rpc.close()
        drill.latencies.append(round((time.monotonic() - began) * 1000, 3))
    if after != before:
        raise ContextMoved('%s generation moved during the %s read; retry' % (role, label))
    return dict(source='node-rpc', role=role, label=label, dag=dag, node=node, value=value,
                observedUnix=time.time(), elapsedMs=round((time.monotonic() - began) * 1000, 3))


def eth_call(url, method, params, timeout):
    request = urllib.request.Request(url, json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': params}).encode(),
                                     {'Content-Type': 'application/json'})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        answer = json.load(response)
    return dict(result=answer.get('result'), error=answer.get('error'))


def eth_result(url, method, params, timeout):
    answer = eth_call(url, method, params, timeout)
    if answer['error'] is not None:
        raise Pending('ETH %s failed: %r' % (method, answer['error']))
    return answer['result']


def chain_ancestor(rpc, block_hash):
    """True/False when the node can say whether the block is on its selected chain; None when it cannot."""
    try:
        block = rpc.call('getBlock', {'hash': block_hash, 'includeTransactions': False})['block']
    except RuntimeError as error:
        if 'cannot find header' in str(error):
            return None
        raise
    flag = (block.get('verboseData') or {}).get('isChainBlock')
    return flag if isinstance(flag, bool) else None


def old_body_available(rpc, old):
    """Same classification as the multiprocess drill: header-only or a named missing header is GC."""
    try:
        block = rpc.call('getBlock', {'hash': old, 'includeTransactions': True})['block']
    except RuntimeError as error:
        if 'cannot find header ' + norm(old) in str(error):
            return False
        raise
    verbose = block.get('verboseData') or {}
    if not isinstance(verbose.get('isHeaderOnly'), bool):
        raise NotObservable('old-block response omitted explicit body status')
    return not verbose['isHeaderOnly']


def consumer_value(rpc, dag, eth_url, timeout, ancestry_of=(), require_readiness=True):
    """Native snapshot, readiness, DNS state, ETH latest/safe/finalized and root header of one generation."""
    block = rpc.call('getBlock', {'hash': dag['sink'], 'includeTransactions': False})['block']
    try:
        settlement, settlement_error = rpc.call('getPalwSettlement', {'daaScore': dag['virtualDaaScore']}), None
    except RuntimeError as error:
        settlement, settlement_error = None, str(error)
    native = (settlement or {}).get('nativeSettlement')
    if require_readiness and native is not None and settlement.get('nativeReadiness') is None:
        raise Pending('native readiness omitted at this sink (moved inside the node); retry')
    dns = rpc.call('getDnsConfirmation')
    eth = {}
    for tag in TAGS:
        answer = eth_call(eth_url, 'eth_getBlockByNumber', [tag, False], timeout)
        if answer['error'] is not None:
            raise Pending('ETH %s read failed: %r' % (tag, answer['error']))
        eth[tag] = answer
    ancestry = {norm(item): chain_ancestor(rpc, item) for item in ancestry_of if item}
    return dict(header=block['header'], settlement=settlement, settlementError=settlement_error, dns=dns, eth=eth,
                ancestry=ancestry)


def consumer_row(drill, role, label, ancestry_of=(), old=None, require_readiness=True):
    eth_url, timeout = drill.plan['nodes'][role]['eth'], drill.plan['probe_timeout_s']

    def body(rpc, dag, node):
        value = consumer_value(rpc, dag, eth_url, timeout, ancestry_of, require_readiness)
        value['oldBodyAvailable'] = None if old is None else old_body_available(rpc, old)
        return value
    row = bracketed(drill, role, label, body)
    row['oldBodyAvailable'] = row['value']['oldBodyAvailable']
    return row


def counters_value(rpc, finals_limit):
    shadow = rpc.call('getPalwCapacityShadow', {'steps': [], 'bond': '', 'adversaryBonds': [], 'includeClaims': True, 'limit': 1})
    finals = rpc.call('getPalwKernelFinals', {'limit': finals_limit})
    return dict(shadow={key: shadow.get(key) for key in ('available', 'claimsTotal', 'bondsTotal')},
                finals=dict(available=finals.get('available'), total=finals.get('total'),
                            listed=[norm(row.get('claimId')) for row in finals.get('finals') or []]))


def paid_claim_value(rpc, base_claim, round_number):
    """The proposed paid-claim read; a node without it makes the predicate BLOCKED, never inferred."""
    try:
        answer = rpc.call(PAID_CLAIM_METHOD, {'baseClaim': base_claim, 'round': round_number})
    except RuntimeError as error:
        text = str(error).lower()
        if 'unknown' in text or 'not found' in text or 'method' in text or 'unsupported' in text:
            raise NotObservable('the node serves no paid-claim read (%s); proposed read-only op 247' % PAID_CLAIM_METHOD)
        raise
    if not isinstance(answer, dict):
        raise NotObservable('the paid-claim read returned no object')
    return answer


def utxo_entries(rpc, addresses, maximum):
    entries = rpc.call('getUtxosByAddresses', {'addresses': list(addresses)})['entries']
    if len(entries) > maximum * max(1, len(addresses)):
        raise NotObservable('UTXO rows exceed the predeclared per-address bound')
    return [dict(address=entry.get('address'), outpoint=dict(transactionId=norm(entry['outpoint']['transactionId']),
                                                            index=entry['outpoint']['index']),
                 utxoEntry=dict(amount=entry['utxoEntry']['amount'], blockDaaScore=entry['utxoEntry']['blockDaaScore'],
                                isCoinbase=entry['utxoEntry']['isCoinbase'])) for entry in entries]


def amounts_of(entries):
    return {'%s:%d' % (entry['outpoint']['transactionId'], entry['outpoint']['index']): entry['utxoEntry']['amount']
            for entry in entries}


def evm_value(eth_url, accounts, timeout):
    latest = eth_result(eth_url, 'eth_getBlockByNumber', ['latest', False], timeout)
    if latest is None:
        raise Pending('no executed ETH latest block yet')
    number = latest['number']
    balances = {}
    for account in accounts:
        balance = eth_result(eth_url, 'eth_getBalance', [account, number], timeout)
        balances[account] = int(balance, 16)
    return dict(ethLatestHash=norm(latest['hash']), ethLatestNumber=int(number, 16), balances=balances)


def evm_spends(eth_url, sender, first, last, timeout, bound):
    """Gas and value of every executed EVM transaction from `sender` in [first, last], from blocks and receipts."""
    if last < first:
        return []
    if last - first + 1 > bound:
        raise NotObservable('the EVM block scan exceeds its predeclared bound')
    spends = []
    for number in range(first, last + 1):
        block = eth_result(eth_url, 'eth_getBlockByNumber', [hex(number), True], timeout)
        if block is None:
            raise Pending('ETH block %d is not executed yet' % number)
        for tx in block.get('transactions') or []:
            if norm(tx.get('from')) != norm(sender):
                continue
            receipt = eth_result(eth_url, 'eth_getTransactionReceipt', [tx['hash']], timeout)
            if receipt is None:
                raise Pending('receipt of %s not served yet' % tx['hash'])
            spends.append(dict(hash=norm(tx['hash']), block=number, valueWei=int(tx['value'], 16),
                               gasWei=int(receipt['gasUsed'], 16) * int(receipt['effectiveGasPrice'], 16),
                               status=receipt.get('status')))
    return spends


class AnchorReorged(NotObservable):
    pass


def accepted_transactions(rpc, anchor, wanted, scan):
    """Locate wanted transaction ids accepted on the selected chain after `anchor`, with their full bodies.
    `wanted=None` collects every accepted transaction of the window (bounded by the plan's scan bounds)."""
    collect_all = wanted is None
    wanted = set() if collect_all else {norm(txid) for txid in wanted}
    found, start, blocks, reads = {}, anchor, 0, 0
    while collect_all or wanted - set(found):
        chain = rpc.call('getVirtualChainFromBlock', {'startHash': start, 'includeAcceptedTransactionIds': True,
                                                      'minConfirmationCount': None})
        if chain.get('removedChainBlockHashes'):
            raise AnchorReorged('the scan anchor %s left the selected chain' % norm(start)[:16])
        added = chain.get('addedChainBlockHashes') or []
        blocks += len(added)
        if blocks > scan['maximumScannedChainBlocks']:
            raise NotObservable('the accepted-transaction scan exceeds its predeclared chain-block bound')
        for entry in chain.get('acceptedTransactionIds') or []:
            accepted = {norm(txid) for txid in entry.get('acceptedTransactionIds') or []}
            hits = accepted - set(found) if collect_all else accepted & (wanted - set(found))
            if hits:
                reads = _bodies(rpc, entry['acceptingBlockHash'], hits, found, reads, scan)
        if not added:
            break
        start = added[-1]
    return found


def _bodies(rpc, accepting, hits, found, reads, scan):
    block = rpc.call('getBlock', {'hash': accepting, 'includeTransactions': False})['block']
    verbose = block.get('verboseData') or {}
    for merged in (verbose.get('mergeSetBluesHashes') or []) + (verbose.get('mergeSetRedsHashes') or []):
        reads += 1
        if reads > scan['maximumMergesetBlockReads']:
            raise NotObservable('the accepted-transaction scan exceeds its predeclared mergeset read bound')
        body = rpc.call('getBlock', {'hash': merged, 'includeTransactions': True})['block']
        for tx in body.get('transactions') or []:
            txid = norm((tx.get('verboseData') or {}).get('transactionId'))
            if txid is None:
                raise NotObservable('block transactions lack verbose transaction ids')
            if txid in hits and txid not in found:
                found[txid] = dict(acceptingBlock=norm(accepting), containingBlock=norm(merged), transaction=tx)
        if hits <= set(found):
            break
    return reads


def outputs_as_amounts(found):
    amounts = {}
    for txid, row in found.items():
        for index, output in enumerate(row['transaction']['outputs']):
            amounts['%s:%d' % (txid, index)] = output['value']
    return amounts


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as source:
        for chunk in iter(lambda: source.read(1 << 20), b''):
            digest.update(chunk)
    return digest.hexdigest()


def network_dir(root, role):
    return Path(root) / role / 'appdir' / NETWORK_DIR


def outbox_records(root, role, kind, files):
    """Digests of the owned kernel/checker outbox records the node child itself wrote (one record directory each)."""
    base = network_dir(root, role) / (KERNEL_OUTBOX if kind == 'kernel-outbox' else CHECKER_OUTBOX)
    records = {}
    if not base.is_dir():
        return records
    for record in sorted(path for path in base.glob('*/*') if path.is_dir()):
        names = {name: sha256_file(record / name) for name in files if (record / name).is_file()}
        if names:
            records[str(record.relative_to(base))] = names
    return records


COVERAGE_FILE = 'rfc06-exec-origin-coverage-v1.json'


def coverage_file(root, role, maximum_bytes=16 << 20):
    """The owned RFC-0006 coverage observation (written by custody/kernel-consumer paid-v4 nodes every 60 s)."""
    path = network_dir(root, role) / COVERAGE_FILE
    try:
        with path.open('rb') as source:
            raw = source.read(maximum_bytes + 1)
    except FileNotFoundError:
        return None
    if len(raw) > maximum_bytes:
        raise NotObservable('the coverage observation exceeds its bounded framing')
    return json.loads(raw)


def coverage_row(drill, root, role, label):
    """The coverage file inside the role's own generation bracket; the judgement requires its DAA to match."""
    return bracketed(drill, role, label, lambda rpc, dag, node: dict(coverage=coverage_file(root, role)))


def provider_stores(root, role):
    base = network_dir(root, role)
    return {str(path.relative_to(base)): path.stat().st_size
            for path in sorted(base.rglob(PROVIDER_STORE_DIR + '/*.store')) if path.is_file()}


def db_listing(root, role):
    """Names and sizes of every owned file of a role (marker included); used to prove a refused launch wrote nothing."""
    base = Path(root) / role / 'appdir'
    return {str(path.relative_to(base)): path.stat().st_size for path in sorted(base.rglob('*')) if path.is_file()}


def log_lines(paths, maximum_bytes=64 << 20):
    """Classify the owned node logs' own diagnostic lines (never an acceptance authority)."""
    rows = []
    for path in paths:
        path = Path(path)
        if not path.is_file():
            continue
        with path.open('rb') as source:
            raw = source.read(maximum_bytes)
        for number, line in enumerate(raw.decode('utf-8', 'replace').splitlines(), 1):
            for kind, pattern in LOG_PATTERNS:
                match = pattern.search(line)
                if match:
                    rows.append(dict(source='owned-file', kind=kind, file=str(path), line=line.strip()[:1000],
                                     lineNumber=number, groups=list(match.groups()), authoritative=False))
    return rows
