"""RFC-0012 §4.4 DNS retirement reconciliation: coinbase output classification and checkbox-7 judgement.

User decision 2026-10-15 §8 adopts (b)+(c): a private profile that arms the
retirement at a later DAA (so real DNS-validator history exists before the
fence), and classification of each coinbase output by its script, with no node
change. This module is the (c) half.

Every coinbase output of every scanned chain block is classified from node reads
only:
  dns-validator  the P2PKH-ML-DSA-87 script of a DNS stake bond's
                 owner_reward_spk_payload, read from the bond's own accepted
                 StakeBond transaction (subnetwork 0x10; the payload's last 64
                 bytes) and cross-checked against getStakeBonds;
  miner          a script some read block's coinbase payload names as its miner;
  palw-payout    an address of a known PALW bond payout (the factory/keyring seats);
  miner-or-palw  both of the last two (non-DNS either way);
  ambiguous-dns  a DNS script that is also a miner or PALW payout;
  unclassified   none of the above.
An unclassified or ambiguous-dns output makes the classification predicate
BLOCKED; nothing is inferred. Every read is bounded by the plan.

Judgement (checkbox 7): before the fence a DNS validator share is present and
every DNS output pays a bond whose node-reported status at that block's DAA is
active; from the fence on no block carries a retired kind (stake bond 0x10,
attestation shard 0x11, precommit 0x19) and no coinbase carries a DNS share,
except historical liabilities the node settles once at their original epoch
finalization (deferred quality bonus / reserve drip), which must lie inside the
plan's explicit legacy settlement window and pay only bonds entitled before the
fence. A window of 0 is the strict reading. No verdict is PASS.
"""
import rfc12_acceptance_judgement as J

P2PKH_PREFIX, P2PKH_SUFFIX = '76c440', '88a6'
SPK_VERSION_HEX = '0000'
STAKE_BOND_KIND = 0x10
RETIRED_KINDS = {0x10: 'stake-bond', 0x11: 'attestation-shard', 0x19: 'precommit'}
VALIDATOR_PUBKEY_LEN = 2592
# version u16 | owner_pubkey_hash 64 | validator_pubkey_hash 64 | validator_pubkey Vec<u8> (u32 len + 2592)
# | amount u64 | activation_daa_score u64 | unbonding_period_blocks u64 | owner_reward_spk_payload 64
STAKE_BOND_PAYLOAD_LEN = 2 + 64 + 64 + 4 + VALIDATOR_PUBKEY_LEN + 8 + 8 + 8 + 64
DNS, MINER, PALW, NON_DNS, AMBIGUOUS, UNCLASSIFIED = (
    'dns-validator', 'miner', 'palw-payout', 'miner-or-palw', 'ambiguous-dns', 'unclassified')


def hexbytes(value, name):
    if not isinstance(value, str) or len(value) % 2 or any(c not in '0123456789abcdefABCDEF' for c in value):
        raise ValueError(name + ' is not hex')
    return bytes.fromhex(value)


def p2pkh_spk(payload_hex):
    """The JSON form of p2pkh_mldsa87_spk(payload): version 0 (BE) then OP_DUP OP_BLAKE2B_512 OP_DATA64 <64> OP_EQUALVERIFY
    OP_CHECKSIG_MLDSA87 (consensus/core/src/mldsa87_primitives.rs)."""
    payload = hexbytes(payload_hex, 'payout payload')
    if len(payload) != 64:
        raise ValueError('a P2PKH-ML-DSA-87 payload is 64 bytes')
    return SPK_VERSION_HEX + P2PKH_PREFIX + payload.hex() + P2PKH_SUFFIX


def parse_stake_bond_payload(payload_hex):
    """Exact Borsh StakeBondPayload (consensus/core/src/dns_finality.rs); any other shape is refused."""
    raw = hexbytes(payload_hex, 'stake bond payload')
    if len(raw) != STAKE_BOND_PAYLOAD_LEN:
        raise ValueError('stake bond payload is %d bytes, not %d' % (len(raw), STAKE_BOND_PAYLOAD_LEN))
    if int.from_bytes(raw[130:134], 'little') != VALIDATOR_PUBKEY_LEN:
        raise ValueError('stake bond validator key is not a 2592-byte ML-DSA-87 key')
    tail = 134 + VALIDATOR_PUBKEY_LEN
    return dict(version=int.from_bytes(raw[0:2], 'little'), ownerPubkeyHash=raw[2:66].hex(),
                validatorPubkeyHash=raw[66:130].hex(),
                amount=int.from_bytes(raw[tail:tail + 8], 'little'),
                activationDaaScore=int.from_bytes(raw[tail + 8:tail + 16], 'little'),
                unbondingPeriodBlocks=int.from_bytes(raw[tail + 16:tail + 24], 'little'),
                ownerRewardSpkPayload=raw[tail + 24:tail + 88].hex())


def coinbase_miner_spk(payload_hex):
    """Miner script of a coinbase payload: blue_score u64 | subsidy u64 | spk version u16 LE | len u8 | script | extra."""
    raw = hexbytes(payload_hex, 'coinbase payload')
    if len(raw) < 19:
        raise ValueError('coinbase payload too short')
    length = raw[18]
    if len(raw) < 19 + length:
        raise ValueError('coinbase payload script truncated')
    version = int.from_bytes(raw[16:18], 'little')
    return '%04x' % version + raw[19:19 + length].hex(), int.from_bytes(raw[8:16], 'little')


def subnetwork_kind(subnetwork_hex):
    """The single-byte subnetwork id of a native kind (`SubnetworkId::from_byte`), else None."""
    raw = hexbytes(subnetwork_hex, 'subnetwork id')
    if not raw or any(raw[1:]):
        return None
    return raw[0]


def bond_scripts(bond_txs, registry):
    """{script: bond outpoint} for every registry bond whose own accepted StakeBond transaction was read.

    bond_txs: {txid: payload hex}; registry: [{bondOutpoint, ownerPubkeyHash, validatorId, amount, activationDaaScore}].
    A parsed payload that disagrees with the node's registry entry is refused (NotObservable): the classification
    would then rest on bytes the node does not stand behind."""
    scripts, unread = {}, []
    for bond in registry:
        txid = J.norm(bond['bondOutpoint'].rsplit(':', 1)[0])
        payload = bond_txs.get(txid)
        if payload is None:
            unread.append(bond['bondOutpoint'])
            continue
        try:
            parsed = parse_stake_bond_payload(payload)
        except ValueError as error:
            raise J.NotObservable('bond %s transaction payload is not a StakeBondPayload: %s' % (bond['bondOutpoint'], error))
        if (parsed['ownerPubkeyHash'], parsed['validatorPubkeyHash'], parsed['amount'], parsed['activationDaaScore']) != \
                (J.norm(bond['ownerPubkeyHash']), J.norm(bond['validatorId']), bond['amount'], bond['activationDaaScore']):
            raise J.NotObservable('bond %s payload disagrees with the node registry entry' % bond['bondOutpoint'])
        scripts[p2pkh_spk(parsed['ownerRewardSpkPayload'])] = bond['bondOutpoint']
    return scripts, unread


def classify_output(output, dns_scripts, miner_scripts, palw_addresses):
    spk, address = output['spk'], output.get('address')
    is_dns, is_miner, is_palw = spk in dns_scripts, spk in miner_scripts, address in palw_addresses
    if is_dns:
        return (AMBIGUOUS if is_miner or is_palw else DNS), dns_scripts[spk]
    if is_miner and is_palw:
        return NON_DNS, None
    if is_miner:
        return MINER, None
    if is_palw:
        return PALW, None
    return UNCLASSIFIED, None


def classify_blocks(blocks, dns_scripts, miner_scripts, palw_addresses):
    out = []
    for block in blocks:
        outputs = []
        for output in block['coinbaseOutputs']:
            kind, bond = classify_output(output, dns_scripts, miner_scripts, palw_addresses)
            outputs.append(dict(output, kind=kind, bond=bond))
        out.append(dict(block, outputs=outputs))
    return out


# ---- judgement (checkbox 7) ---------------------------------------------------------------------

def classification_complete(blocks):
    J.require_real(blocks)
    if not blocks:
        raise J.NotObservable('no chain block was scanned')
    bad = [(block['hash'][:16], output['index'], output['kind']) for block in blocks for output in block['outputs']
           if output['kind'] in (UNCLASSIFIED, AMBIGUOUS)]
    if bad:
        raise J.NotObservable('%d coinbase outputs cannot be classified from node reads: %r' % (len(bad), bad[:8]))
    counts = {}
    for block in blocks:
        for output in block['outputs']:
            counts[output['kind']] = counts.get(output['kind'], 0) + 1
    return dict(blocks=len(blocks), outputsByKind=counts)


def pre_fence_dns_share(blocks, fence, status_at):
    """Before the fence the DNS validator share exists and pays only bonds the node reports active at that DAA."""
    J.require_real(blocks)
    pre = [block for block in blocks if block['daa'] < fence]
    if not pre:
        raise J.NotObservable('no chain block below the fence was scanned')
    paid, totals = 0, {}
    for block in pre:
        for output in block['outputs']:
            if output['kind'] != DNS:
                continue
            status = status_at.get((output['bond'], block['daa']))
            if status is None:
                raise J.NotObservable('the entitlement of %s at DAA %d was not read' % (output['bond'], block['daa']))
            if status != 'active':
                raise J.Violation('a pre-fence DNS share at DAA %d pays bond %s whose status there is %s'
                                  % (block['daa'], output['bond'], status))
            paid += 1
            totals[output['bond']] = totals.get(output['bond'], 0) + output['value']
    if not paid:
        raise J.NotObservable('no coinbase before the fence carries a DNS validator share; no DNS history accumulated')
    return dict(preFenceBlocks=len(pre), dnsOutputs=paid, perBondSompi=totals)


def post_fence_dns_share(blocks, bodies, fence, settlement_window, entitled):
    """From the fence: no block body at or above it carries a retired kind, and no coinbase carries a DNS share except
    once-settled legacy liabilities inside the window, paid only to bonds entitled before the fence.

    `bodies` are every block read (chain blocks and their mergesets), each judged at its OWN header DAA: a pre-fence
    block merged after the fence is history, not a post-fence inclusion."""
    J.require_real(blocks)
    post = [block for block in blocks if block['daa'] >= fence]
    if not post:
        raise J.NotObservable('no chain block at or after the fence was scanned')
    for body in bodies:
        retired = sorted(set(kind for kind in body['kinds'] if kind in RETIRED_KINDS))
        if body['daa'] >= fence and retired:
            raise J.Violation('block %s at DAA %d carries retired kind(s) %s after the fence'
                              % (body['hash'][:16], body['daa'], ', '.join(RETIRED_KINDS[kind] for kind in retired)))
    legacy, totals, last = 0, {}, None
    for block in post:
        for output in block['outputs']:
            if output['kind'] != DNS:
                continue
            if block['daa'] >= fence + settlement_window:
                raise J.Violation('a DNS validator share at DAA %d, after the fence and its legacy settlement window'
                                  % block['daa'])
            if output['bond'] not in entitled:
                raise J.Violation('a post-fence DNS output pays %s, which held no entitlement before the fence' % output['bond'])
            legacy += 1
            last = block['daa']
            totals[output['bond']] = totals.get(output['bond'], 0) + output['value']
    return dict(postFenceBlocks=len(post), legacySettlementOutputs=legacy, legacySettlementSompi=totals,
                lastLegacySettlementDaa=last, settlementWindowDaa=settlement_window)


def legacy_exit_paid_once(record):
    """A pre-fence DNS bond's post-fence exit pays its principal once to the declared destination."""
    if not record.get('commands'):
        raise J.NotObservable('the plan drives no legacy bond exit')
    if not record.get('released'):
        raise J.NotObservable('the exiting bond did not reach its release height within the bound')
    J.require_real([record['destinationAfter']])
    return J.exactly_one_new_utxo(record['destinationBefore'], record['destinationAfter']['value']['entries'],
                                  record['amount'] - record['maximumFeeSompi'], record['amount'])


def reserve_balance(view):
    raise J.NotObservable('the DNS security reserve balance (reserve_balance_store) is served by no read; reserve drip '
                          'outputs are classified above, the balance itself is not reconciled')


# ---- bounded node reads -------------------------------------------------------------------------

class ReadBudget:
    def __init__(self, maximum):
        self.maximum, self.used = maximum, 0

    def take(self, count=1):
        self.used += count
        if self.used > self.maximum:
            raise J.NotObservable('the retirement scan exceeds its predeclared read bound (%d reads)' % self.maximum)


def spk_hex(value):
    if isinstance(value, str):
        return value.lower()
    if isinstance(value, dict) and 'script' in value:
        return '%04x' % value.get('version', 0) + J.norm(value['script'])
    raise ValueError('unknown scriptPublicKey shape')


def chain_hashes(rpc, start, maximum):
    hashes, cursor = [], start
    while True:
        chain = rpc.call('getVirtualChainFromBlock', {'startHash': cursor, 'includeAcceptedTransactionIds': False,
                                                      'minConfirmationCount': None})
        if chain.get('removedChainBlockHashes'):
            raise J.NotObservable('the scan start left the selected chain')
        added = [J.norm(item) for item in chain.get('addedChainBlockHashes') or []]
        if not added:
            return hashes
        hashes.extend(added)
        if len(hashes) > maximum:
            raise J.NotObservable('the selected chain exceeds the predeclared chain-block bound (%d)' % maximum)
        cursor = added[-1]


def read_block(rpc, block_hash, budget, cache):
    if block_hash not in cache:
        budget.take()
        cache[block_hash] = rpc.call('getBlock', {'hash': block_hash, 'includeTransactions': True})['block']
    return cache[block_hash]


def body_of(block_hash, block):
    transactions = block.get('transactions') or []
    if not transactions:
        raise J.NotObservable('block %s serves no body (pruned or header-only); scan an archival role' % block_hash[:16])
    return transactions


def scan_window(rpc, genesis, from_daa, to_daa, maximum_chain, budget):
    """Chain blocks with from_daa <= DAA <= to_daa, their coinbase outputs, every merged body's kinds at its own DAA,
    every miner script any read coinbase names, and every StakeBond transaction payload read on the way."""
    hashes = chain_hashes(rpc, genesis, maximum_chain)
    cache = {}
    low, high = 0, len(hashes)
    while low < high:  # first chain block at or above from_daa
        middle = (low + high) // 2
        if read_block(rpc, hashes[middle], budget, cache)['header']['daaScore'] < from_daa:
            low = middle + 1
        else:
            high = middle
    blocks, bodies, bond_txs, miners = [], {}, {}, set()
    for block_hash in hashes[low:]:
        block = read_block(rpc, block_hash, budget, cache)
        daa = block['header']['daaScore']
        if daa > to_daa:
            break
        verbose = block.get('verboseData') or {}
        members = [(block_hash, block)] + [(J.norm(item), read_block(rpc, J.norm(item), budget, cache))
                                           for item in (verbose.get('mergeSetBluesHashes') or []) + (verbose.get('mergeSetRedsHashes') or [])]
        for member_hash, member in members:
            if member_hash in bodies:
                continue
            transactions = body_of(member_hash, member)
            miners.add(coinbase_miner_spk(transactions[0]['payload'])[0])
            kinds = []
            for tx in transactions[1:]:
                kind = subnetwork_kind(tx['subnetworkId'])
                if kind is not None:
                    kinds.append(kind)
                if kind == STAKE_BOND_KIND:
                    bond_txs[J.norm((tx.get('verboseData') or {}).get('transactionId'))] = tx['payload']
            bodies[member_hash] = dict(source='node-rpc', hash=member_hash, daa=member['header']['daaScore'], kinds=kinds)
        coinbase = body_of(block_hash, block)[0]
        outputs = [dict(index=index, value=output['value'], spk=spk_hex(output['scriptPublicKey']),
                        address=(output.get('verboseData') or {}).get('scriptPublicKeyAddress'))
                   for index, output in enumerate(coinbase['outputs'])]
        blocks.append(dict(source='node-rpc', hash=block_hash, daa=daa, coinbaseOutputs=outputs))
    return dict(blocks=blocks, bodies=list(bodies.values()), bondTxs=bond_txs, minerScripts=sorted(miners),
                readsUsed=budget.used, chainBlocks=len(hashes))


def bond_registry(rpc, page_limit, maximum_pages, budget, pov=None):
    bonds, cursor = [], None
    for _ in range(maximum_pages):
        budget.take()
        answer = rpc.call('getStakeBonds', {'ownerPubkeyHash': None, 'statusIn': None, 'cursor': cursor, 'limit': page_limit,
                                            'povDaaScore': pov})
        bonds.extend(answer.get('bonds') or [])
        cursor = answer.get('nextCursor')
        if not cursor:
            return bonds
    raise J.NotObservable('the stake bond registry exceeds its predeclared page bound')


def statuses_at(rpc, daas, page_limit, maximum_pages, budget):
    """{(bond outpoint, DAA): effective status} as the node reports it at each point-of-view DAA."""
    status = {}
    for daa in sorted(set(daas)):
        for bond in bond_registry(rpc, page_limit, maximum_pages, budget, daa):
            status[(bond['bondOutpoint'], daa)] = bond['effectiveStatus']
    return status
