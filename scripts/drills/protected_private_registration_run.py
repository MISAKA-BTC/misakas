"""Start owned loopback peers and run protected registration on their chain."""
import argparse
from contextlib import ExitStack
from functools import partial
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.request

from misaka_wrpc_json import WsRpc
from protected_claim_observation import (
    block_read, expectation_from_studio, hash64, observe, observe_claim_progress, receipt_from_producer_log,
    require, unsigned, validate_receipt_observation,
)
from protected_historical_clock import HEARTBEAT_INTERVAL_MS, ProtectedHistoricalClock
from protected_inventory_acceptance import AcceptancePending, observe_transaction
from protected_registration_workflow import ContextMoved, RegistrationWorkflow, chain_identity, registration_status, write_new


# The protected receipt spend horizon is at most DISPUTE_WINDOW_DAA in
# palw_court_policy_v1.rs, measured from the recorded original acceptance.
PROTECTED_HARD_WINDOW_DAA = 10_000
# This audit counts selected blocks, including heartbeats that spend no DAA.
# A claim can remain valid after the observer's ordinary 12,000-block bound.
PROTECTED_LIFECYCLE_SCAN_BLOCKS = 50_000


def wait_until(deadline, read):
    while time.monotonic() < deadline:
        result = read()
        if result is not None:
            return result
        time.sleep(0.1)
    raise TimeoutError('isolated peer readiness or DAA progression timed out')


def relay_registered_control(source, miner, txid, timeout, check_context):
    """Relay the exact signed transaction, including any unconfirmed change it spends."""
    until = time.monotonic() + timeout

    def entry(client, transaction_id):
        try:
            result = client.call('getMempoolEntry', dict(
                transactionId=transaction_id, includeOrphanPool=True, filterTransactionPool=False))['mempoolEntry']
        except RuntimeError as error:
            if 'not found' not in str(error):
                raise
            return None
        return result if not result['is_orphan'] else None

    relayed = set()

    def transfer(transaction_id, known, ancestors):
        if transaction_id in relayed or entry(miner, transaction_id) is not None:
            return
        require(len(ancestors) < 64 and transaction_id not in ancestors, 'invalid tokenizer funding ancestry')
        require(time.monotonic() < until, 'tokenizer relay timed out')
        tx = known['transaction']
        require(tx['verboseData']['transactionId'] == transaction_id, 'tokenizer relay transaction ID changed')
        for tx_input in tx['inputs']:
            parent_id = tx_input['previousOutpoint']['transactionId']
            pending = entry(source, parent_id)
            if pending is not None:
                transfer(parent_id, pending, ancestors | {transaction_id})
        try:
            reply = miner.call('submitTransaction', dict(transaction=tx, allowOrphan=False))
        except RuntimeError:
            if entry(miner, transaction_id) is None:
                raise
        else:
            require(reply['transactionId'] == transaction_id, 'tokenizer miner returned another transaction')
        relayed.add(transaction_id)

    while time.monotonic() < until:
        if check_context():
            if entry(miner, txid) is not None:
                return
            signed = entry(source, txid)
            if signed is not None:
                transfer(txid, signed, set())
                return
        time.sleep(0.1)
    raise TimeoutError('tokenizer control did not reach the heartbeat miner before the clock step')


def stop_child(child):
    if child.poll() is None:
        child.terminate()
    try:
        child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()


def start_provider(config, output, stack, timeout):
    executable = Path(config['executable'])
    port = unsigned(config['port'], 16)
    max_store = unsigned(config['maxStoreGiB'])
    require(executable.is_absolute() and executable.is_file() and port > 0 and 0 < max_store < 2**34,
            'invalid owned evidence provider configuration')
    url = f'http://127.0.0.1:{port}'
    with (output / 'provider.log').open('xb') as log:
        child = subprocess.Popen(
            [str(executable), 'serve', '--root', str(output / 'evidence'),
             '--listen', f'127.0.0.1:{port}', '--max-store-gb', str(max_store)],
            stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
        )
    stack.callback(stop_child, child)

    def ready():
        require(child.poll() is None, 'evidence provider exited before listening')
        with (output / 'provider.log').open() as log:
            for line in log:
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if event.get('event') == 'serving':
                    require(event.get('url') == url, 'evidence provider listened at another URL')
                    return url
        return None

    return wait_until(time.monotonic() + timeout, ready)


def registration_peer(config, bootstrap):
    """Use the peer that advertises the registrant's locally reserved Panel float."""
    registration = config['registration']
    seats = bootstrap['seats']
    owners = [index for index, peer in enumerate(config['peers']) if 'seat' in peer
              and any(seats[seat]['bond']['bondOutpoint'] == registration['bond']
                      for seat in [peer['seat'], *peer.get('extraSeats', [])])]
    require(len(owners) == 1, 'registrant must have exactly one owned seat peer')
    return owners[0]


def independent_funding(rpc, bond, minimum, *, require_reservation=True, transaction=None):
    """Verify this signer can fund carriers while its Panel float stays reserved."""
    facts = rpc.call('getPalwProducerFacts', dict(classId='', bondTransactionId='', bondIndex=0, withBond=False))
    locked = set(facts['lockedBondOutpoints'])
    float_outpoint = bond['feeFloatOutpoint']
    if require_reservation:
        require(float_outpoint in locked, 'signer RPC does not expose its reserved Panel float')
    locked.update((float_outpoint, bond['bondOutpoint']))
    entries = rpc.call('getUtxosByAddresses', dict(addresses=[bond['address']]))['entries']
    for entry in entries:
        outpoint, utxo = entry['outpoint'], entry['utxoEntry']
        if transaction is not None and outpoint['transactionId'] != transaction:
            continue
        name = f"{outpoint['transactionId']}:{outpoint['index']}"
        if name not in locked and not utxo['isCoinbase'] and unsigned(utxo['amount']) > minimum:
            return dict(outpoint=name, amountSompi=utxo['amount'])


def require_independent_funding(rpc, bond, minimum, *, require_reservation=True):
    funding = independent_funding(rpc, bond, minimum, require_reservation=require_reservation)
    if funding is not None:
        return funding
    raise ValueError('signer needs a separate confirmed carrier funding output')


def fund_private_carriers(output, registration, bootstrap, expected, peers, clients, recipients, step):
    """Fund fresh isolated signers through the wallet and confirm on every peer."""
    salt = bytes.fromhex(registration['salt'])
    require(len(salt) == 32 and any(salt), 'private carrier funding needs an explicit 32-byte genesis salt')
    salt_id = hashlib.blake2b(salt, digest_size=8, key=b'misaka-palw-drill-salt-id/v1').hexdigest()
    require(salt_id == bootstrap['profile']['saltId'], 'private carrier funding salt differs from the bootstrap')
    main = bootstrap['main']
    key = Path(registration['bootstrap']).parent / main['seedFile']
    cli = Path(registration['cli'])
    require(key.is_absolute() and key.is_file() and cli.is_absolute() and cli.is_file(),
            'private carrier funding needs the exported main key and wallet CLI')
    for label, owner, bond, minimum in recipients:
        existing = independent_funding(clients[owner], bond, minimum, require_reservation=False)
        if existing is not None:
            continue
        amount = unsigned(max(10_000_000_000, minimum + 1))
        whole, fraction = divmod(amount, 100_000_000)
        dag, _ = chain_identity(clients[owner], expected)
        origin = dict(expected, version=1, scanFromBlock=dag['sink'])
        directory = output / ('fund-' + label)
        directory.mkdir(mode=0o700)
        write_new(directory / 'intent.json',
                  dict(origin, recipient=bond['address'], amountSompi=amount))
        command = [str(cli), '--network', 'testnet-12', '--output', 'json',
                   '--rpc', f"127.0.0.1:{peers[owner]['borsh']}",
                   '--palw-drill-genesis-salt', registration['salt'], 'wallet', 'send',
                   '--key-file', str(key), '--to', bond['address'],
                   '--amount', f'{whole}.{fraction:08d}', '--yes']
        with (directory / 'wallet.stdout').open('xb') as out, (directory / 'wallet.stderr').open('xb') as err:
            subprocess.run(command, stdin=subprocess.DEVNULL, stdout=out, stderr=err, check=True,
                           timeout=registration['commandTimeoutSeconds'])
        result = json.loads((directory / 'wallet.stdout').read_text())
        require(result['ok'] is True and result['dryRun'] is False and result['amountSompi'] == amount
                and result['from'] == main['address'] and result['to'] == bond['address'],
                'wallet response differs from private carrier funding intent')
        txid = hash64(result['txid'])
        observation = dict(origin, transactionId=txid)
        while True:
            try:
                accepted, funds = [], []
                for rpc in clients:
                    row = observe_transaction(rpc, observation)
                    fund = independent_funding(rpc, bond, minimum, require_reservation=False, transaction=txid)
                    _, after = chain_identity(rpc, expected)
                    if row['context'] != list(after):
                        raise ContextMoved('peer moved during private carrier funding readback')
                    accepted.append(row)
                    funds.append(fund)
                if (all(row['acceptingBlock'] == accepted[0]['acceptingBlock'] for row in accepted)
                        and funds[0] is not None and all(fund == funds[0] for fund in funds)
                        and funds[0]['amountSompi'] == amount):
                    write_new(directory / 'acceptance.json', dict(accepted=accepted, outputs=funds))
                    break
            except (AcceptancePending, ContextMoved):
                pass
            step()


def studio_peer_plan(config, bootstrap):
    """Resolve the executor and replay inputs before starting the chain."""
    studio = config['studio']
    producer = unsigned(studio['producerPeer'])
    peers = config['peers']
    require(producer < len(peers) and 'seat' in peers[producer],
            'receipt producer must be an owned bonded peer')
    bond = bootstrap['seats'][unsigned(peers[producer]['seat'])]['bond']
    key = (Path(config['registration']['bootstrap']).parent / bond['seedFile']).resolve()
    settings = json.loads(Path(studio['settings']).read_text())['node']
    prompt = settings['free_prompt']
    require(prompt['bond'] == bond['bondOutpoint'] and Path(settings['producer_key_path']).resolve() == key,
            'Studio executor must be the owned receipt producer')
    artifact = Path(prompt['artifact'])
    require(artifact.is_absolute() and artifact.is_file(), 'Panel requires the exact Studio class artifact')
    require(any('seat' in peer for index, peer in enumerate(peers) if index != producer),
            'the run needs an owned Panel peer distinct from the Studio producer')
    return producer, bond['bondOutpoint'], artifact


def warmup_checkpoint(config, previous, bootstrap, *, inventory=False, tokenizer=False, studio=False):
    """Resume only native warmup from the original owned databases."""
    require(previous.is_absolute() and previous.is_dir(), 'warmup run must be an absolute existing directory')
    require(json.loads((previous / 'run.json').read_text()) == config,
            'warmup configuration differs from the original run')
    if studio:
        require((previous / 'registration-result.json').is_file() and not (previous / 'result.json').exists(),
                'Studio resume requires a completed registration and an unfinished run')
        studio_dir = previous / 'studio'
        require(not any((studio_dir / name).exists() for name in ('studio-run.json', 'studio-evidence.json', 'original.json'))
                and not any(path.is_file() or path.is_symlink() for path in (studio_dir / 'fp-outbox').rglob('*')),
                'the stopped Studio run may have submitted a claim')
    else:
        require(not any((previous / name).exists() for name in ('registration-result.json', 'studio', 'result.json')),
                'checkpoint already passed inventory registration')
        if tokenizer:
            require(any((previous / 'registration').glob('*-tkr-send-open.stdout')),
                    'tokenizer resume requires a submitted Open')
        elif inventory:
            require((previous / 'registration/inventory-open-submit-acceptance.json').is_file(),
                    'inventory resume requires an accepted Open')
        else:
            require(not (previous / 'registration').exists(), 'warmup resume requires a run that never began registration')
    profile = bootstrap['profile']
    source = previous
    visited = {source.resolve()}
    while (source / 'warmup-resume.json').is_file():
        source = Path(json.loads((source / 'warmup-resume.json').read_text())['source'])
        require(source.is_absolute() and source.is_dir() and source.resolve() not in visited,
                'warmup database source is invalid or circular')
        require(json.loads((source / 'run.json').read_text()) == config,
                'warmup database source differs from the original run')
        visited.add(source.resolve())
    paths = [source / f'peer-{index}' for index in range(len(config['peers']))]
    for path in paths:
        marker = path / 'rfc12-private-probe-v1.json'
        require(not path.is_symlink() and json.loads(marker.read_text()) == profile,
                'warmup peer database belongs to another private policy')
    clock = json.loads((previous / 'clock' / 'clock.json').read_text())
    require(clock['version'] == 1 and clock['genesis'] == profile['genesis'],
            'warmup clock names another chain')
    timestamp = unsigned(clock['timestampMs'])
    require(timestamp >= config['initialTimestampMs'], 'warmup clock predates its configured start')
    return paths, timestamp


def run(config, output, after_registration=None, resume_warmup=None, *, resume_inventory=None,
        resume_tokenizer=None, tokenizer_clock_checkpoint=None, isolated_node=None, resume_studio=None):
    require(sum(path is not None for path in (resume_warmup, resume_inventory, resume_tokenizer, resume_studio)) <= 1, 'choose one checkpoint')
    require(tokenizer_clock_checkpoint is None or resume_tokenizer is not None,
            'a separate clock checkpoint requires a tokenizer resume')
    checkpoint = resume_studio or resume_tokenizer or resume_inventory or resume_warmup
    registration = config['registration']
    bootstrap = json.loads(Path(registration['bootstrap']).read_text())
    profile = bootstrap['profile']
    require(bootstrap['kind'] == 'protected-private-bootstrap-keys'
            and bootstrap['releaseApproval'] is False and profile['releaseApproval'] is False
            and profile['skipProofOfWork'] is False, 'expected an isolated protected bootstrap')
    expected = dict(genesisHash=profile['genesis'], consensusParamsId=profile['consensusParamsId'])
    activation = unsigned(profile['protectedClaimPrivatePolicyV3']['activationDaa'])
    peers = config['peers']
    require(2 <= len(peers) <= 8, 'two to eight owned peers are required')
    ports = [peer[field] for peer in peers for field in ('p2p', 'grpc', 'borsh', 'json')]
    if 'provider' in config:
        ports.append(config['provider']['port'])
    if 'studio' in config:
        ports.append(config['studio']['port'])
        require('provider' in config, 'Studio lifecycle requires the owned provider')
    require(len(set(ports)) == len(ports) and all(unsigned(port, 16) > 0 for port in ports),
            'every loopback listener needs a distinct port')
    require(output.is_absolute(), 'an absolute fresh output directory is required')
    maximum_steps = unsigned(config['maximumClockSteps'])
    fund_from_genesis = config.get('fundCarriersFromPrivateGenesis', False)
    require(type(fund_from_genesis) is bool, 'fundCarriersFromPrivateGenesis must be a boolean')
    step_timeout = unsigned(config['stepTimeoutSeconds'])
    startup_timeout = unsigned(config.get('startupTimeoutSeconds', step_timeout))
    settle_seconds = unsigned(config['stepSettleSeconds'])
    owned_seats = []
    for peer in peers:
        extras = peer.get('extraSeats', [])
        require(type(extras) is list and (not extras or 'seat' in peer), 'extra seats require a primary seat')
        owned_seats.extend(unsigned(seat, 32) for seat in ([peer['seat']] if 'seat' in peer else []) + extras)
    require(len(set(owned_seats)) == len(owned_seats) and all(seat < len(bootstrap['seats']) for seat in owned_seats),
            'every seat must be exported and run on only one owned peer')
    registrant = registration_peer(config, bootstrap)
    registration_bond = next(seat['bond'] for seat in bootstrap['seats']
                             if seat['bond']['bondOutpoint'] == registration['bond'])
    registration = dict(registration, borshPort=peers[registrant]['borsh'], jsonPort=peers[registrant]['json'])
    producer, executor_bond, artifact = studio_peer_plan(config, bootstrap) if 'studio' in config else (None, None, None)
    require(maximum_steps > 0 and 0 < settle_seconds <= step_timeout <= 300 and startup_timeout > 0,
            'explicit clock limits are required')
    if checkpoint is None:
        appdirs = [output / f'peer-{index}' for index in range(len(peers))]
        initial_timestamp = config['initialTimestampMs']
    else:
        appdirs, initial_timestamp = warmup_checkpoint(config, checkpoint, bootstrap, inventory=resume_inventory is not None, tokenizer=resume_tokenizer is not None, studio=resume_studio is not None)
    if tokenizer_clock_checkpoint is not None:
        clock_resume = json.loads((tokenizer_clock_checkpoint / 'warmup-resume.json').read_text())
        require(clock_resume['source'] == str(resume_tokenizer),
                'clock checkpoint does not resume the same tokenizer run')
        require(json.loads((tokenizer_clock_checkpoint / 'run.json').read_text()) == config,
                'clock checkpoint configuration differs from the tokenizer run')
        require(not any((tokenizer_clock_checkpoint / name).exists() for name in
                        ('registration-result.json', 'studio', 'result.json')),
                'clock checkpoint already passed tokenizer registration')
        require(not any((tokenizer_clock_checkpoint / 'registration').glob('*-tkr-send-*.intent.json')),
                'clock checkpoint may have submitted a different tokenizer control')
        clock = json.loads((tokenizer_clock_checkpoint / 'clock/clock.json').read_text())
        require(clock['version'] == 1 and clock['genesis'] == profile['genesis']
                and unsigned(clock['timestampMs']) >= initial_timestamp,
                'clock checkpoint does not advance the same private chain')
        initial_timestamp = clock['timestampMs']
    output.mkdir(mode=0o700)
    write_new(output / 'run.json', config)
    if checkpoint is not None:
        write_new(output / 'warmup-resume.json', dict(source=str(checkpoint), timestampMs=initial_timestamp))
    with ExitStack() as stack:
        provider = start_provider(config['provider'], output, stack, step_timeout) if 'provider' in config else None
        clock = stack.enter_context(ProtectedHistoricalClock(
            output / 'clock', profile['genesis'], bootstrap['genesisTimestampMs'], initial_timestamp))
        permit_path = clock.directory / 'heartbeat-permit.json' if isolated_node is not None else None
        node = isolated_node or config['node']
        for index, peer in enumerate(peers):
            args = [
                '--private-probe-salt=' + registration['salt'],
                '--private-probe-protected-policy=' + bootstrap['policyFile'],
                '--testnet', '--netsuffix=12', '--nodnsseed', '--outpeers=0', '--disable-upnp',
                '--unsaferpc', '--utxoindex', '--appdir=' + str(appdirs[index]),
                f"--listen=127.0.0.1:{peer['p2p']}", f"--rpclisten=127.0.0.1:{peer['grpc']}",
                f"--rpclisten-borsh=127.0.0.1:{peer['borsh']}", f"--rpclisten-json=127.0.0.1:{peer['json']}",
            ]
            if index == 0:
                args.append('--palw-heartbeat-miner-address=' + bootstrap['workers']['heartbeat']['address'])
            if 'seat' in peer:
                seat_index = unsigned(peer['seat'], 32)
                bond = bootstrap['seats'][seat_index]['bond']
                key = Path(registration['bootstrap']).parent / bond['seedFile']
                require(key.is_absolute() and key.is_file(), 'exported seat key unavailable')
                args.extend([
                    '--palw-producer-key=' + str(key),
                    '--palw-producer-bond=' + bond['bondOutpoint'],
                    '--palw-fee-outpoint=' + bond['feeFloatOutpoint'],
                ])
                if provider:
                    args.append('--palw-evidence-provider-dir=' + provider)
                if artifact is not None:
                    args.append('--palw-class-artifact=' + str(artifact))
                for extra in peer.get('extraSeats', []):
                    other = bootstrap['seats'][extra]['bond']
                    other_key = Path(registration['bootstrap']).parent / other['seedFile']
                    require(other_key.is_file() and ',' not in str(other_key), 'invalid extra seat key path')
                    args.append(f"--palw-drill-extra-seat=key={other_key},bond={other['bondOutpoint']},"
                                f"fee={other['feeFloatOutpoint']},pay={other['address']}")
            if index == producer:
                args.append('--palw-produce')
            clock.start_peer(f'peer-{index}', node, args,
                             heartbeat_permit=permit_path if index == 0 else None)
        clock.seal()

        def connect(peer):
            clock._alive()
            try:
                return WsRpc(host='127.0.0.1', port=peer['json'], timeout=5)
            except OSError:
                return None

        clients = []
        startup_deadline = time.monotonic() + startup_timeout
        for peer in peers:
            client = wait_until(startup_deadline, lambda: connect(peer))
            stack.callback(client.close)
            clients.append(client)
            chain_identity(client, expected)
        for client in clients[1:]:
            client.call('addPeer', {'peerAddress': {'ip': '127.0.0.1', 'port': peers[0]['p2p']}, 'isPermanent': True})

        wait_until(time.monotonic() + step_timeout, lambda:
                   True if all(rpc.call('getConnectedPeerInfo')['peerInfo'] for rpc in clients) else None)

        def agreement():
            clock._alive()
            try:
                views = [chain_identity(rpc, expected)[0] for rpc in clients]
            except ContextMoved:
                return None
            if len({(view['sink'], view['virtualDaaScore']) for view in views}) == 1:
                return views[0]
            return None

        steps = 0
        pending_control = None

        def step():
            nonlocal steps, pending_control
            require(steps < maximum_steps, 'isolated clock step limit reached')
            before = wait_until(time.monotonic() + step_timeout, agreement)
            if permit_path is not None:
                if pending_control is not None:
                    txid, slot = pending_control
                    require(before['virtualDaaScore'] == slot, 'reserved DAA moved before heartbeat permit')
                else:
                    txid = None
                with tempfile.NamedTemporaryFile(mode='w', dir=clock.directory, delete=False) as stream:
                    temporary = Path(stream.name)
                    json.dump(dict(sink=before['sink'], daa=before['virtualDaaScore'], transactionId=txid), stream)
                os.replace(temporary, permit_path)
            acknowledgement = clock.advance_to(clock.current + HEARTBEAT_INTERVAL_MS)
            steps += 1
            after = before
            deadline = time.monotonic() + settle_seconds
            while time.monotonic() < deadline:
                candidate = agreement()
                if candidate is not None:
                    after = candidate
                    if after['sink'] != before['sink']:
                        break
                time.sleep(0.1)
            write_new(output / f'clock-{steps:06}.json', dict(acknowledgement, before=before, after=after))
            if permit_path is not None and pending_control is not None and before['sink'] != after['sink']:
                txid, slot = pending_control
                source = block_read(clients[0], after['sink'], transactions=True)
                require(source['header']['daaScore'] == slot and any(
                    tx.get('verboseData', {}).get('transactionId') == txid for tx in source['transactions']),
                    'the selected source did not carry the signed tokenizer control at its slot')
                write_new(output / f'tkr-source-{slot}.json', dict(
                    txid=txid, reservedDaa=slot, sourceBlock=after['sink']))
                pending_control = None
            return after

        # Registration supplies the normal signed carriers. The owned clock
        # advances only between reads, so it cannot race a control send.
        view = wait_until(time.monotonic() + step_timeout, agreement)
        registration_budget = sum(unsigned(registration[field]) for field in (
            'maximumCarrierBatchFeeSompi', 'maximumInventoryPrepaidSompi',
            'maximumTkrFeeSompi', 'maximumRegistrationChargeSompi'))
        recipients = [('registration', registrant, registration_bond, registration_budget)]
        if after_registration is not None:
            studio_bond = bootstrap['seats'][peers[producer]['seat']]['bond']
            recipients.append(('studio', producer, studio_bond, 0))
        if fund_from_genesis and checkpoint is None:
            fund_private_carriers(output, registration, bootstrap, expected, peers, clients, recipients, step)
            view = wait_until(time.monotonic() + step_timeout, agreement)
        funding = {label: require_independent_funding(clients[owner], bond, minimum, require_reservation=False)
                   for label, owner, bond, minimum in recipients}
        write_new(output / 'carrier-funding.json', funding)
        while view['virtualDaaScore'] < activation:
            view = step()
        require_independent_funding(clients[registrant], registration_bond, registration_budget)
        if after_registration is not None:
            require_independent_funding(clients[producer], studio_bond, 0)
        if resume_studio is not None:
            require(after_registration is not None, 'Studio resume requires its lifecycle callback')
            result = json.loads((resume_studio / 'registration-result.json').read_text())
            prior = result['registration']
            row = prior['registration']
            require(result['expected'] == dict(version=1, **expected) and result['nativeTokenizerRegistered'] is True
                    and row['status'] == 'complete', 'checkpoint belongs to another chain or registration')
            def read_registration():
                try:
                    views = [registration_status(client, expected, prior['openId'], row['openBodyHex'], prior)
                             for client in clients]
                except ContextMoved:
                    return None
                require(all(view['registration'] == row for view in views),
                        'peers did not recover the completed tokenizer registration')
                return views
            readback = wait_until(time.monotonic() + step_timeout, read_registration)
            write_new(output / 'tokenizer-registration-three-peer-readback.json', readback)
        else:
            workflow = RegistrationWorkflow(registration, output / 'registration', clients[registrant], progress=step)
            def relay_control(txid, slot):
                """Place the exact signed control at the heartbeat miner."""
                nonlocal pending_control
                if permit_path is not None and slot is not None:
                    require(pending_control is None, 'another tokenizer control awaits its source block')
                    pending_control = (txid, slot)
                source, miner = clients[registrant], clients[0]
                if source is miner:
                    return
                def check_context():
                    tip = agreement()
                    if tip is None:
                        return False
                    if slot is not None:
                        require(tip['virtualDaaScore'] == slot, 'tokenizer slot moved before relay')
                    return True

                relay_registered_control(source, miner, txid, step_timeout, check_context)

            result = workflow.execute(resume_inventory / 'registration' if resume_inventory else None,
                                      tkr_checkpoint=resume_tokenizer / 'registration' if resume_tokenizer else None,
                                      relay_control=relay_control)
        write_new(output / 'registration-result.json', result)
        if after_registration is not None:
            # The next stage must use these very peers and this clock owner.
            result = after_registration(result, clients, step, clock.directory, executor_bond)
        write_new(output / 'result.json', result)
        return result


def receipt_window_elapsed(original, detail):
    accepted_daa = unsigned(original['origin']['acceptedDaa'])
    return unsigned(detail['tipDaa']) > accepted_daa + PROTECTED_HARD_WINDOW_DAA


def require_gate_c_status(detail):
    """Require this protected claim to use the independent certification lane."""
    require(detail['gateCGoverns'] is True, 'protected Studio claim is outside Gate C')
    status = detail['claim']['securityCertification']
    if status == 'pending':
        return False
    require(status == 'certified', f'protected Studio claim Gate C ended with {status}')
    return True


def receipt_peer_view(rpc, expected, producer, receipt, quantum):
    """Read the same selected receipt and spent quantum from a separate node."""
    dag, stamp = chain_identity(rpc, expected)
    producer_sink, _, producer_daa, *_ = producer['context']
    if (dag['sink'], dag['virtualDaaScore']) != (producer_sink, producer_daa):
        raise ContextMoved('waiting for receipt peer to reach the producer sink')
    approval = producer['panelApprovalAcceptance']
    observed = observe(rpc, expected, previous=producer, receipt=receipt,
                       panel_approval=approval['carrierTransaction'],
                       maximum_chain_blocks=PROTECTED_LIFECYCLE_SCAN_BLOCKS)
    if list(stamp) != observed['context']:
        raise ContextMoved('receipt peer moved during validation')
    claim = observed['detail']['claim']
    quanta = observed['detail']['receiptQuanta']
    spent = quanta['spentIndices']
    require(claim['phase'] == 'final' and observed['panelApprovalAcceptance'] == approval,
            'receipt peer did not confirm the same Final and Panel approval')
    require(require_gate_c_status(observed['detail']), 'receipt peer did not confirm Gate C certification')
    require(observed['receiptBlock']['header'] == producer['receiptBlock']['header'],
            'receipt peers returned different block headers')
    require(quanta == producer['detail']['receiptQuanta'] and quanta['spentIndicesComplete'] is True
            and quanta['spentCount'] == claim['quantaSpent'] == len(set(spent)) == len(spent)
            and quantum in spent, 'receipt peer did not confirm the same unique consumed quantum')
    return observed


def studio_lifecycle(config, output, registration, clients, step, logs, executor_bond):
    """Run one fresh Studio request and retain native observations as it lands."""
    studio = config['studio']
    port = unsigned(studio['port'], 16)
    require(port > 0, 'Studio needs an explicit loopback port')
    executable, settings = Path(studio['executable']), Path(studio['settings'])
    models, validator = Path(studio['models']), Path(studio['receiptValidator'])
    for path in (executable, settings, models, validator):
        require(path.is_absolute() and path.exists(), 'Studio paths must be absolute and present')
    settings_value = json.loads(settings.read_text())
    node = settings_value['node']
    fp = node['free_prompt']
    reg = config['registration']
    require(fp['evidence_provider'] == f"http://127.0.0.1:{config['provider']['port']}"
            and fp['private_protected']['genesis_salt'] == reg['salt'],
            'Studio settings differ from the owned chain or evidence provider')
    data = output / 'studio'
    data.mkdir(mode=0o700)
    # The producer's own RPC exposes its reserved Panel UTXOs to the rail.
    producer = unsigned(studio['producerPeer'])
    peer = config['peers'][producer]
    rpc = clients[producer]
    node['misaka_rpc'] = f"ws://127.0.0.1:{peer['borsh']}"
    fp['rpc_json'] = f"ws://127.0.0.1:{peer['json']}"
    settings = data / 'settings.json'
    write_new(settings, settings_value)
    timeout = unsigned(studio['timeoutSeconds'])
    deadline = time.monotonic() + timeout
    base = f'http://127.0.0.1:{port}/api/v1'
    http = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def request(path, payload=None):
        body = None if payload is None else json.dumps(payload).encode()
        req = urllib.request.Request(base + path, data=body,
                                     headers={'Content-Type': 'application/json'})
        try:
            with http.open(req, timeout=min(600, max(1, deadline - time.monotonic()))) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            message = error.read(4096).decode('utf-8', errors='replace')
            write_new(data / 'http-error.json', dict(path=path, status=error.code, body=message))
            raise ValueError(f'Studio {path} returned HTTP {error.code}: {message}') from error

    with (data / 'studio.log').open('xb') as log:
        child = subprocess.Popen(
            [str(executable), '--host', '127.0.0.1', '--port', str(port),
             '--settings', str(settings), '--models-dir', str(models), '--data-dir', str(data)],
            stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
            env={k: v for k, v in os.environ.items() if not k.startswith('MISAKA_STUDIO_')},
        )
        started = False
        try:
            def ready():
                require(child.poll() is None, 'Studio exited before serving its API')
                try:
                    return request('/network/prompt-stack')
                except urllib.error.URLError:
                    return None

            wait_until(deadline, ready)
            discovered = request('/models/refresh', {})
            artifact = Path(fp['artifact']).resolve()
            selected = [model for model in discovered if Path(model['path']).resolve() == artifact]
            require(len(selected) == 1, 'Studio did not discover the exact configured model')
            write_new(data / 'studio-model.json', selected[0])
            stack = request('/network/prompt-stack/start', {})
            started = True
            write_new(data / 'stack.json', stack)
            outbox = Path(stack['outbox'])
            require(outbox.is_absolute() and outbox.is_relative_to(data), 'Studio outbox is outside this run')
            prompt = {'prompt': studio['prompt'], 'max_tokens': unsigned(studio['maxTokens'], 32)}
            write_new(data / 'request.json', prompt)
            def gateway_ready():
                status = request('/network/prompt-mining')
                if status.get('health'):
                    return status
                current = request('/network/prompt-stack')
                if not current['gateway']['running']:
                    write_new(data / 'gateway-startup-failed.json', current)
                    raise ValueError('Studio gateway exited before its health endpoint became ready')

            gateway = wait_until(min(deadline, time.monotonic() + config['startupTimeoutSeconds']), gateway_ready)
            write_new(data / 'gateway-health.json', gateway)
            run_result = request('/network/prompt-mining/run', prompt)
            write_new(data / 'studio-run.json', run_result)
            job_id = hash64(run_result['fp_job_id'])
            stem = outbox / ('fp-job-' + job_id[:16])
            rail_path = stem.with_suffix('.rail.json')
            failed_summary = stem.with_suffix('.json.submit-failed')
            expired_commitment = stem.with_suffix('.commitment-unsigned.borsh.expired')

            def until(stage, read, *, admitted=False):
                stage_deadline = time.monotonic() + timeout if admitted else deadline
                highest_daa = None
                retry_moved_snapshot = True
                while time.monotonic() < stage_deadline:
                    require(child.poll() is None, 'Studio exited during ' + stage)
                    try:
                        value = read()
                    except ContextMoved:
                        # Allow an in-flight block or earlier context sample to
                        # settle before opening another heartbeat slot.
                        if retry_moved_snapshot:
                            retry_moved_snapshot = False
                            time.sleep(0.1)
                            continue
                        value = None
                    if value is not None:
                        return value
                    daa = step()['virtualDaaScore']
                    retry_moved_snapshot = True
                    if admitted and (highest_daa is None or daa > highest_daa):
                        highest_daa = daa
                        stage_deadline = time.monotonic() + timeout
                raise TimeoutError(stage)

            def submission():
                # The rail removes terminal failures from the status endpoint's
                # input queue. Keep the local marker as the authoritative reason
                # to stop stepping this job.
                require(not failed_summary.exists(), f'rail stopped submitting Studio job {job_id}')
                require(not expired_commitment.exists(), f'Studio job {job_id} expired before submission')
                job = request('/network/prompt-mining/jobs/' + job_id)
                require(job.get('status') not in ('answered', 'misattributed', 'voided'),
                        f"Studio job {job_id} cannot produce a claim: {job.get('note')}")
                if not job.get('submission_txid') or not rail_path.is_file():
                    return None
                rail = json.loads(rail_path.read_text())
                expected = expectation_from_studio(run_result, job, rail, registration['chain'])
                write_new(data / 'studio-evidence.json',
                          dict(studioRun=run_result, studioJob=job, rail=rail, chain=registration['chain']))
                return expected

            expected = until('protected rail submission', submission)
            observe_lifecycle = partial(observe, rpc, expected, maximum_chain_blocks=PROTECTED_LIFECYCLE_SCAN_BLOCKS)

            def admission():
                response = rpc.call('getPalwClaimDetail', {'claimHex': expected['claim']})
                require(response['available'] is True, 'native claim details unavailable')
                return observe_lifecycle() if response['found'] else None

            original = until('protected original admission', admission)
            write_new(data / 'original.json', original)
            producer_log = logs / f'peer-{producer}.log'
            panel_logs = [logs / f'peer-{index}.log' for index, peer in enumerate(config['peers'])
                          if index != producer and 'seat' in peer]

            def progress_view(stage):
                detail = observe_claim_progress(rpc, expected, original)
                if stage != 'receipt' and receipt_window_elapsed(original, detail):
                    write_new(data / 'claim-window-elapsed.json', dict(
                        stage=stage, expected=expected, origin=original['origin'], detail=detail))
                    raise ValueError(f'protected claim hard deadline passed while waiting for {stage}')
                row = detail['claim']
                require(row['phase'] != 'voided', f"Studio claim voided: {row.get('voidReason')}")
                require(detail['protectedOriginal']['publicRefutationRecorded'] is False,
                        'public refutation recorded for Studio claim')
                return detail

            def panel_approved():
                detail = progress_view('Panel approval')
                if not detail['protectedOriginal']['fullReplayApprovalRecorded']:
                    return None
                return observe_lifecycle(previous=original, panel_logs=panel_logs, excluded_panel_bond=executor_bond)

            approved = until('same-claim included Panel approval', panel_approved, admitted=True)
            write_new(data / 'panel-approval.json', approved)
            approval = approved['panelApprovalAcceptance']['carrierTransaction']

            def finalized():
                if progress_view('Final')['claim']['phase'] != 'final':
                    return None
                current = observe_lifecycle(previous=original, panel_approval=approval)
                if current['detail']['claim']['phase'] != 'final':
                    raise ContextMoved('Final changed during full readback')
                return current

            final = until('same-claim Final with included Panel approval', finalized, admitted=True)
            write_new(data / 'final.json', final)

            def certified():
                detail = progress_view('Gate C certification')
                if not require_gate_c_status(detail):
                    return None
                current = observe_lifecycle(previous=original, panel_approval=approval)
                if current['detail']['claim']['phase'] != 'final' or not require_gate_c_status(current['detail']):
                    raise ContextMoved('Gate C certification changed during full readback')
                return current

            certification = until('same-claim Gate C certification', certified, admitted=True)
            write_new(data / 'gate-c-certification.json', certification)

            receipt_reads = 0

            def receipt_read():
                nonlocal receipt_reads
                current = progress_view('receipt')
                if current['receiptQuanta']['spentCount'] == 0:
                    if receipt_window_elapsed(original, current):
                        write_new(data / 'receipt-window-elapsed.json', dict(
                            expected=expected, origin=original['origin'], detail=current))
                        raise ValueError('protected receipt hard deadline passed with no consumed quantum for this claim')
                    return None
                candidate = receipt_from_producer_log(
                    producer_log, expected['claim'], rpc=rpc,
                    spent_indices=current['receiptQuanta']['spentIndices'])
                if candidate is None:
                    if receipt_window_elapsed(original, current):
                        write_new(data / 'receipt-readback-unresolved.json', dict(
                            expected=expected, origin=original['origin'], detail=current))
                        raise ValueError('protected receipt spend is recorded but its selected block could not be matched')
                    return None
                receipt, quantum = candidate
                observed = observe_lifecycle(previous=original, receipt=receipt, panel_approval=approval)
                if not require_gate_c_status(observed['detail']):
                    raise ContextMoved('Gate C certification changed before receipt readback')
                receipt_reads += 1
                record = data / f'receipt-observation-{receipt_reads:04d}.json'
                write_new(record, observed)
                audit = validate_receipt_observation(rpc, record, observed, expected, receipt, validator, quantum)
                peers = {str(index): receipt_peer_view(client, expected, observed, receipt, quantum)
                         for index, client in enumerate(clients) if index != producer}
                for index, client in enumerate(clients):
                    _, stamp = chain_identity(client, expected)
                    prior = observed if index == producer else peers[str(index)]
                    if list(stamp) != prior['context']:
                        raise ContextMoved('peer advanced during the final receipt audit')
                peer_record = record.with_suffix('.peers.json')
                write_new(peer_record, peers)
                return dict(version=1, registration=registration, expected=expected, original=observed['origin'],
                            phase=observed['detail']['claim']['phase'], receiptAudit=audit,
                            receiptObservation=record.name, receiptPeerObservations=peer_record.name,
                            receiptReadbackPeers=[producer, *(int(index) for index in peers)],
                            securityCertification=observed['detail']['claim']['securityCertification'],
                            panelApproval=observed['panelApprovalAcceptance'])

            return until('same-claim receipt block and validated spent quantum', receipt_read, admitted=True)
        finally:
            if started and child.poll() is None:
                try:
                    write_new(data / 'stack-final.json', request('/network/prompt-stack'))
                    request('/network/prompt-stack/stop', {})
                except (OSError, ValueError):
                    pass
            stop_child(child)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--resume-warmup', type=Path,
                        help='stopped run directory whose registration stage never began')
    parser.add_argument('--resume-inventory', type=Path, help='stopped run with an accepted inventory Open and a refused Part batch')
    parser.add_argument('--resume-tokenizer', type=Path, help='stopped run with completed Inventory and an incomplete TKR1 session')
    parser.add_argument('--tokenizer-clock-checkpoint', type=Path,
                        help='stopped pre-submission retry of the same tokenizer run with a later clock')
    parser.add_argument('--isolated-node', type=Path,
                        help='private node executable with operator-controlled heartbeat templates')
    parser.add_argument('--resume-studio', type=Path, help='stopped run with registration complete and no Studio claim sent')
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    continuation = None
    if 'studio' in config:
        def continuation(registration, clients, step, logs, executor_bond):
            return studio_lifecycle(config, args.output, registration, clients, step, logs, executor_bond)
    print(json.dumps(run(config, args.output, continuation, args.resume_warmup, resume_inventory=args.resume_inventory,
                         resume_tokenizer=args.resume_tokenizer,
                         tokenizer_clock_checkpoint=args.tokenizer_clock_checkpoint, isolated_node=args.isolated_node,
                         resume_studio=args.resume_studio)))


if __name__ == '__main__':
    main()
