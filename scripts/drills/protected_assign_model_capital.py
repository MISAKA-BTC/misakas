"""Submit a private bond's first signed model-capital assignment before Studio admission."""
import argparse
import json
from pathlib import Path
import subprocess

from misaka_wrpc_json import WsRpc
from protected_private_registration_run import require_independent_funding
from protected_registration_workflow import chain_identity


def submit_first_assignment(rpc, manifest, signer, bond, model, amount):
    bootstrap = json.loads(manifest.read_text())
    profile = bootstrap['profile']
    if (profile['kind'] != 'rfc12-private-integration-probe' or profile['releaseApproval'] is not False
            or profile['protectedClaimPrivatePolicyV3']['releaseApproval'] is not False):
        raise ValueError('private protected profile required')
    expected = dict(genesisHash=profile['genesis'], consensusParamsId=profile['consensusParamsId'])
    chain_identity(rpc, expected)
    owner, = [seat['bond'] for seat in bootstrap['seats'] if seat['bond']['bondOutpoint'] == bond]
    txid, index = bond.split(':')
    facts = rpc.call('getPalwProducerFacts', dict(classId=model, bondTransactionId=txid, bondIndex=int(index), withBond=True))
    if not (facts['available'] and facts['bondKnown'] and facts['classId'] == model and facts.get('artifactRoot')
            and 0 < amount <= int(facts['bondCollateral'])):
        raise ValueError('registered class, known owner bond and covered amount required')
    response = rpc.call('getPalwFenceSchedule', dict(view='capital-assignment', key=bond))
    if not response['available'] or not response['found']:
        raise ValueError('native capital assignment view unavailable')
    view = json.loads(response['viewJson'])
    if view['fence']['status'] != 'active' or view['assigned'] or view['sequence']:
        raise ValueError('first assignment requires an active fence and an unassigned bond')
    funding = require_independent_funding(rpc, owner, 1_000_000, require_reservation=False)
    request = {'manifest': str(manifest.resolve()), 'genesis': profile['genesis'], 'bond': bond,
               'class': model, 'amountSompi': amount, 'funding': {**funding, 'amountSompi': int(funding['amountSompi'])}}
    signed = json.loads(subprocess.check_output([signer], input=json.dumps(request).encode()))
    sent = rpc.call('submitTransaction', dict(transaction=signed['transaction'], allowOrphan=False))
    if sent['transactionId'] != signed['id']:
        raise ValueError('accepted transaction ID differs from locally signed ID')
    return sent['transactionId']


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--signer', required=True, type=Path)
    parser.add_argument('--port', required=True, type=int)
    parser.add_argument('--bond', required=True)
    parser.add_argument('--model', required=True)
    parser.add_argument('--amount-sompi', required=True, type=int)
    args = parser.parse_args()
    client = WsRpc(host='127.0.0.1', port=args.port, timeout=60)
    try:
        print(submit_first_assignment(client, args.manifest, args.signer, args.bond, args.model, args.amount_sompi))
    finally:
        client.close()
