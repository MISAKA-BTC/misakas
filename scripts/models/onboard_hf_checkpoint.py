#!/usr/bin/env python3
"""Run the pinned HF -> preflight -> runtime pack -> optional private registration path.

Every stage retains its actual argv, logs and exit code. Registration, conformance,
and public-network activation remain distinct outcomes; no user key is loaded.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time
import urllib.request
import fetch_hf_checkpoint as hf


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as f:
        while block := f.read(1 << 20):
            h.update(block)
    return h.hexdigest()


def declared_context(config: dict) -> int:
    values = [config[k] for k in ('max_position_embeddings', 'n_positions', 'max_seq_len') if k in config]
    if not values or any(type(x) is not int or x <= 0 for x in values) or len(set(values)) != 1:
        raise ValueError('missing, invalid or conflicting source context; no context is inferred')
    return values[0]


def native_result_matches(pack: dict, report: dict) -> None:
    declarations = pack['declared']
    if len(declarations) != 1:
        raise ValueError('expected one declared class')
    expected = declarations[0]
    if (report.get('schema') != 'misaka.hf-registration-local-acceptance.v1'
            or report.get('class_id') != expected['class_id']
            or report.get('artifact_root') != pack['result']['inventory_root']
            or report.get('max_context') != expected['max_context']
            or report.get('public_network') is not False
            or not all(report.get(k) is True for k in ('mempool_accepted', 'registration_mined',
                                                       'persisted_registry_read', 'independent_node_replay'))):
        raise ValueError('native acceptance did not bind the declared class/root/context or complete every required stage')


def failure_diagnostic(stderr: str) -> dict | None:
    """Classify a representation failure without inferring model training/quality."""
    if 'LOGITS_I64_Q24_V2' in stderr:
        profile, dtype = 'LOGITS_I64_Q24_V2', 'i64'
    elif 'LOGITS_Q24_V1' in stderr:
        profile, dtype = 'LOGITS_Q24_V1', 'i32'
    else:
        return None
    match = re.search(r'calibrated logits reach ([0-9]+(?:\.[0-9]+)?)', stderr)
    return {'category': 'OUTPUT_REPRESENTATION_RANGE', 'profile': profile,
            'dtype': dtype, 'fraction_bits': 24,
            'observed_absmax': float(match.group(1)) if match else None,
            'training_status_inferred': False,
            'resolution': 'requires a separately supported output representation; do not waive the bound'}


class Run:
    def __init__(self, root: Path, timeout: int):
        root.mkdir(parents=True, exist_ok=False)
        self.root, self.timeout = root, timeout
        self.state = {'schema': 'misaka.hf-onboarding-run.v1', 'status': 'RUNNING', 'stages': [],
                      'registration': 'NOT_RUN', 'public_network': False, 'claim_certified': False}
        self.save()

    def save(self):
        temporary = self.root/'run.json.tmp'
        temporary.write_text(json.dumps(self.state, indent=2)+'\n')
        temporary.replace(self.root/'run.json')

    def stage(self, name: str, argv: list[str], env=None):
        entry = {'name': name, 'argv': argv, 'status': 'RUNNING'}
        self.state['stages'].append(entry)
        self.save()
        started = time.monotonic()
        print(name, flush=True)
        try:
            with (self.root/(name+'.stdout')).open('wb') as out, (self.root/(name+'.stderr')).open('wb') as err:
                process = subprocess.Popen(argv, stdout=out, stderr=err, env=env, start_new_session=True)
                try:
                    code = process.wait(timeout=self.timeout)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
                    raise ValueError(f'{name}: stage exceeded timeout')
            entry['exit_code'] = code
            if code:
                with (self.root/(name+'.stderr')).open('rb') as log:
                    diagnostic = failure_diagnostic(log.read(1 << 20).decode('utf-8', errors='replace'))
                if diagnostic is not None:
                    entry['diagnostic'] = diagnostic
                raise ValueError(f'{name}: exit {code}; see retained stage logs')
            entry['status'] = 'PASS'
        except Exception:
            entry['status'] = 'FAILED'
            raise
        finally:
            entry['elapsed_seconds'] = round(time.monotonic()-started, 3)
            self.save()


def execute(args) -> dict:
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', args.repo) or not re.fullmatch(r'[0-9a-f]{40}', args.revision):
        raise ValueError('expected owner/repository and an immutable full commit SHA')
    if any(p in {'.', '..'} for p in args.repo.split('/')):
        raise ValueError('invalid repository segment')
    if bool(args.fixture_exporter) != bool(args.native_test_bin):
        raise ValueError('private acceptance requires both exporter and test binary')
    tools = [args.palw_class] + ([args.fixture_exporter, args.native_test_bin] if args.fixture_exporter else [])
    for tool in tools:
        if not Path(tool).is_file() or not os.access(tool, os.X_OK):
            raise ValueError(f'executable not found: {tool}')
    if not args.calib.is_file():
        raise ValueError('calibration token file not found')
    run = Run(args.out.resolve(), args.stage_timeout)
    run.state.update(repository=args.repo, revision=args.revision,
                     tools=[{'path': str(Path(t).resolve()), 'sha256': digest(Path(t))} for t in tools],
                     scripts=[{'path': str(Path(t).resolve()), 'sha256': digest(Path(t))}
                              for t in (__file__, hf.__file__)])
    try:
        url = f'https://huggingface.co/{args.repo}/resolve/{args.revision}/config.json'
        with urllib.request.urlopen(url, timeout=60) as response:
            raw = response.read(hf.MAX_METADATA+1)
        if len(raw) > hf.MAX_METADATA:
            raise ValueError('source config exceeds ceiling')
        context = declared_context(json.loads(raw))
        run.state['source_context'] = context
        (run.root/'source-config.json').write_bytes(raw)
        shutil.copy2(args.calib, run.root/'calibration.json')
        run.state['calibration_sha256'] = digest(run.root/'calibration.json')
        run.stage('preflight', [args.palw_class, 'preflight', '--network', args.network,
            f'hf://{args.repo}@{args.revision}', '--max-context', str(context), '--json'])
        # The preflight above reads config and tensor headers, before any complete weights.
        run.stage('fetch-source', [sys.executable, str(Path(hf.__file__).resolve()),
            '--repo', args.repo, '--revision', args.revision, '--out', str(run.root/'checkpoint'),
            '--max-bytes', str(args.max_bytes)])
        if (run.root/'checkpoint/config.json').read_bytes() != raw:
            raise ValueError('source config changed between preflight and verified download')
        run.state['source_manifest_sha256'] = digest(run.root/'checkpoint/source-manifest.json')
        run.save()
        artifact, pack = run.root/'model.palwtir', run.root/'pack'
        declare_network = 'private-wide-q24' if args.logits_bits == 64 else args.network
        run.stage('pack-build', [args.palw_class, 'pack', 'build', '--model', str(run.root/'checkpoint'),
            '--out', str(artifact), '--pack', str(pack), '--calib', str(run.root/'calibration.json'),
            '--repo', args.repo, '--revision', args.revision, '--context', str(context),
            '--max-window', str(context), '--logits-bits', str(args.logits_bits), '--declare', f'{declare_network}:max-context={context}',
            '--prompts', '1', '--prefill', '4', '--decode', '2'] + (['--stream'] if args.stream else []) + (['--hf-reference',str(args.hf_reference)] if args.hf_reference else []))
        manifest = json.loads((pack/'pack.json').read_text())
        if ({x['name'] for x in manifest['executor']['implementations']} != {'reference', 'typed-backend', 'independent'}
                or not manifest['conformance']['vectors'] or manifest['declared'][0]['max_context'] != context):
            raise ValueError('incomplete conformance implementations/vectors or changed source scope')
        declared = Path(str(artifact)+f'.{declare_network}.palwtir')
        run.state['declared_artifact'] = str(declared)
        run.state['artifact_sha256'] = digest(declared)
        run.state['class_id'] = manifest['declared'][0]['class_id']
        run.state['pack_sha256'] = digest(pack/'pack.json')
        if args.fixture_exporter:
            fixture = run.root/'registration-fixture.json'
            run.stage('export-fixture', [args.fixture_exporter, str(declared), str(fixture)])
            env = {**os.environ, 'MISAKA_HF_REGISTRATION_FIXTURE': str(fixture),
                   'MISAKA_HF_REGISTRATION_REPORT': str(run.root/'native.json')}
            # The unoptimized full consensus harness has a large async test frame.
            # Same policy as scripts/release/run_arming_evidence.py, only for this test subprocess.
            env.setdefault('RUST_MIN_STACK', str(64 << 20))
            if args.logits_bits == 64: env['MISAKA_PRIVATE_WIDE_Q24'] = '1'
            run.state['native_test_resources'] = {'RUST_MIN_STACK': env['RUST_MIN_STACK'],
                'private_wide_q24': args.logits_bits == 64, 'consensus_limits_changed': False}
            run.save()
            run.stage('native-register', [args.native_test_bin,
                'g14_downloaded_hf_artifact_registration_mined_and_replayed', '--ignored', '--nocapture'], env)
            result = json.loads((run.root/'native.json').read_text())
            native_result_matches(manifest, result)
            if result['artifact_sha256'] != run.state['artifact_sha256']:
                raise ValueError('native report names different artifact bytes')
            run.state['registration'] = 'PASS_PRIVATE_NATIVE_AND_REPLAY'
        run.state['status'] = 'PASS' if args.fixture_exporter else 'PREPARED_NOT_REGISTERED'
    except Exception as error:
        run.state.update(status='FAILED', error=str(error))
        raise
    finally:
        run.save()
    return run.state


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo', required=True)
    p.add_argument('--revision', required=True)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--calib', required=True, type=Path)
    p.add_argument('--palw-class', required=True)
    p.add_argument('--fixture-exporter')
    p.add_argument('--native-test-bin')
    p.add_argument('--hf-reference', type=Path)
    p.add_argument('--logits-bits', type=int, choices=[32,64], default=32)
    p.add_argument('--network', choices=['testnet-12'], default='testnet-12')
    p.add_argument('--stream', action='store_true', help='read conformance tensors on demand on all three executors')
    p.add_argument('--max-bytes', type=int, default=2 << 30)
    p.add_argument('--stage-timeout', type=int, default=1800)
    args = p.parse_args()
    for field in ('palw_class', 'fixture_exporter', 'native_test_bin'):
        if getattr(args, field):
            setattr(args, field, str(Path(getattr(args, field)).resolve()))
    try:
        print(json.dumps(execute(args), indent=2))
    except (ValueError, OSError) as error:
        p.exit(1, f'onboarding failed: {error}\n')


if __name__ == '__main__':
    main()
