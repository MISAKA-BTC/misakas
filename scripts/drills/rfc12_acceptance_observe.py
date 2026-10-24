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
# Read-only op 247 (rpc/service get_palw_paid_claim_call, consensus/core palw_paid_claim_read_v1): see paid_claim_read_spec().
PAID_CLAIM_METHOD = 'getPalwPaidClaim'
ACCEPTANCE_READS_METHOD = 'getPalwAcceptanceReads'
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
    """The paid-claim read this harness uses (op 247, proto 1312/1313); a node built before op 247 does not serve it."""
    return dict(method=PAID_CLAIM_METHOD, op=247, protoFields=[1312, 1313], readOnly=True,
                request=dict(baseClaim='hex128', round='u32'),
                response=dict(available='bool', found='bool', sink='hex128 (the sink the answer stands at)',
                              tipDaa='u64', baseClaim='hex128', enrollment='hex128 (PaidClaimEnrollment id)',
                              classId='hex128', jobId='hex128', producerBond='txid:index',
                              materialConsent='hex128', state='text', round='u32', roundFound='bool',
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
    """The paid-claim read (op 247); a node built before it makes the predicate BLOCKED, never inferred."""
    try:
        answer = rpc.call(PAID_CLAIM_METHOD, {'baseClaim': base_claim, 'round': round_number})
    except RuntimeError as error:
        text = str(error).lower()
        if 'unknown' in text or 'not found' in text or 'method' in text or 'unsupported' in text:
            raise NotObservable('the node serves no paid-claim read (%s); it predates read-only op 247' % PAID_CLAIM_METHOD)
        raise
    if not isinstance(answer, dict):
        raise NotObservable('the paid-claim read returned no object')
    return answer


def acceptance_reads(rpc, reserve_blocks=(), outpoints=()):
    """Op 246: the DNS security reserve balance per block and the virtual UTXO point read by outpoint (`txid:index`). A node built
    before it makes the dependent predicates BLOCKED, never inferred."""
    reserve_blocks, outpoints = list(reserve_blocks), list(outpoints)
    if len(reserve_blocks) > 64 or len(outpoints) > 64:
        raise NotObservable('op 246 reads at most 64 blocks and 64 outpoints a call')
    try:
        answer = rpc.call(ACCEPTANCE_READS_METHOD, {'reserveBlocks': reserve_blocks, 'outpoints': outpoints})
    except RuntimeError as error:
        text = str(error).lower()
        if 'unknown' in text or 'not found' in text or 'method' in text or 'unsupported' in text:
            raise NotObservable('the node serves no acceptance reads (%s); it predates read-only op 246' % ACCEPTANCE_READS_METHOD)
        raise
    if not isinstance(answer, dict) or not answer.get('available'):
        raise NotObservable('the acceptance reads are not available on this node')
    return dict(sink=norm(answer['sink']), virtualDaa=answer['virtualDaa'],
                reserve={norm(r['blockHash']): (r['balanceSompi'] if r['known'] else None) for r in answer['reserveBalances']},
                outpoints={o['outpoint']: (dict(amount=o['amount'], blockDaaScore=o['blockDaaScore'], isCoinbase=o['isCoinbase'],
                                                scriptPublicKey='%04x' % o['scriptVersion'] + norm(o['scriptHex']))
                                           if o['present'] else None) for o in answer['outpoints']})


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


def _eth_quantity(value, name, bits):
    if not isinstance(value, str) or re.fullmatch('0x(?:0|[1-9a-fA-F][0-9a-fA-F]*)', value) is None:
        raise NotObservable(name + ' is not a canonical ETH quantity')
    number = int(value, 16)
    if number >= 2**bits:
        raise NotObservable(name + ' exceeds its native unsigned width')
    return number


def _eth_head_stamp(block):
    if not isinstance(block, dict) or not isinstance(block.get('hash'), str) \
            or re.fullmatch('0x[0-9a-fA-F]{64}', block['hash']) is None:
        raise NotObservable('ETH latest omitted its actual 32-byte block hash')
    return norm(block['hash']), _eth_quantity(block.get('number'), 'ETH latest number', 64)


def evm_value(eth_url, accounts, timeout):
    latest = eth_result(eth_url, 'eth_getBlockByNumber', ['latest', False], timeout)
    if latest is None:
        raise Pending('no executed ETH latest block yet')
    stamp = _eth_head_stamp(latest)
    block_hash = latest['hash']
    # The adapter already implements EIP-1898. Numeric selectors can resolve to
    # another block during a same-height reorg, mixing balances under this hash.
    selector = dict(blockHash=block_hash, requireCanonical=True)
    balances = {}
    for account in accounts:
        balance = eth_result(eth_url, 'eth_getBalance', [account, selector], timeout)
        balances[account] = _eth_quantity(balance, 'ETH account balance', 256)
    after = eth_result(eth_url, 'eth_getBlockByNumber', ['latest', False], timeout)
    if after is None or _eth_head_stamp(after) != stamp:
        raise ContextMoved('ETH latest moved while reading hash-pinned account balances')
    return dict(ethLatestHash=stamp[0], ethLatestNumber=stamp[1], balances=balances)


def model_position_value(rpc, dag, holder, line):
    """Read one position from the actual native API, refusing a stale cached tip or malformed zero fallback."""
    answer = rpc.call('getPalwModelPositions', {'holder': holder})
    if not isinstance(answer, dict) or norm(answer.get('holder')) != norm(holder):
        raise NotObservable('model position response does not name the requested holder')
    tip, daa = answer.get('tipHash'), answer.get('tipDaa')
    if not isinstance(tip, str) or not tip or type(daa) is not int or not 0 <= daa < 2**64:
        raise NotObservable('model positions omitted an exact native tip')
    if norm(tip) != norm(dag['sink']) or daa != dag['virtualDaaScore']:
        raise Pending('model position snapshot is not at the bracketed native tip')
    positions = answer.get('positions')
    if not isinstance(positions, list):
        raise NotObservable('model position rows unavailable; cannot infer zero units')
    units, seen = 0, set()
    for position in positions:
        if not isinstance(position, dict):
            raise NotObservable('malformed native model position row')
        identity, amount = norm(position.get('lineId')), position.get('units')
        if not isinstance(identity, str) or re.fullmatch('[0-9a-f]{128}', identity) is None \
                or type(amount) is not int or not 0 <= amount < 2**64:
            raise NotObservable('model position identity or units are not native values')
        if identity in seen:
            raise NotObservable('model position response repeats a line')
        seen.add(identity)
        if identity == norm(line):
            units = amount
    return dict(tipHash=norm(tip), tipDaa=daa, units=units)


def evm_spends(eth_url, sender, first, last, timeout, bound, *, parent_hash, last_hash, remaining=None):
    """Fees for one hash-linked interval after the before-balance block through the after-balance block.

    Blocks come from immutable hash selectors. Numeric endpoint checks only establish
    current canonical membership, never select transaction/receipt data by height.
    The caller brackets this scan with the same native generation as its balance.
    """
    if type(first) is not int or type(last) is not int or not 1 <= first < 2**64 or not 0 <= last < 2**64 \
            or type(bound) is not int or bound <= 0:
        raise NotObservable('invalid finite EVM reconciliation interval')
    if last < first - 1 or last - first + 1 > bound:
        raise NotObservable('the EVM block scan exceeds or contradicts its predeclared bound')
    def expected_hash(value):
        value = norm(value)
        if not isinstance(value, str) or re.fullmatch('[0-9a-f]{64}', value) is None:
            raise NotObservable('EVM reconciliation endpoint is not an actual 32-byte hash')
        return value
    parent_hash, last_hash = expected_hash(parent_hash), expected_hash(last_hash)
    def read(method, params):
        allowed = min(timeout, remaining()) if remaining is not None else timeout
        if allowed <= 0:
            raise NotObservable('EVM reconciliation read budget expired')
        value = eth_result(eth_url, method, params, allowed)
        if remaining is not None:
            remaining()
        return value
    def canonical_endpoint():
        block = read('eth_getBlockByNumber', [hex(last), False])
        if block is None or _eth_head_stamp(block) != (last_hash, last):
            raise ContextMoved('the frozen EVM reconciliation endpoint left the canonical chain')
    canonical_endpoint()
    if last == first - 1:
        if last_hash != parent_hash:
            raise ContextMoved('an empty EVM interval changed its before-balance hash')
        return []
    target, blocks_seen, transactions_seen, groups = last_hash, set(), set(), []
    for number in range(last, first - 1, -1):
        if target in blocks_seen:
            raise NotObservable('EVM reconciliation contains a cyclic block path')
        blocks_seen.add(target)
        block = read('eth_getBlockByHash', ['0x' + target, True])
        if block is None:
            raise Pending('frozen EVM block data is unavailable')
        if _eth_head_stamp(block) != (target, number):
            raise NotObservable('EVM hash walk returned another block or skipped a block number')
        parent = block.get('parentHash')
        if not isinstance(parent, str) or re.fullmatch('0x[0-9a-fA-F]{64}', parent) is None:
            raise NotObservable('EVM block omitted its actual parent hash')
        transactions = block.get('transactions')
        if not isinstance(transactions, list):
            raise NotObservable('full EVM transaction bodies are unavailable')
        spends = []
        for index, tx in enumerate(transactions):
            if remaining is not None:
                remaining()
            if not isinstance(tx, dict) or not isinstance(tx.get('hash'), str) \
                    or re.fullmatch('0x[0-9a-fA-F]{64}', tx['hash']) is None \
                    or not isinstance(tx.get('from'), str) or re.fullmatch('0x[0-9a-fA-F]{40}', tx['from']) is None:
                raise NotObservable('full EVM transaction identity is unavailable')
            txid = norm(tx['hash'])
            if txid in transactions_seen:
                raise NotObservable('EVM reconciliation repeats a transaction')
            transactions_seen.add(txid)
            if norm(tx['from']) != norm(sender):
                continue
            receipt = read('eth_getTransactionReceipt', [tx['hash']])
            if receipt is None:
                raise Pending('receipt of %s is unavailable' % tx['hash'])
            if not isinstance(receipt, dict):
                raise NotObservable('EVM receipt body is malformed')
            if norm(receipt.get('transactionHash')) != txid or norm(receipt.get('blockHash')) != target \
                    or _eth_quantity(receipt.get('blockNumber'), 'receipt block number', 64) != number \
                    or _eth_quantity(receipt.get('transactionIndex'), 'receipt transaction index', 64) != index:
                raise ContextMoved('EVM receipt is not for the frozen block/transaction position')
            status = _eth_quantity(receipt.get('status'), 'receipt status', 8)
            if status not in (0, 1):
                raise NotObservable('EVM receipt status is neither success nor failure')
            spends.append(dict(hash=txid, block=number, blockHash=target,
                valueWei=_eth_quantity(tx.get('value'), 'EVM transaction value', 256),
                gasWei=_eth_quantity(receipt.get('gasUsed'), 'receipt gas used', 64) *
                    _eth_quantity(receipt.get('effectiveGasPrice'), 'receipt gas price', 256), status=receipt['status']))
        groups.append(spends)
        target = norm(parent)
    if target != parent_hash:
        raise ContextMoved('EVM fee history does not descend from the before-balance block')
    canonical_endpoint()
    return [spend for group in reversed(groups) for spend in group]


class AnchorReorged(NotObservable):
    pass


def accepted_transactions(rpc, anchor, wanted, scan, *, end=None):
    """Locate wanted transaction ids accepted on the selected chain after `anchor`, with their full bodies.
    `wanted=None` collects every accepted transaction of the window (bounded by the plan's scan bounds).
    `end` fixes an inclusive selected-chain endpoint; later blocks never enter the measurement."""
    collect_all = wanted is None
    wanted = set() if collect_all else {norm(txid) for txid in wanted}
    anchor, end = norm(anchor), norm(end)
    found, start, blocks, reads = {}, anchor, 0, 0
    reached = end is not None and end == anchor
    while not reached and (end is not None or collect_all or wanted - set(found)):
        chain = rpc.call('getVirtualChainFromBlock', {'startHash': start, 'includeAcceptedTransactionIds': True,
                                                      'minConfirmationCount': None})
        if chain.get('removedChainBlockHashes'):
            raise AnchorReorged('the scan anchor %s left the selected chain' % norm(start)[:16])
        added = [norm(block) for block in chain.get('addedChainBlockHashes') or []]
        page = added
        if end is not None and end in added:
            page = added[:added.index(end) + 1]
            reached = True
        blocks += len(added)
        if blocks > scan['maximumScannedChainBlocks']:
            raise NotObservable('the accepted-transaction scan exceeds its predeclared chain-block bound')
        for entry in chain.get('acceptedTransactionIds') or []:
            accepting = norm(entry['acceptingBlockHash'])
            if end is not None and accepting not in page:
                if accepting in added:
                    continue  # beyond the frozen endpoint in this same RPC page
                raise NotObservable('accepted transaction entry is outside its selected-chain page')
            accepted = {norm(txid) for txid in entry.get('acceptedTransactionIds') or []}
            hits = accepted - set(found) if collect_all else accepted & (wanted - set(found))
            if hits:
                reads = _bodies(rpc, entry['acceptingBlockHash'], hits, found, reads, scan)
                if collect_all and hits - set(found):
                    raise NotObservable('accepted transaction bodies are incomplete; total costs stay UNKNOWN')
        if not added:
            break
        start = added[-1]
    if end is not None:
        if not reached:
            raise NotObservable('the frozen accepted-transaction endpoint was not reached on the selected chain')
        current = rpc.call('getVirtualChainFromBlock', {'startHash': end, 'includeAcceptedTransactionIds': False,
                                                       'minConfirmationCount': None})
        if current.get('removedChainBlockHashes'):
            raise AnchorReorged('the frozen accepted-transaction endpoint left the selected chain')
        blocks += len(current.get('addedChainBlockHashes') or [])
        if blocks > scan['maximumScannedChainBlocks']:
            raise NotObservable('the endpoint recheck exceeds the predeclared chain-block bound')
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


COVERAGE_FILE = 'rfc06-exec-origin-coverage-v2.json'


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
    """The coverage file inside the role's own generation bracket; the judgement requires its sink, DAA and local import epoch to match."""
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


def rfc12_state_value(rpc, outpoints=(), block_hash=''):
    """Exact point/reserve read; a failed read is never an absent/spent UTXO."""
    if len(outpoints) > 64 or len(set(outpoints)) != len(outpoints):
        raise NotObservable('RFC12 point read needs at most 64 distinct outpoints')
    for point in outpoints:
        if not isinstance(point, str) or not re.fullmatch(r'[0-9a-f]{128}:[0-9]+', point) \
                or int(point.rsplit(':', 1)[1]) > 2**32 - 1:
            raise NotObservable('RFC12 point read contains a malformed outpoint')
    try:
        # Validation's op246 is canonical. Preserve its known/present distinction
        # while checking the raw lists before dictionaries could hide omissions.
        requested_blocks = [block_hash] if block_hash else []
        response = rpc.call(ACCEPTANCE_READS_METHOD, dict(reserveBlocks=requested_blocks, outpoints=list(outpoints)))
        if not isinstance(response, dict) or response.get('available') is not True:
            raise ValueError('op246 state read is unavailable')
        balances = response['reserveBalances']
        if not isinstance(balances, list) or len(balances) != len(requested_blocks):
            raise ValueError('missing or unsolicited reserve response')
        reserve = None
        for row in balances:
            if norm(row['blockHash']) != block_hash or type(row['known']) is not bool:
                raise ValueError('reserve response names another block or has no known flag')
            reserve = row['balanceSompi'] if row['known'] else None
        utxos = []
        for row in response['outpoints']:
            if type(row.get('present')) is not bool:
                raise ValueError('outpoint response has no exact presence flag')
            txid, index = row['outpoint'].rsplit(':', 1)
            if not index.isascii() or not index.isdecimal() or str(int(index)) != index:
                raise ValueError('outpoint response index is not canonical')
            entry = None
            if row['present']:
                version, script = row['scriptVersion'], row['scriptHex']
                if type(version) is not int or not 0 <= version <= 65535 or not isinstance(script, str) \
                        or len(script) % 2 or not re.fullmatch(r'[0-9a-fA-F]*', script):
                    raise ValueError('outpoint script is malformed')
                entry = dict(amount=row['amount'], scriptPublicKey='%04x' % version + script.lower(),
                             isCoinbase=row['isCoinbase'], blockDaaScore=row['blockDaaScore'])
            utxos.append(dict(outpoint=dict(transactionId=norm(txid), index=int(index)), utxoEntry=entry))
        value = dict(version=1, blockHash=block_hash or norm(response['sink']), reserveBalance=reserve,
                     virtualSelectedParent=norm(response['sink']), virtualDaaScore=response['virtualDaa'], utxos=utxos)
    except (OSError, RuntimeError, KeyError, TypeError, ValueError) as error:
        raise NotObservable('RFC12 reserve/outpoint read unavailable: %s' % error) from error
    if type(value.get('version')) is not int or value['version'] != 1 or not isinstance(value.get('utxos'), list):
        raise NotObservable('unsupported RFC12 state snapshot')
    if type(value.get('virtualDaaScore')) is not int or not 0 <= value['virtualDaaScore'] < 2**64:
        raise NotObservable('RFC12 snapshot virtual DAA is not an exact u64')
    if any(not isinstance(value.get(k), str) or not re.fullmatch(r'[0-9a-f]{128}', value[k])
           for k in ('blockHash', 'virtualSelectedParent')):
        raise NotObservable('RFC12 snapshot omits its exact block/virtual generation')
    reserve = value.get('reserveBalance')
    if reserve is not None and (type(reserve) is not int or not 0 <= reserve < 2**64):
        raise NotObservable('reserve balance is not an exact u64')
    if block_hash and norm(value.get('blockHash')) != norm(block_hash):
        raise NotObservable('reserve snapshot names another block')
    found = {}
    for row in value['utxos']:
        point = row.get('outpoint') or {}
        index = point.get('index')
        if type(index) is not int or not 0 <= index < 2**32:
            raise NotObservable('outpoint response index is not u32')
        key = '%s:%d' % (norm(point.get('transactionId')), index)
        if key not in outpoints or key in found or 'utxoEntry' not in row:
            raise NotObservable('missing, duplicate or unsolicited outpoint response')
        entry = row['utxoEntry']
        if entry is not None and (not isinstance(entry, dict) or type(entry.get('amount')) is not int
                                  or not 0 < entry['amount'] < 2**64 or type(entry.get('isCoinbase')) is not bool
                                  or 'scriptPublicKey' not in entry):
            raise NotObservable('outpoint response omits the exact UTXO value/script')
        found[key] = entry
    if set(found) != set(outpoints):
        raise NotObservable('outpoint response omits a requested point')
    value['pointStates'] = found
    return value
