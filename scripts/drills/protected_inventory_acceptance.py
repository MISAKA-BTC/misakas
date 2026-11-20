"""Read an EOF carrier transaction's accepting DAA for inventory-continue.

Transaction acceptance does not establish that the PALW inventory action took
effect. The subsequent live tokenizer quote must authenticate the completed
EOF and its class/container/semantics parents. This reader never submits.
"""
import argparse
import json
from pathlib import Path

from misaka_wrpc_json import WsRpc
from protected_claim_observation import block_read, chain_read, consensus_params_id, hash64, require, unsigned
from rfc12_acceptance_observe import ContextMoved, context


class AcceptancePending(ValueError):
    """No accepting point yet; callers may poll without resubmitting."""


def observe_transaction(rpc, expected, *, maximum_chain_blocks=12_000):
    require(expected.get('version') == 1, 'unknown expectation version')
    consensus_params_id(expected['consensusParamsId'])
    for name in ('genesisHash', 'transactionId', 'scanFromBlock'):
        hash64(expected[name])
    require(0 < unsigned(maximum_chain_blocks) <= 100_000, 'invalid chain scan bound')
    dag, node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
    before = context(dag, node)
    require(node['genesisHash'] == expected['genesisHash']
            and node['consensusParamsId'] == expected['consensusParamsId'], 'another chain/profile')
    require(node['pruningImportStable'] is True, 'import is not stable')
    chain = chain_read(rpc, expected['scanFromBlock'], maximum_chain_blocks, tip=dag['sink'])
    added = chain['addedChainBlockHashes']
    require(len(set(added)) == len(added), 'duplicate selected-chain block')
    if not added and dag['sink'] == expected['scanFromBlock'] and not chain['acceptedTransactionIds']:
        raise AcceptancePending('carrier has no unique acceptance after the scan anchor')
    require(added and added[-1] == dag['sink'], 'scan did not reach the observed sink')
    for block in added:
        hash64(block)
    rows = chain['acceptedTransactionIds']
    require(len(rows) <= len(added), 'too many accepting rows')
    row_blocks = [row['acceptingBlockHash'] for row in rows]
    require(len(set(row_blocks)) == len(row_blocks) and set(row_blocks) <= set(added),
            'acceptance rows do not match the selected chain')
    matching = []
    for row in rows:
        txids = row['acceptedTransactionIds']
        count = txids.count(expected['transactionId'])
        require(count <= 1, 'duplicate carrier acceptance within a block')
        if count:
            matching.append(row)
    if not matching:
        raise AcceptancePending('carrier has no unique acceptance after the scan anchor')
    require(len(matching) == 1, 'carrier has no unique acceptance after the scan anchor')
    accepting = block_read(rpc, matching[0]['acceptingBlockHash'])
    require(accepting['verboseData']['isChainBlock'] is True, 'accepting block is not selected')
    accepted_daa = unsigned(accepting['header']['daaScore'])
    require(accepted_daa <= unsigned(dag['virtualDaaScore']), 'acceptance is after the observed tip')
    after = context(rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus'))
    if before != after:
        raise ContextMoved('native generation changed during carrier acceptance readback')
    return dict(version=1, kind='protectedCarrierTransactionAcceptance', source='node-rpc',
                expected=expected, context=list(before), acceptingBlock=accepting['header']['hash'],
                acceptedDaa=accepted_daa, acceptedTransactionIds=matching[0], nativeEffectAuthenticated=False)


def observe(rpc, expected, *, maximum_chain_blocks=12_000):
    result = observe_transaction(rpc, expected, maximum_chain_blocks=maximum_chain_blocks)
    result.update(kind='protectedInventoryCarrierAcceptance', acceptedOpenDaa=result['acceptedDaa'],
                  nativeInventoryAccepted=False, nativeTokenizerRegistered=False,
                  next='inventory-continue uses acceptedOpenDaa; require a live TKR1 quote after EOF Finish')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, required=True)
    parser.add_argument('--expected', type=Path, required=True,
                        help='version 1 JSON: genesisHash, consensusParamsId, transactionId, scanFromBlock (before submission)')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--maximum-chain-blocks', type=int, default=12_000)
    parser.add_argument('--purpose', choices=('inventory-open', 'class-semantics'), default='inventory-open',
                        help='class-semantics returns the transaction accepting point; an earlier native binding still takes precedence')
    args = parser.parse_args()
    expected = json.loads(args.expected.read_text())
    rpc = WsRpc(host='127.0.0.1', port=args.port, timeout=30)
    try:
        reader = observe if args.purpose == 'inventory-open' else observe_transaction
        result = reader(rpc, expected, maximum_chain_blocks=args.maximum_chain_blocks)
        result['purpose'] = args.purpose
    finally:
        rpc.close()
    with args.output.open('x') as output:
        json.dump(result, output, indent=2)
        output.write('\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
