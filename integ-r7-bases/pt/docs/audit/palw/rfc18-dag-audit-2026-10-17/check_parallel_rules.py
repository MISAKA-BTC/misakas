"""Independent Python checks of selected rules in the supplied PALW source.

These are NOT Rust tests, not a kaspad/node run, and not a concurrency benchmark.
Run: python3 check_parallel_rules.py [repo_root]
"""
from __future__ import annotations
import ast
import hashlib
import itertools
import json
import sys
from pathlib import Path

ROOT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('/mnt/data/dag_audit_src/misakas-pre')
DOMAIN = b'misaka-palw/rfc0018/sibling-order/v1'

def h64word(n: int) -> bytes:
    return b'\x00' * 56 + n.to_bytes(8, 'little')

def priority(execution: bytes) -> bytes:
    return hashlib.blake2b(execution, digest_size=64, key=DOMAIN).digest()

def canonical(works: list[tuple[str | None, bytes, str]]) -> list[tuple[str | None, bytes, str]]:
    slots = [i for i, w in enumerate(works) if w[0] is not None]
    groups: dict[str, list[tuple[bytes, int]]] = {}
    for i in slots:
        c, e, _ = works[i]
        assert c is not None
        groups.setdefault(c, []).append((priority(e), i))
    classes = [sorted(g) for _, g in sorted(groups.items())]
    classes.sort(key=lambda g: g[0])
    order = []
    r = 0
    while len(order) < len(slots):
        for g in classes:
            if r < len(g):
                order.append(g[r][1])
        r += 1
    out = list(range(len(works)))
    for slot, idx in zip(slots, order):
        out[slot] = idx
    return [works[i] for i in out]

def main() -> None:
    src = ROOT / 'consensus/core/src/palw_rfc0018_v1.rs'
    text = src.read_text()
    assert DOMAIN.decode() in text
    assert 'pub fn palw_sibling_canonical_order_v1' in text
    works = [(c, h64word(i), f'{c}{j}') for i, (c, j) in enumerate(itertools.product('ABC', range(2)), 1)]
    reference = canonical(works)
    count = 0
    for perm in itertools.permutations(works):
        out = canonical(list(perm))
        assert out == reference
        assert len({w[0] for w in out[:3]}) == 3
        count += 1
    mixed = [works[4], (None, h64word(98), 'receipt'), works[1], works[2],
             (None, h64word(99), 'nonattempt'), works[3], works[0], works[5]]
    m = canonical(mixed)
    assert m[1] == mixed[1] and m[4] == mixed[4]
    assert [w for w in m if w[0] is not None] == reference

    # Execute just the actual source's field helper, without loading the drill or making RPCs.
    driver = ROOT / 'scripts/drills/rfc0018_dag_drill.py'
    tree = ast.parse(driver.read_text())
    fn = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'field')
    env: dict = {}
    exec(compile(ast.Module(body=[fn], type_ignores=[]), str(driver), 'exec'), env)
    field = env['field']
    panels = {}
    identity_assertions = []
    for name, claim_id in [('a', '11' * 64), ('b', '22' * 64), ('c', '33' * 64)]:
        empty_rpc_row = {}  # hypothetical missing panel data, not a real node observation
        panels[name] = json.dumps(field(empty_rpc_row, 'seats', 'assignments', default=[]), sort_keys=True)
        identity_assertions.append(field(empty_rpc_row, 'claimId', default=claim_id) in (claim_id, '0x' + claim_id))
    assert all(identity_assertions)
    distinct = len(set(panels.values())) == len(panels)
    assert distinct is False

    # Only the arithmetic of resolved_through, not the full state-machine or retirement floor.
    unresolved_a = [100]
    b_final_score = 110
    cutoff_a_open = min(unresolved_a) - 1
    cutoff_a_resolved = (1 << 64) - 1
    assert b_final_score > cutoff_a_open
    assert b_final_score <= cutoff_a_resolved

    result = {
        'scope': 'Independent Python rule checks; no Rust compilation, node execution, or model inference.',
        'canonical_order': {'permutations_checked': count, 'unique_outputs': 1,
                            'one_per_class_in_first_round': True,
                            'nonattempt_slots_preserved': True,
                            'canonical_names': [w[2] for w in reference]},
        'drill_guard_hypothetical_empty_rpc': {'existing_identity_assertions_all_pass': all(identity_assertions),
                                             'reported_panelsDistinctByClaim': distinct,
                                             'interpretation': 'The existing checks alone do not establish nonempty bound panels; no real RPC was called.'},
        'resolved_prefix_arithmetic': {'oldest_unresolved_score': 100, 'resolved_through': cutoff_a_open,
                                       'other_certified_claim_score': b_final_score,
                                       'other_claim_outside_resolved_prefix': b_final_score > cutoff_a_open,
                                       'warning': 'Does not establish that other individual claim Final or payouts stop.'},
        'source_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                          for p in [src, driver, ROOT / 'consensus/core/src/palw_state_v2.rs']}
    }
    target = Path(__file__).with_name('rule_check_results.json')
    target.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(result, ensure_ascii=False, indent=2))

if __name__ == '__main__':
    main()
