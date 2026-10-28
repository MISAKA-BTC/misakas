#!/usr/bin/env python3
"""Reproduce static inventory and conditional arithmetic used by the RFC audit.

This does not compile or test the node and is not a network benchmark.
Usage: python reproduce_checks.py --repo /path/to/misakas-pre --out checks.json
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
from pathlib import Path


def ceil_div(a: int, b: int) -> int:
    if a < 0 or b <= 0:
        raise ValueError('a must be nonnegative and b positive')
    return (a + b - 1) // b


def history_path(context: int, tile: int, arity: int) -> list[int]:
    if context <= 0 or tile <= 0 or arity < 2:
        raise ValueError('positive context/tile and arity >= 2 required')
    path = [ceil_div(context, tile)]
    while path[-1] > 1:
        path.append(ceil_div(path[-1], arity))
    return path


def analyze(repo: Path) -> dict:
    if not (repo / 'docs/rfc').is_dir():
        raise ValueError('repository must contain docs/rfc')
    paths = sorted(p for p in (repo / 'docs/rfc').glob('*.md')
                   if re.match(r'^\d{4}-', p.name))
    inventory = []
    for p in paths:
        data = p.read_bytes()
        text = data.decode('utf-8')
        inventory.append({'path': str(p.relative_to(repo)),
                          'lines': len(text.splitlines()), 'bytes': len(data),
                          'sha256': hashlib.sha256(data).hexdigest()})
    signature_source = repo / 'consensus/core/src/mldsa87_primitives.rs'
    sig_text = signature_source.read_text(encoding='utf-8')
    match = re.search(r'MLDSA87_SIGNATURE_LEN\s*:\s*usize\s*=\s*([\d_]+)', sig_text)
    if not match:
        raise ValueError('MLDSA87_SIGNATURE_LEN constant not found')
    sig_size = int(match.group(1).replace('_', ''))
    arithmetic = []
    for k in (4, 16):
        for context in (32768, 2097152):
            chain = history_path(context, 16, k)
            rounds = len(chain) - 1
            signature_count = 1 + 2 * rounds
            arithmetic.append({'context': context, 'tile': 16, 'arity': k,
                'tile_count_path': chain, 'history_round_trips': rounds,
                'signed_root_and_history_moves_only': signature_count,
                'signature_only_bytes': signature_count * sig_size,
                'signature_only_KiB': signature_count * sig_size / 1024,
                'SC64_candidate_bytes': 65536,
                'SC64_remaining_after_signatures': 65536 - signature_count * sig_size,
                'condition': 'all history rounds occur; existing per-move ML-DSA-87 signatures; no batching or new codec; excludes filing, body, final proof and outer framing'})
    burst = []
    for n, s, b in [(10000,65536,25000),(10000,32768,12500),(1000,65536,25000)]:
        burst.append({'courts': n, 'assumed_total_bytes_per_court': s,
             'assumed_court_bandwidth_B_per_DAA': b,
             'wire_only_DAA_lower_bound': ceil_div(n*s,b),
             'note': 'hypothetical lower bound; no waiting, CPU, control, reorg or recovery'})
    module_name='palw_court_capacity_v1.rs'
    code_paths=[str(p.relative_to(repo)) for p in repo.rglob(module_name)]
    mentions=[]
    for p in paths:
        for n,line in enumerate(p.read_text(encoding='utf-8').splitlines(),1):
            if 'palw_court_capacity_v1' in line:
                mentions.append({'path':str(p.relative_to(repo)),'line':n,'text':line})
    missing_index={}
    for index in ('INDEX.md','TOPICS.md','README.md'):
        text=(repo/'docs/rfc'/index).read_text(encoding='utf-8')
        missing_index[index]=[p.name for p in paths if p.name not in text]
    evidence={}
    for rel,needle in [
        ('consensus/core/tests/review_heldnode_arity.rs', 'assert_eq!(held_aware, Ok(4)'),
        ('consensus/core/src/palw_court_v2.rs', 'PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT'),
        ('docs/spec/palw/04b-tensor-ir.md', 'The executor signs the root claim and every round')]:
        text=(repo/rel).read_text(encoding='utf-8')
        evidence[rel]={'needle':needle,'found':needle in text}
    # Deliberately simple countermodel: authenticated values need not obey a sum.
    parent=100; children=[40,50]
    committed_hashes=[hashlib.sha256(str(x).encode('ascii')).hexdigest() for x in children]
    reopened_hashes=[hashlib.sha256(str(x).encode('ascii')).hexdigest() for x in children]
    fold_example={'claimed_parent':parent, 'correct_children':children,
        'all_child_hashes_match_committed_values':committed_hashes==reopened_hashes,
        'arithmetic_fold_valid':sum(children)==parent,
        'scope':'toy counterexample to hash-membership implying sum correctness; not an attack on the implemented fold checker'}
    assertions={
        'numbered_RFC_count_25':len(paths)==25,
        '32K_k4_rounds_6':len(history_path(32768,16,4))-1==6,
        '2M_k4_rounds_9':len(history_path(2097152,16,4))-1==9,
        '32K_k16_rounds_3':len(history_path(32768,16,16))-1==3,
        '2M_k16_rounds_5':len(history_path(2097152,16,16))-1==5,
        'signature_bytes_4627':sig_size==4627,
        '2M_k4_signature_floor_87913':19*sig_size==87913,
        '2M_k4_floor_exceeds_64KiB':19*sig_size>65536,
        'remaining_window_6880':10000-3120==6880,
        'burst_example_26215':ceil_div(10000*65536,25000)==26215,
        'hash_does_not_imply_fold':fold_example['all_child_hashes_match_committed_values'] and not fold_example['arithmetic_fold_valid'],
        'expected_signature_and_arity_source_markers':all(x['found'] for x in evidence.values())}
    return {'scope':'static snapshot inspection and conditional arithmetic only; no Rust test, benchmark or node execution',
        'repo_path_at_run':str(repo), 'numbered_rfc_inventory':inventory,
        'numbered_rfc_count':len(paths),'numbered_rfc_lines':sum(x['lines'] for x in inventory),
        'numbered_rfc_bytes':sum(x['bytes'] for x in inventory),
        'signature_size':sig_size, 'rounds_and_signature_lower_bounds':arithmetic,
        'conditional_bandwidth_lower_bounds':burst,'remaining_window_DAA':6880,
        'capacity_module_paths_in_this_snapshot':code_paths,
        'capacity_module_RFC_mentions':mentions,'not_linked_from_indexes':missing_index,
        'source_markers':evidence,'toy_fold_countermodel':fold_example,
        'arithmetic_and_source_marker_checks':assertions,
        'all_arithmetic_and_source_marker_checks_pass':all(assertions.values())}


def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args=parser.parse_args()
    try:
        result=analyze(args.repo.resolve())
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    except (OSError,ValueError,UnicodeError) as exc:
        parser.exit(2,f'error: {exc}\n')
    print(json.dumps({'checks':len(result['arithmetic_and_source_marker_checks']),
                     'passed':sum(result['arithmetic_and_source_marker_checks'].values()),
                     'numbered_rfcs':result['numbered_rfc_count'],
                     'capacity_module_paths':result['capacity_module_paths_in_this_snapshot'],
                     'out':str(args.out)},ensure_ascii=False))
    if not result['all_arithmetic_and_source_marker_checks_pass']:
        raise SystemExit(1)

if __name__=='__main__':
    main()
