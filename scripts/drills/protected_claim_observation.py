"""Read one protected Studio claim from an unchanged native RPC generation.

No registration, state insertion, clock advancement or transaction submission.
These are node observations, not independent consensus validation or evidence
that a Panel process actually executed its worker. Receipt envelope validation
is a separate native step (protected_receipt_observe.rs).
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

from misaka_wrpc_json import WsRpc
from rfc12_acceptance_observe import ContextMoved, context


ORIGIN_FIELDS = (
    'claim', 'executionRoot', 'original', 'carrierTransaction', 'carrierBlock',
    'acceptingBlock', 'acceptedTxOrdinal', 'eventOrdinal', 'acceptedDaa',
    'acceptedBlueScore',
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def hash64(value):
    require(isinstance(value, str) and len(value) == 128
            and all(c in '0123456789abcdef' for c in value) and value != '0' * 128,
            'expected a nonzero canonical Hash64')
    return value


def consensus_params_id(value):
    require(isinstance(value, str) and len(value) == 64
            and all(c in '0123456789abcdef' for c in value) and value != '0' * 64,
            'expected a nonzero canonical 32-byte consensus params ID')
    return value


def unsigned(value, bits=64):
    require(type(value) is int and 0 <= value < 2 ** bits, 'expected an exact unsigned integer')
    return value


def check_expectation(expected):
    require(expected.get('version') == 1, 'unknown expectation version')
    consensus_params_id(expected['consensusParamsId'])
    for key in ('genesisHash', 'networkDomain', 'claim', 'transactionId', 'executionRoot'):
        hash64(expected[key])


def expectation_from_studio(studio_run, studio_job, rail, chain):
    """Bind Studio's job to the submitted protected rail carrier."""
    job = hash64(studio_run['fp_job_id'])
    require(studio_job.get('object') == 'misaka.request'
            and studio_job.get('job_id') == job, 'Studio status names another job')
    require(hash64(studio_job['output_root']) == hash64(studio_run['output_root']),
            'Studio output changed')
    claim = hash64(studio_job['claim_id'])
    txid = hash64(studio_job['submission_txid'])
    require(rail.get('schema') == 'misaka.palw.fp-rail-tx.v1'
            and rail.get('fp_claim_id') == claim and rail.get('submitted') == txid,
            'rail summary names another submitted claim')
    require(unsigned(rail['protected_funding_sompi']) > 0, 'rail carrier lacks protected funding')
    expected = dict(version=1, genesisHash=chain['genesisHash'],
                    consensusParamsId=chain['consensusParamsId'], networkDomain=chain['networkDomain'],
                    claim=claim, transactionId=txid, executionRoot=rail['protected_execution_root'])
    check_expectation(expected)
    return expected


def origin_of(detail, expected):
    row = detail.get('protectedOriginal')
    require(isinstance(row, dict) and row.get('version') == 1 and 'error' not in row,
            'native frame/admission link unavailable')
    for name in ORIGIN_FIELDS[:6]:
        hash64(row[name])
    unsigned(row['acceptedTxOrdinal'], 32)
    unsigned(row['eventOrdinal'], 32)
    unsigned(row['acceptedDaa'])
    unsigned(row['acceptedBlueScore'])
    require(row['claim'] == expected['claim'] == detail['claim']['claimId'], 'claim changed')
    require(row['carrierTransaction'] == expected['transactionId'], 'carrier transaction changed')
    require(row['executionRoot'] == expected['executionRoot'], 'execution root changed')
    require(row['observedTip'] == detail['tipBlock'] and row['observedTipDaa'] == detail['tipDaa'],
            'claim detail mixes native snapshots')
    require(unsigned(detail['tipDaa']) >= row['acceptedDaa'], 'readback precedes admission')
    for name in ('fullReplayApprovalRecorded', 'publicRefutationRecorded'):
        require(type(row[name]) is bool, 'missing native approval/refutation flag')
    return {name: row[name] for name in ORIGIN_FIELDS}


def block_read(rpc, block_hash, transactions=False):
    block = rpc.call('getBlock', {'hash': hash64(block_hash), 'includeTransactions': transactions})['block']
    require(block['header']['hash'] == block_hash == block['verboseData']['hash'], 'block identity changed')
    require(block['verboseData']['isHeaderOnly'] is False, 'block body was not validated/retained')
    return block


def chain_read(rpc, start, maximum, *, tip=None, include_transactions=True):
    cursor = hash64(start)
    result = dict(addedChainBlockHashes=[], acceptedTransactionIds=[])
    seen = {cursor}
    while cursor != tip:
        page = rpc.call('getVirtualChainFromBlock', {
            'startHash': cursor, 'includeAcceptedTransactionIds': include_transactions, 'minConfirmationCount': None,
        })
        if page['removedChainBlockHashes']:
            raise ContextMoved('required block left the selected chain')
        added = page['addedChainBlockHashes']
        require(len(added) <= maximum - len(result['addedChainBlockHashes']),
                'chain read exceeds the declared scan bound')
        for block in added:
            require(hash64(block) not in seen, 'duplicate selected-chain block')
            seen.add(block)
        result['addedChainBlockHashes'].extend(added)
        result['acceptedTransactionIds'].extend(page['acceptedTransactionIds'])
        if tip is None:
            break
        if not added or (tip in seen and added[-1] != tip):
            raise ContextMoved('selected-chain page did not end at the observed tip')
        cursor = added[-1]
    return result


def claim_snapshot(rpc, expected, previous):
    dag, node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
    before = context(dag, node)
    require(node['genesisHash'] == expected['genesisHash']
            and node['consensusParamsId'] == expected['consensusParamsId'], 'another chain/profile')
    require(node['pruningImportStable'] is True, 'import is not stable')
    response = rpc.call('getPalwClaimDetail', {'claimHex': expected['claim']})
    require(response['available'] is True and response['found'] is True, 'claim is not admitted/readable')
    detail = json.loads(response['viewJson'])
    require(detail['kind'] == 'palwClaimDetail' and detail['version'] == 1, 'unknown claim detail schema')
    origin = origin_of(detail, expected)
    if detail['tipBlock'] != dag['sink']:
        raise ContextMoved('PALW readback is not at the observed native sink')
    if previous is not None:
        require(previous['expected'] == expected and previous['origin'] == origin,
                'original inclusion changed since the earlier lifecycle observation')

    return dag, before, detail, origin


def observe_claim_progress(rpc, expected, previous):
    """Read current claim state against an already verified admission."""
    check_expectation(expected)
    _, before, detail, _ = claim_snapshot(rpc, expected, previous)
    if before != context(rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')):
        raise ContextMoved('native generation changed during protected claim progress read')
    return detail


def observe(rpc, expected, *, previous=None, receipt=None, panel_approval=None, panel_log=None,
            panel_logs=None, excluded_panel_bond=None, maximum_chain_blocks=12_000):
    check_expectation(expected)
    require(0 < unsigned(maximum_chain_blocks) <= 100_000, 'invalid chain scan bound')
    dag, before, detail, origin = claim_snapshot(rpc, expected, previous)

    accepting = block_read(rpc, origin['acceptingBlock'])
    require(accepting['verboseData']['isChainBlock'] is True, 'admission block is not selected')
    require(accepting['header']['daaScore'] == origin['acceptedDaa'], 'admission DAA differs from native readback')
    require(accepting['header']['blueScore'] == origin['acceptedBlueScore'],
            'admission blue score differs from native readback')
    chain = chain_read(rpc, accepting['verboseData']['selectedParentHash'], maximum_chain_blocks, tip=dag['sink'])
    require(origin['acceptingBlock'] in chain['addedChainBlockHashes'], 'admission absent from selected chain')
    entries = [row for row in chain['acceptedTransactionIds'] if row['acceptingBlockHash'] == origin['acceptingBlock']]
    require(len(entries) == 1, 'claim carrier transaction was not accepted at its recorded point')
    accepted = entries[0]['acceptedTransactionIds']
    ordinal = origin['acceptedTxOrdinal']
    require(ordinal < len(accepted) and accepted[ordinal] == expected['transactionId'],
            'claim carrier transaction was not accepted at its recorded ordinal')
    require(accepted.count(expected['transactionId']) == 1, 'duplicate claim carrier acceptance')
    merged = accepting['verboseData']['mergeSetBluesHashes'] + accepting['verboseData']['mergeSetRedsHashes']
    require(origin['carrierBlock'] in merged, 'carrier is outside the accepting mergeset')
    carrier = block_read(rpc, origin['carrierBlock'], True)
    matching = [tx for tx in carrier['transactions'] if tx['verboseData']['transactionId'] == expected['transactionId']]
    require(len(matching) == 1, 'exact claim carrier body unavailable')

    if panel_log is not None:
        require(panel_approval is None, 'choose one Panel approval source')
        panel_approval = approval_from_panel_log(panel_log, origin, chain['acceptedTransactionIds'])

    if panel_logs is not None:
        require(panel_log is None and panel_approval is None, 'choose one Panel approval source')
        accepted_rows = detail.get('protectedReplayApprovals', [])
        logged = panel_approval_candidates(panel_logs, origin)
        candidates = [
            row['carrierTransaction'] for row in accepted_rows
            if row['bond'] != excluded_panel_bond and row['carrierTransaction'] in logged
        ]
        if not candidates:
            raise ContextMoved('awaiting a matching accepted approval from an independent Panel seat')
        panel_approval = candidates[0]

    approval_entry = None
    if panel_approval is not None:
        require(hash64(panel_approval) != expected['transactionId'], 'approval txid equals original carrier')
        require(detail['protectedOriginal']['fullReplayApprovalRecorded'] is True,
                'native claim has no full-replay approval')
        approvals = [row for row in chain['acceptedTransactionIds']
                     if panel_approval in row['acceptedTransactionIds']]
        require(len(approvals) == 1, 'Panel approval transaction lacks a unique accepting point')
        blocks = chain['addedChainBlockHashes']
        approval_block = approvals[0]['acceptingBlockHash']
        require(approval_block in blocks and blocks.index(approval_block) >= blocks.index(origin['acceptingBlock']),
                'Panel approval predates original admission')
        stored = [row for row in detail.get('protectedReplayApprovals', [])
                  if row['carrierTransaction'] == panel_approval]
        require(len(stored) == 1 and stored[0]['acceptingBlock'] == approval_block,
                'Panel transaction does not match the stored approval for this original')
        approval_entry = stored[0]
        ordinal = unsigned(approval_entry['acceptedTxOrdinal'], 32)
        require(ordinal < len(approvals[0]['acceptedTransactionIds'])
                and approvals[0]['acceptedTransactionIds'][ordinal] == panel_approval,
                'Panel approval acceptance ordinal differs')
        if approval_block == origin['acceptingBlock']:
            require(ordinal > origin['acceptedTxOrdinal'], 'Panel approval predates original admission in the same block')
        approval_header = block_read(rpc, approval_block)
        require(approval_header['header']['daaScore'] == unsigned(approval_entry['acceptedDaa']),
                'Panel approval acceptance DAA differs')
        approval_merged = (approval_header['verboseData']['mergeSetBluesHashes']
                           + approval_header['verboseData']['mergeSetRedsHashes'])
        require(approval_entry['carrierBlock'] in approval_merged, 'approval carrier is outside the accepting mergeset')
        approval_carrier = block_read(rpc, approval_entry['carrierBlock'], True)
        require(sum(tx['verboseData']['transactionId'] == panel_approval
                    for tx in approval_carrier['transactions']) == 1, 'exact approval carrier body unavailable')

    receipt_block = None
    if receipt is not None:
        receipt_block = block_read(rpc, receipt)
        require(receipt_block['verboseData']['isChainBlock'] is True, 'receipt block is not on the selected chain')
        require(receipt_block['header']['powAlgoId'] == 7, 'block is not a PFS3 receipt')
        selected = chain['addedChainBlockHashes']
        require(receipt in selected, 'receipt block is outside the admission-to-tip history')
        prerequisite = origin['acceptingBlock'] if approval_entry is None else approval_entry['acceptingBlock']
        require(selected.index(receipt) > selected.index(prerequisite), 'receipt block does not follow its prerequisite')
        claim = detail['claim']
        receipt_daa = unsigned(receipt_block['header']['daaScore'])
        require(claim['phase'] == 'final'
                and origin['acceptedDaa'] < unsigned(claim['phaseDaa']) <= receipt_daa <= detail['tipDaa'],
                'receipt DAA does not match a Final claim in the observed history')

    after = context(rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus'))
    if before != after:
        raise ContextMoved('native generation changed during protected claim readback')
    return dict(version=1, kind='protectedClaimObservation', source='node-rpc', expected=expected,
                context=list(before), origin=origin, detail=detail, acceptingBlock=accepting,
                acceptedTransactionIds=entries[0], carrierBlock=carrier, receiptBlock=receipt_block,
                panelApprovalAcceptance=approval_entry,
                nativeReceiptEnvelopeValidated=False, actualClaimCompletion=False)


def panel_approval_candidates(paths, origin):
    pattern = re.compile(
        r'submitted a full-replay approval claim=' + hash64(origin['claim'])
        + r' original=' + hash64(origin['original'])
        + r' accepting_block=' + hash64(origin['acceptingBlock'])
        + r' txid=([0-9a-f]{128})(?![0-9a-f])'
    )
    matches = set()
    for path in paths:
        with path.open() as stream:
            for line in stream:
                match = pattern.search(line)
                if match:
                    matches.add(match[1])
    return matches


def approval_from_panel_log(path, origin, accepted):
    accepted_ids = {txid for row in accepted for txid in row['acceptedTransactionIds']}
    matches = panel_approval_candidates([path], origin) & accepted_ids
    require(len(matches) == 1, 'expected one accepted Panel approval from this log for the exact original')
    return matches.pop()


def receipt_from_producer_log(path, claim, *, rpc=None, spent_indices=None):
    """Locate a receipt for this claim in the owned producer log."""
    claim = hash64(claim)
    pattern = re.compile(
        r'PFS3 submitted block=([0-9a-f]{128}) claim=' + claim + r' quantum=([0-9]+) parent='
    )
    candidates = set()
    with path.open() as stream:
        for line in stream:
            match = pattern.search(line)
            if match:
                candidates.add((match[1], unsigned(int(match[2]), 32)))
    if rpc is not None:
        # A stale template may have been accepted into the DAG without
        # consuming a quantum on the selected chain. Keep only native spends.
        candidates = {
            (block, quantum) for block, quantum in candidates
            if quantum in spent_indices and block_read(rpc, block)['verboseData']['isChainBlock'] is True
        }
        require(len(candidates) == len({quantum for _, quantum in candidates}),
                'multiple selected receipt blocks report the same consumed quantum')
        return min(candidates, key=lambda candidate: (candidate[1], candidate[0])) if candidates else None
    require(len(candidates) == 1, 'expected one receipt candidate for this claim in the producer log')
    return candidates.pop()


def validate_receipt_observation(rpc, output, observed, expected, receipt, validator, logged_quantum=None):
    validator_hash = hashlib.sha256(validator.read_bytes()).hexdigest()
    report_path = output.with_name(output.name + '.receipt.json')
    subprocess.run([str(validator), str(output), str(report_path)], check=True, timeout=60)
    report = json.loads(report_path.read_text())
    require(report.get('version') == 1 and report.get('kind') == 'protectedReceiptObservationAudit'
            and all(report.get(field) is True for field in
                    ('headerHashChecked', 'nativeReceiptEnvelopeValidated', 'spentIndexObserved')),
            'native receipt validator did not confirm the signed header and spent index')
    require(report['claim'] == expected['claim'] and report['block'] == receipt,
            'native receipt audit names another claim or block')
    if logged_quantum is not None:
        require(report['quantumIndex'] == logged_quantum, 'receipt quantum differs from producer log')
    require(hashlib.sha256(validator.read_bytes()).hexdigest() == validator_hash,
            'native receipt validator changed during execution')
    # A valid historical signature is not a fresh selected-chain observation.
    after = context(rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus'))
    if list(after) != observed['context']:
        raise ContextMoved('native generation changed during receipt authentication; retain files as historical only')
    with output.with_name(output.name + '.validation.json').open('x') as f:
        json.dump(dict(validator=str(validator), validatorSha256=validator_hash,
            observationSha256=hashlib.sha256(output.read_bytes()).hexdigest(),
            receiptAuditSha256=hashlib.sha256(report_path.read_bytes()).hexdigest(),
            contextRechecked=True, actualClaimCompletion=False), f, indent=2)
        f.write('\n')
    return report

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, required=True)
    inputs = parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument('--expected', type=Path)
    inputs.add_argument('--studio-evidence', type=Path,
                        help='JSON containing studioRun, studioJob, rail and chain records')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--previous', type=Path)
    approval_input = parser.add_mutually_exclusive_group()
    approval_input.add_argument('--panel-approval', help='full-replay carrier txid from the owned Panel log')
    approval_input.add_argument('--panel-log', type=Path, help='owned Panel log with exact original and approval txid')
    receipt_input = parser.add_mutually_exclusive_group()
    receipt_input.add_argument('--receipt')
    receipt_input.add_argument('--producer-log', type=Path,
                               help='owned producer log containing one PFS3 submission for the claim')
    parser.add_argument('--receipt-validator', type=Path,
                        help='absolute path to the built native protected-receipt-observe binary; required with --receipt')
    parser.add_argument('--maximum-chain-blocks', type=int, default=12_000)
    args = parser.parse_args()
    if (args.receipt or args.producer_log) and (not args.receipt_validator or not args.receipt_validator.is_absolute()):
        parser.error('receipt observation requires an absolute --receipt-validator')
    if args.studio_evidence:
        evidence = json.loads(args.studio_evidence.read_text())
        expected = expectation_from_studio(evidence['studioRun'], evidence['studioJob'], evidence['rail'], evidence['chain'])
    else:
        expected = json.loads(args.expected.read_text())
    previous = json.loads(args.previous.read_text()) if args.previous else None
    receipt, logged_quantum = (
        receipt_from_producer_log(args.producer_log, expected['claim'])
        if args.producer_log else (args.receipt, None)
    )
    rpc = WsRpc(host='127.0.0.1', port=args.port, timeout=30)
    try:
        observed = observe(rpc, expected, previous=previous, receipt=receipt, panel_approval=args.panel_approval, panel_log=args.panel_log,
                           maximum_chain_blocks=args.maximum_chain_blocks)
        with args.output.open('x') as f:
            json.dump(observed, f, indent=2)
            f.write('\n')
        if receipt:
            validate_receipt_observation(rpc, args.output, observed, expected, receipt,
                                         args.receipt_validator, logged_quantum)
    finally:
        rpc.close()


if __name__ == '__main__':
    main()
