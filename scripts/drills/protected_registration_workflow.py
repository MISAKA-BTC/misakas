"""Register the actual Studio class and tokenizer on an isolated protected chain.

Uses the normal signed CLI/RPC routes, then reads native TKR1 completion. No
accepted rows, slot rewrites, clock changes or model-review decisions are made
here. A private harness can supply a progress callback to coordinate its owned
nodes between reads; the default waits on the running chain. Commands are run
once, with durable intent/output logs. An uncertain send stops for readback,
never automatically rebroadcasts or starts a replacement registration.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

from misaka_wrpc_json import WsRpc
from protected_claim_observation import consensus_params_id, hash64, require, unsigned
from protected_inventory_acceptance import AcceptancePending, observe_transaction
from rfc12_acceptance_observe import ContextMoved, context


def write_new(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def chain_identity(rpc, expected):
    dag, node = rpc.call('getBlockDagInfo'), rpc.call('getPalwNodeStatus')
    stamp = context(dag, node)
    require(node['genesisHash'] == expected['genesisHash']
            and node['consensusParamsId'] == expected['consensusParamsId'], 'another chain/profile')
    require(node['pruningImportStable'] is True, 'import is not stable')
    hash64(dag['sink'])
    return dag, stamp


def tokenizer_status_reply(rpc, open_id):
    try:
        return rpc.call('getPalwTokenizerRegistrationStatus', {'openId': hash64(open_id)})
    except RuntimeError as error:
        message = str(error)
        if any(reason in message for reason in (
                'tokenizer selected view changed; retry the read', 'tokenizer parent unavailable: NoOpinion')):
            raise ContextMoved('native tokenizer selected view temporarily unavailable') from error
        raise


def registration_status(rpc, expected, open_id, open_hex, previous=None):
    """One generation, exact Open and immutable slots; absence is only pending."""
    dag, before = chain_identity(rpc, expected)
    reply = tokenizer_status_reply(rpc, open_id)
    _, after = chain_identity(rpc, expected)
    if before != after:
        raise ContextMoved('generation changed during tokenizer readback')
    require(reply['available'] is True, 'native tokenizer status unavailable')
    view = json.loads(reply['observationJson'])
    require(view['schema'] == 1 and view['openId'] == open_id, 'status names another Open')
    if view['acceptedParent']['block'] != dag['sink'] or unsigned(view['prospectiveDaa']) != dag['virtualDaaScore']:
        raise ContextMoved('tokenizer status is not at the observed native generation')
    hash64(view['acceptedParentRoot'])
    if reply['found'] is False:
        require(view['registration'] is None, 'absence carries a registration')
        require(previous is None, 'accepted registration disappeared')
        raise AcceptancePending('Open has no accepted native registration yet')
    require(reply['found'] is True, 'invalid found flag')
    row = view['registration']
    require(row['openId'] == open_id and row['openBodyHex'] == open_hex, 'accepted Open bytes changed')
    require(row['status'] in ('pending', 'complete'), 'registration expired or was superseded')
    hash64(row['policyId'])
    hash64(row['accepted']['block'])
    unsigned(row['accepted']['daa'])
    require(type(row['parts']) is list and len(row['parts']) == row['quote']['partCount'] > 0,
            'invalid indexed Part count')
    for index, part in enumerate(row['parts']):
        require(part['index'] == index and type(part['received']) is bool, 'invalid indexed Part')
        unsigned(part['reservedDaa'])
        require((part['accepted'] is not None) == part['received'], 'Part acceptance differs from its flag')
        if part['received']:
            hash64(part['accepted']['block'])
            require(unsigned(part['accepted']['daa']) == part['reservedDaa'], 'Part accepted outside its immutable slot')
    require(len(row['finishes']) == 2, 'missing immutable Finish slots')
    for finish in row['finishes']:
        unsigned(finish['reservedDaa'])
        require(type(finish['attempted']) is bool, 'invalid Finish attempt flag')
    if previous is not None:
        old = previous['registration']
        for key in ('accepted', 'openBodyHex', 'policyId', 'quote', 'chargeId'):
            require(row[key] == old[key], 'accepted registration origin/quote changed')
        require(len(row['parts']) == len(old['parts']), 'Part count changed')
        for part, prior in zip(row['parts'], old['parts']):
            for key in ('index', 'offset', 'bytes', 'reservedDaa'):
                require(part[key] == prior[key], 'immutable Part slot/extent changed')
            if prior['received']:
                require(part == prior, 'accepted Part changed or disappeared')
        for finish, prior in zip(row['finishes'], old['finishes']):
            require(finish['kind'] == prior['kind'] and finish['reservedDaa'] == prior['reservedDaa'],
                    'immutable Finish slot changed')
            require(not prior['attempted'] or finish['attempted'], 'accepted Finish attempt disappeared')
    if row['status'] == 'complete':
        require(all(p['received'] for p in row['parts']), 'completion lacks accepted Parts')
        hash64(row['registrationId'])
        hash64(row['terminal']['block'])
        terminal = unsigned(row['terminal']['daa'])
        require(any(f['attempted'] and f['reservedDaa'] == terminal for f in row['finishes']),
                'completion is outside its accepted Finish slots')
    return view


def completed_registration_on_peers(clients, expected, prior):
    """Read one completed registration at the same selected parent on each peer."""
    row = prior['registration']
    require(row['status'] == 'complete', 'registration is still pending')
    try:
        before = [chain_identity(client, expected)[1] for client in clients]
        if len({(stamp[0], stamp[2]) for stamp in before}) != 1:
            return None
        try:
            views = [registration_status(client, expected, prior['openId'], row['openBodyHex'], prior)
                     for client in clients]
        except (ValueError, AcceptancePending):
            if before != [chain_identity(client, expected)[1] for client in clients]:
                return None
            raise
        after = [chain_identity(client, expected)[1] for client in clients]
    except ContextMoved:
        return None
    if before != after or any(view['acceptedParent']['block'] != stamp[0]
                              or view['prospectiveDaa'] != stamp[2] for view, stamp in zip(views, before)):
        return None
    require(all(view['registration'] == row for view in views),
            'peers did not recover the completed tokenizer registration')
    require(len({view['acceptedParentRoot'] for view in views}) == 1,
            'peers disagree on tokenizer state root at the same selected parent')
    return views


def next_control(view):
    """Never derive a replacement slot. Return an existing slot or completion."""
    row, now = view['registration'], unsigned(view['prospectiveDaa'])
    if row['status'] == 'complete':
        return None
    pending = [part for part in row['parts'] if not part['received']]
    if pending:
        # The native scheduler orders equal-deadline packets by packet ID.
        # Part index identifies the bytes, independently of service order.
        part = min(pending, key=lambda row: (unsigned(row['reservedDaa']), row['index']))
        slot = unsigned(part['reservedDaa'])
        require(now <= slot, 'unaccepted Part missed its immutable slot')
        return f"part-{part['index']:04}.tkr1", slot
    for finish in row['finishes']:
        slot = unsigned(finish['reservedDaa'])
        if not finish['attempted'] and slot >= now:
            return 'finish.tkr1', slot
    raise ValueError('both immutable Finish opportunities were used or missed')


class RegistrationWorkflow:
    def __init__(self, config, output, rpc, progress=None):
        require(config['version'] == 1, 'unknown workflow version')
        bootstrap = json.loads(Path(config['bootstrap']).read_text())
        profile = bootstrap['profile']
        require(bootstrap['kind'] == 'protected-private-bootstrap-keys' and bootstrap['version'] == 3
                and bootstrap['releaseApproval'] is False, 'explicit protected private bootstrap required')
        require(profile['kind'] == 'rfc12-private-integration-probe' and profile['network'] == 'testnet-12'
                and profile['skipProofOfWork'] is False and profile['releaseApproval'] is False
                and profile['protectedClaimPrivatePolicyV3']['releaseApproval'] is False,
                'wrong private factory')
        require(re.fullmatch(r'[0-9a-f]{64}', config['salt']) is not None, 'explicit private genesis salt required')
        salt_id = hashlib.blake2b(bytes.fromhex(config['salt']), digest_size=8,
                                  key=b'misaka-palw-drill-salt-id/v1').hexdigest()
        require(salt_id == profile['saltId'], 'CLI salt differs from the exported private genesis')
        self.expected = dict(version=1, genesisHash=hash64(profile['genesis']),
                             consensusParamsId=consensus_params_id(profile['consensusParamsId']))
        self.chain = dict(self.expected, networkDomain=hash64(bootstrap['networkDomain']))
        self.activation_daa = unsigned(profile['protectedClaimPrivatePolicyV3']['activationDaa'])
        for port in ('borshPort', 'jsonPort'):
            require(0 < unsigned(config[port], 16), 'invalid loopback port')
        for field in ('maximumCarrierBatchFeeSompi', 'maximumInventoryPrepaidSompi', 'maximumTkrFeeSompi',
                      'maximumRegistrationChargeSompi', 'inventoryAcceptSpanDaa', 'tkrAcceptSpanDaa',
                      'tkrFinishSpanDaa', 'workflowTimeoutSeconds', 'commandTimeoutSeconds'):
            require(0 < unsigned(config[field]), f'{field} must be explicitly positive')
        require(config['commandTimeoutSeconds'] <= 300, 'command timeout exceeds 300 seconds')
        require(0 < unsigned(config['maximumChainBlocks']) <= 100_000, 'invalid scan bound')
        hash64(config['nonce'])
        for field in ('cli', 'palwClass', 'artifact', 'container', 'semantics', 'rawTokenizer', 'eogOpening', 'policy', 'keyFile'):
            require(Path(config[field]).is_absolute() and Path(config[field]).is_file(), f'{field} must be an absolute file')
        seats = bootstrap['seats']
        require(any(seat['bond']['bondOutpoint'] == config['bond']
                    and (Path(config['bootstrap']).parent / seat['bond']['seedFile']).resolve() == Path(config['keyFile']).resolve()
                    for seat in seats), 'registrant must be an exported private bond/key pair')
        self.config, self.rpc = config, rpc
        self.output = Path(output)
        require(self.output.is_absolute(), 'output must be an absolute fresh directory')
        self.output.mkdir(mode=0o700)
        write_new(self.output / 'input.json', config)
        self.deadline = time.monotonic() + config['workflowTimeoutSeconds']
        self.progress = progress or (lambda: time.sleep(0.1))
        self.prefix = [config['cli'], '--network', 'testnet-12', '--output', 'json',
                       '--rpc', f"127.0.0.1:{config['borshPort']}", '--palw-drill-genesis-salt', config['salt']]
        self.tokenizer = self.prefix + ['palw', 'tokenizer-registration']
        self.key = ['--key-file', config['keyFile']]
        self.sequence = 0

    def wait(self, read):
        while True:
            require(time.monotonic() < self.deadline, 'registration workflow deadline expired')
            try:
                result = read()
                if result is not None:
                    return result
            except ContextMoved:
                # Re-read a moving snapshot before advancing the clock. The
                # next immutable tokenizer control may be due at this DAA.
                time.sleep(0.1)
                continue
            except AcceptancePending:
                pass
            self.progress()

    def run(self, label, argv, json_result=True):
        require(time.monotonic() < self.deadline, 'registration workflow deadline expired')
        self.wait(lambda: chain_identity(self.rpc, self.expected))
        self.sequence += 1
        stem = self.output / f'{self.sequence:03}-{label}'
        write_new(stem.with_suffix('.intent.json'), dict(argv=argv, completed=False,
                   submissionMayBeUncertain=True, retryAutomatically=False))
        with stem.with_suffix('.stdout').open('xb') as out, stem.with_suffix('.stderr').open('xb') as err:
            result = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=out, stderr=err,
                                    timeout=min(self.config['commandTimeoutSeconds'], self.deadline - time.monotonic()))
        write_new(stem.with_suffix('.result.json'), dict(returncode=result.returncode))
        require(result.returncode == 0, f'{label} failed; inspect its retained output before any retry')
        return json.loads(stem.with_suffix('.stdout').read_text()) if json_result else None

    def acceptance(self, transaction, anchor):
        expected = dict(self.expected, transactionId=hash64(transaction), scanFromBlock=anchor)
        return self.wait(lambda: observe_transaction(self.rpc, expected,
                         maximum_chain_blocks=self.config['maximumChainBlocks']))

    def submit_objects(self, label, paths):
        dag, _ = self.wait(lambda: chain_identity(self.rpc, self.expected))
        anchor = dag['sink']
        write_new(self.output / (label + '-anchor.json'), dict(self.expected, scanFromBlock=anchor))
        result = self.run(label, self.prefix + ['palw', 'submit-object', *self.key,
                         '--maximum-total-fee-sompi', str(self.config['maximumCarrierBatchFeeSompi']),
                         '--submit', '--object', *map(str, paths)])
        require(result['ok'] is True and result['submitted'] is True, 'carriers were not submitted')
        carriers = result['carriers']
        require([c['object'] for c in carriers] == list(map(str, paths)), 'submitted object order/identity changed')
        require(len({c['txid'] for c in carriers}) == len(carriers), 'duplicate submitted transaction')
        accepted = [self.acceptance(c['txid'], anchor) for c in carriers]
        write_new(self.output / (label + '-acceptance.json'), accepted)
        return accepted

    def resume_inventory(self, previous):
        """Recheck the accepted setup and Open before using its signed inventory Parts."""
        def accepted(label):
            rows = json.loads((previous / f'{label}-acceptance.json').read_text())
            checked = []
            for row in rows:
                expected = row['expected']
                require(all(expected[name] == value for name, value in self.expected.items()),
                        'checkpoint acceptance belongs to another chain')
                current = observe_transaction(self.rpc, expected, maximum_chain_blocks=self.config['maximumChainBlocks'])
                require(all(current[key] == row[key] for key in ('acceptingBlock', 'acceptedDaa', 'acceptedTransactionIds')),
                        'checkpoint carrier acceptance changed')
                checked.append(current)
            return checked

        require(json.loads((previous / 'input.json').read_text()) == self.config, 'registration checkpoint configuration changed')
        for label in ('class-submit', 'setup-submit', 'inventory-open-submit'):
            rows = self.wait(lambda: accepted(label))
            write_new(self.output / f'{label}-acceptance.json', rows)
        prefix = next(previous.glob('*-inventory-parts-submit.stderr'))
        lines = [json.loads(line) for line in prefix.read_text().splitlines() if line.startswith('{')]
        intent, failure = lines
        require(intent['kind'] == 'palwCarrierBatchIntent'
                and 'artifact inventory part changes the bounded original stream offset' in failure['error']
                and '/part-0001.object.borsh' in failure['error'],
                'checkpoint is not the supported inventory batch failure')
        first = intent['carriers'][0]
        anchor = json.loads((previous / 'inventory-parts-submit-anchor.json').read_text())
        require(all(anchor[name] == value for name, value in self.expected.items()), 'batch anchor belongs to another chain')
        expected = dict(anchor, transactionId=hash64(first['txid']))
        # Restarted nodes have an empty mempool. An already accepted prefix
        # must still be recognized before constructing the next carrier.
        def read_first():
            try:
                return observe_transaction(self.rpc, expected, maximum_chain_blocks=self.config['maximumChainBlocks'])
            except AcceptancePending:
                return False
        return self.wait(read_first)

    def resume_tokenizer(self, previous):
        """Reuse completed Inventory after the previous TKR1 material has been collected."""
        require(json.loads((previous / 'input.json').read_text()) == self.config, 'tokenizer checkpoint configuration changed')
        def ancestor_with(relative_path):
            source = previous
            visited = {source.resolve()}
            while not (source / relative_path).is_file():
                pointer = source.parent / 'warmup-resume.json'
                require(pointer.is_file(), f'{relative_path} missing from tokenizer checkpoint history')
                source = Path(json.loads(pointer.read_text())['source']) / 'registration'
                require(source.resolve() not in visited, 'circular tokenizer inventory checkpoint')
                visited.add(source.resolve())
            return source

        original = ancestor_with('inventory-finish-submit-acceptance.json')
        inventory = ancestor_with('inventory-open/offer.borsh') / 'inventory-open'
        for label in ('setup-submit', 'inventory-finish-submit'):
            rows = json.loads((original / f'{label}-acceptance.json').read_text())
            for row in rows:
                expected = row['expected']
                require(all(expected[name] == value for name, value in self.expected.items()),
                        'checkpoint acceptance belongs to another chain')
                current = self.wait(lambda: observe_transaction(
                    self.rpc, expected, maximum_chain_blocks=self.config['maximumChainBlocks']))
                require(all(current[key] == row[key] for key in ('acceptingBlock', 'acceptedDaa', 'acceptedTransactionIds')),
                        'checkpoint carrier acceptance changed')
        setup_rows = json.loads((original / 'setup-submit-acceptance.json').read_text())
        draft, = previous.glob('*-tkr-draft.stdout')
        prepared = json.loads(draft.read_text())
        sent, = previous.glob('*-tkr-send-open.stdout')
        open_id = hash64(json.loads(sent.read_text())['openId'])
        require(prepared['openId'] == open_id, 'checkpoint draft differs from the submitted Open')
        material_until = unsigned(prepared['materialUntilDaa'])
        require(material_until > unsigned(prepared['finishUntilDaa']), 'invalid retained material deadline')

        def collected():
            dag, before = chain_identity(self.rpc, self.expected)
            if dag['virtualDaaScore'] < material_until:
                return None
            reply = tokenizer_status_reply(self.rpc, open_id)
            if chain_identity(self.rpc, self.expected)[1] != before:
                raise ContextMoved('old tokenizer session moved during collection readback')
            require(reply['available'] is True, 'native tokenizer status unavailable')
            view = json.loads(reply['observationJson'])
            require(view['openId'] == open_id, 'readback names another tokenizer Open')
            row = view['registration']
            if row is not None:
                require(row['status'] == 'expired', 'old tokenizer session was not expired')
                return None
            require(reply['found'] is False, 'removed tokenizer session is still marked found')
            return True

        self.wait(collected)
        nonce = hashlib.blake2b(bytes.fromhex(open_id), digest_size=64, person=b'misaka-tkr-retry').hexdigest()
        return setup_rows, inventory, nonce

    def execute(self, inventory_checkpoint=None, *, tkr_checkpoint=None, relay_control=None):
        c, out = self.config, self.output
        dag, _ = self.wait(lambda: chain_identity(self.rpc, self.expected))
        require(dag['virtualDaaScore'] >= self.activation_daa, 'private policy not active')
        nonce = c['nonce']
        if tkr_checkpoint is not None:
            setup_rows, inventory, nonce = self.resume_tokenizer(tkr_checkpoint)
        else:
            accepted_first = None
            if inventory_checkpoint is None:
                # Model registration is unreviewed. This existing CLI switch omits the
                # local runtime-pack gate; all native claim and signature checks remain.
                registration = out / 'class-registration.object'
                self.run('class-build', self.prefix + ['palw', 'tir-registration', *self.key,
                         '--artifact', c['artifact'], '--bond', c['bond'], '--skip-pack-verify', '--out', str(registration)])
                self.submit_objects('class-submit', [registration])
                setup = out / 'class-setup'
                self.run('class-setup', [c['palwClass'], 'tir-fp-setup', '--network', 'testnet-12', '--drill-salt', c['salt'],
                         '--artifact', c['artifact'], '--container', c['container'], '--semantics', c['semantics'],
                         *self.key, '--out', str(setup)], json_result=False)
                setup_rows = self.submit_objects('setup-submit', [setup / name for name in
                                  ('class-layout.object', 'class-semantics.object', 'container-binding.object')])
                dag, _ = self.wait(lambda: chain_identity(self.rpc, self.expected))
                inventory = out / 'inventory-open'
                self.run('inventory-open-build', self.tokenizer + ['inventory-open', *self.key, '--bond', c['bond'],
                         '--class-semantics', c['semantics'], '--raw-tokenizer', c['rawTokenizer'],
                         '--accept-until-daa', str(unsigned(dag['virtualDaaScore'] + c['inventoryAcceptSpanDaa'])),
                         '--max-prepaid-sompi', str(c['maximumInventoryPrepaidSompi']), '--output-dir', str(inventory)])
                opening = self.submit_objects('inventory-open-submit', [inventory / 'open.object.borsh'])[0]
            else:
                accepted_first = self.resume_inventory(inventory_checkpoint)
                setup_rows = json.loads((out / 'setup-submit-acceptance.json').read_text())
                opening, = json.loads((out / 'inventory-open-submit-acceptance.json').read_text())
                inventory = inventory_checkpoint / 'inventory-open'
            continuation = out / 'inventory-continuation'
            plan = self.run('inventory-continue-build', self.tokenizer + ['inventory-continue', *self.key,
                            '--offer', str(inventory / 'offer.borsh'), '--raw-tokenizer', c['rawTokenizer'],
                            '--accepted-open-daa', str(opening['acceptedDaa']), '--output-dir', str(continuation)])
            parts = [continuation / f'part-{index:04}.object.borsh' for index in range(unsigned(plan['parts']))]
            require(parts, 'EOF has no Parts')
            if accepted_first:
                write_new(out / 'inventory-part-0000-submit-acceptance.json', [accepted_first])
            for index, part in enumerate(parts):
                if index == 0 and accepted_first:
                    continue
                self.submit_objects(f'inventory-part-{index:04}-submit', [part])
            self.submit_objects('inventory-finish-submit', [continuation / 'finish.object.borsh'])
        pins = self.run('parent-pins', [c['palwClass'], 'tir-fp-pins', '--network', 'testnet-12', '--drill-salt', c['salt'],
                       '--artifact', c['artifact'], '--container', c['container'], '--semantics', c['semantics'],
                       '--semantics-bound-daa', str(setup_rows[1]['acceptedDaa']),
                       '--eof-offer', str(inventory / 'offer.borsh'), '--policy', c['policy'], '--out', str(out / 'pins.json')])
        require(pins['parentsAuthenticated'] is False, 'candidate pins unexpectedly claim native acceptance')
        dag, _ = self.wait(lambda: chain_identity(self.rpc, self.expected))
        now = unsigned(dag['virtualDaaScore'])
        spec = dict(pins['partialSpecification'], nonce=nonce, proposed_accepting_daa=now,
                    accept_until_daa=unsigned(now + c['tkrAcceptSpanDaa']),
                    finish_until_daa=unsigned(now + c['tkrFinishSpanDaa']),
                    max_registration_charge=c['maximumRegistrationChargeSompi'])
        write_new(out / 'open-specification.json', spec)
        draft, prepared = out / 'tkr-draft', out / 'tkr-prepared'
        draft_result = self.run('tkr-draft', self.tokenizer + ['draft-open', '--specification', str(out / 'open-specification.json'),
                         '--policy', c['policy'], '--raw-tokenizer', c['rawTokenizer'], '--eog-opening', c['eogOpening'],
                         '--output-dir', str(draft)])
        self.run('tkr-prepare', self.tokenizer + ['prepare', *self.key, '--open-body', str(draft / 'open-body.borsh'),
                 '--raw-tokenizer', c['rawTokenizer'], '--eog-opening', c['eogOpening'], '--output-dir', str(prepared)])
        open_id = hash64(draft_result['openId'])
        open_hex = (draft / 'open-body.borsh').read_bytes().hex()
        view = None
        def status():
            nonlocal view
            current = registration_status(self.rpc, self.expected, open_id, open_hex, view)
            view = current
            return current
        def send(name, slot=None):
            sent = self.run('tkr-send-' + name.removesuffix('.tkr1') + (f'-{slot}' if slot is not None else ''),
                            self.tokenizer + ['send', *self.key, '--payload', str(prepared / name),
                            '--quote', str(prepared / 'quote.json'), '--max-fee', str(c['maximumTkrFeeSompi']), '--submit'])
            require(sent['submitted'] is True and sent['openId'] == open_id and sent['reservedDaa'] == slot,
                    'sender returned another control or reservation')
            hash64(sent['txid'])
            if relay_control is not None:
                relay_control(sent['txid'], slot)
        send('open.tkr1')
        self.wait(status)
        while True:
            action = next_control(view)
            if action is None:
                break
            name, slot = action
            def at_slot():
                current = status()
                require(next_control(current) == action, 'selected control changed before submission')
                return current if current['prospectiveDaa'] == slot else None
            self.wait(at_slot)
            send(name, slot)
            def control_accepted():
                current = status()
                row = current['registration']
                if name.startswith('part-'):
                    received = row['parts'][int(name[5:9])]['received']
                else:
                    received = row['status'] == 'complete' or any(f['reservedDaa'] == slot and f['attempted'] for f in row['finishes'])
                # Never try the same transaction again if its slot moved before
                # native acceptance. The retained send output is the readback handle.
                require(received or current['prospectiveDaa'] <= slot, 'submitted control missed native acceptance at its slot')
                return current if received else None
            self.wait(control_accepted)
        result = dict(version=1, kind='protectedPrivateRegistration', expected=self.expected,
                      nativeTokenizerRegistered=True, registration=view, actualClaimCompletion=False,
                      chain=self.chain,
                      next='create a fresh Studio claim against this registered native state')
        write_new(out / 'result.json', result)
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    rpc = WsRpc(host='127.0.0.1', port=config['jsonPort'], timeout=30)
    try:
        workflow = RegistrationWorkflow(config, args.output, rpc)
        print(json.dumps(workflow.execute()))
    finally:
        rpc.close()


if __name__ == '__main__':
    main()
