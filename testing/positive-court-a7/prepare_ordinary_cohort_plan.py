#!/usr/bin/env python3
"""Metadata-only source preparation; NEVER reads/stat's a model payload or runs a binary.

Only this repository's archived JSON/source/prompt control files are read. The
result is a dependency/pin plan, not execution/registration/metadata-owner proof.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

SLUGS = ('smollm2-360m', 'smollm2-135m', 'pythia-70m')
ARMS = ('SmolLm2360M', 'SmolLm2135M', 'Pythia70M')

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--repository', type=Path, required=True)
    p.add_argument('--staged-harness-source', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    args = p.parse_args()
    root = args.repository.resolve()
    source = args.staged_harness_source.read_text()
    source_sha = hashlib.sha256(source.encode()).hexdigest()
    historical = root / 'docs/evidence/positive-court-a0-2026-10-31/integration/real-llm-run-8gib-20261101'
    manifest_record = json.loads((historical / 'manifest.json').read_text())
    historical_models = {v['slug']: v for v in manifest_record['models']}
    models = []
    for slug, arm in zip(SLUGS, ARMS):
        record = historical / slug / 'runner.json'
        old = json.loads(record.read_text())
        prompt_path = root / 'testing/positive-court-a7/real-llm-inputs' / (slug + '.json')
        prompt = json.loads(prompt_path.read_text())
        # Strict static arm parse, no Rust/model execution. All pin text must be
        # exact archived record data, not values read from candidate artifacts.
        body = re.search(r'Self::' + arm + r'\s*=>\s*FixedModelPinV2\s*\{(.*?)\n\s*\}', source, re.S)
        if not body:
            raise ValueError('source arm absent: ' + arm)
        def string(name):
            m = re.search(r'\b' + name + r':\s*"([^"]+)"', body.group(1))
            if not m:
                raise ValueError('source field absent: ' + name)
            return m.group(1)
        expected = {
            'model': old['model'],
            'class_id': old['class_id'],
            'artifact_root': old['inputs']['artifact_root'],
            'registered_tokenizer': old['inputs']['registered_tokenizer'],
            'original_context_hash': old['job']['context_hash'],
            'expected_artifact_sha256': old['artifact_sha256'],
        }
        expected['revision'] = historical_models[slug]['revision']
        def number(name):
            m = re.search(r'\b' + name + r':\s*(\d+)', body.group(1))
            if not m:
                raise ValueError('source numeric field absent: ' + name)
            return int(m.group(1))
        if number('original_max_context') != old['original_max_context'] or number('artifact_bytes') != old['inputs']['artifact_bytes']:
            raise ValueError('source numeric pin mismatch: ' + slug)
        if sha(prompt_path) != old['prompt_ids_sha256']:
            raise ValueError('prompt bytes changed from archived run: ' + slug)
        for key, value in expected.items():
            if string(key) != value:
                raise ValueError('source pin mismatch: ' + slug + ':' + key)
        field = 'PYTHIA_PROMPT' if slug == 'pythia-70m' else 'SMOL_PROMPT'
        raw = re.search(r'const ' + field + r'.*?=\s*&\[(.*?)\];', source, re.S)
        if not raw or [int(x) for x in raw.group(1).split(',') if x.strip()] != prompt:
            raise ValueError('literal prompt mismatch: ' + slug)
        models.append({
            'slug': slug, **expected, 'revision': string('revision'),
            'original_max_context': old['original_max_context'],
            'profile': old['resolved_profile'],
            'prefill': old['declared_prefill_tokens'], 'decode': old['exact_decode_tokens'],
            'prompt_ids': prompt, 'prompt_source': str(prompt_path), 'prompt_source_sha256': sha(prompt_path),
            'archived_record': str(record), 'archived_record_sha256': sha(record),
            'artifact_path_metadata_only': old['inputs']['artifact'],
            'artifact_payload_read': False,
        })
    result = {
        'status': 'SOURCE_PINS_MATCHED_NOT_EXECUTED',
        'runtime_readiness': 'WAITING_INDEPENDENT_BRIDGE_REVIEW_AND_COMPLETE_EXTERNAL_METADATA_OWNER',
        'harness_source': str(args.staged_harness_source), 'harness_sha256': source_sha,
        'scope': 'same-owned-parameters short-local ordinary CPU then legacy dense baseline',
        'native_authority': False, 'model_execution': False, 'cargo': False,
        'model_payload_read_or_stat': False, 'cold_claimed': False, 'rss_limit_claimed': False,
        'maximum_position_coverage_claimed': False, 'metadata_whole_owner_established_here': False,
        'dependencies': [
            'local forward r3 source060b574d93bfc062d3cd8c901f440e43c208316939e603f1350b1117d11b3b1a (UNRUN)',
            'separate same-crate dense measurement bridge, required unobserved runner entries (UNRUN)',
            'complete pre-guard external metadata owner/census: Header+Class+full Source+Space+Compile+controls/errors+independent aliases',
            'Root-controlled source/binary mapping, build/run slot, finite whole host quota and outer deadline/process cleanup policy',
        ],
        'models': models,
        'not_implemented_in_this_library': [
            'self-owned metadata bootstrap/CLI', 'full-file SHA/publisher authentication',
            'Panel one-pass record/material generation', 'Node fullrelation/Court/P/credit/settlement',
            'new native Assignment/Result/rooted role/funding authority',
        ],
    }
    args.out.write_text(json.dumps(result, indent=2) + '\n')

if __name__ == '__main__':
    main()
