#!/usr/bin/env python3
"""Read-only segmented native history coverage; never certifies EVM compatibility.

Supports the original /tmp census NDJSON and closed .ndjson.gz segments.
sha256/observed_bytes refer to original decompressed bytes; storage_* records the physical file. No network calls or source edits.
Usage: python3 scripts/drills/misaka_public_history_analyze.py segment1.ndjson ...
"""
import argparse
import collections
import hashlib
import gzip
import zlib
import json
from pathlib import Path

IDENTITY = ('genesisHash', 'consensusParamsId', 'consensusScheduleId',
            'importSessionEpoch', 'importSessionPhase')


class PrefixReader:
    """Only expose the immutable observed physical prefix to gzip."""
    def __init__(self, stream, size, digest=None):
        self.stream, self.remaining, self.digest = stream, size, digest

    def read(self, count=-1):
        if not self.remaining:
            return b''
        count = min(self.remaining, 65536 if count < 0 else count)
        chunk = self.stream.read(count)
        if len(chunk) != count:
            raise ValueError('segment shrank during read')
        self.remaining -= len(chunk)
        if self.digest is not None:
            self.digest.update(chunk)
        return chunk


def segment_lines(path, size, storage_digest=None):
    # No decompression temp file and no whole-file read. gzip validates its footer
    # and CRC when iterated through EOF. The observed physical prefix is fixed.
    with open(path, 'rb') as stream:
        if str(path).endswith('.gz'):
            with gzip.GzipFile(fileobj=PrefixReader(stream, size, storage_digest), mode='rb') as decoded:
                while True:
                    line = decoded.readline()
                    if not line:
                        break
                    yield line
        else:
            remaining = size
            while remaining:
                line = stream.readline(remaining)
                if not line:
                    raise ValueError('segment shrank during read')
                remaining -= len(line)
                if storage_digest is not None:
                    storage_digest.update(line)
                yield line


INTEGRITY_ERRORS = (OSError, EOFError, zlib.error, ValueError)


def read_segment(path):
    size = Path(path).stat().st_size
    records, faults, digest, storage_digest = [], [], hashlib.sha256(), hashlib.sha256()
    consumed = 0
    try:
        for index, line in enumerate(segment_lines(path, size, storage_digest)):
            consumed += len(line)
            digest.update(line)
            try:
                value = json.loads(line)
                if not isinstance(value, dict):
                    raise ValueError('record is not an object')
                if value.get('kind') != 'page':
                    records.append(value)
            except (ValueError, UnicodeError) as error:
                faults.append({'line': index + 1, 'kind': 'incomplete_line' if not line.endswith(b'\n') else 'invalid_line', 'error': str(error)})
    except INTEGRITY_ERRORS as error:
        faults.append({'kind': 'compression_or_storage_integrity', 'error': str(error)})
    return records, faults, digest.hexdigest(), size, consumed, storage_digest.hexdigest()


def iter_records(path, size):
    try:
        for line in segment_lines(path, size):
            try:
                value = json.loads(line)
            except (ValueError, UnicodeError):
                continue  # already recorded by the first pass
            if isinstance(value, dict):
                yield value
    except INTEGRITY_ERRORS:
        return  # first/third passes record integrity faults, never grant coverage


def anchor_block(stamp):
    anchor = stamp['anchor']
    return anchor.get('block', anchor)


def stamp_identity(stamp):
    node, dag = stamp['node'], stamp['dag']
    result = {key: node[key] for key in IDENTITY}
    result['pruningPointHash'] = dag['pruningPointHash']
    if any(value is None or value == '' for value in result.values()):
        raise ValueError('missing identity value')
    if node.get('pruningImportStable') is not True:
        raise ValueError('pruningImportStable must be true')
    block = anchor_block(stamp)
    if block.get('verboseData', {}).get('isChainBlock') is not True:
        raise ValueError('fixed anchor is not canonical')
    return result, block['header']


def analyze(paths):
    rows, observations, faults, segments = {}, collections.defaultdict(list), [], []
    identity = anchor_header = None
    anchor = genesis = cursor = None
    chain_complete = False
    for path in paths:
        records, parse_faults, digest, size, decoded_size, storage_sha = read_segment(path)
        info = {'path': str(path), 'sha256': digest, 'observed_bytes': decoded_size, 'storage_bytes': size, 'storage_sha256': storage_sha,
                'encoding': 'gzip' if str(path).endswith('.gz') else 'identity', 'parse_faults': parse_faults, 'pages': 0, 'errors': [], 'bounded_stops': []}
        segments.append(info)
        faults.extend({'segment': str(path), **f} for f in parse_faults)
        before = [r for r in records if r.get('kind') == 'before']
        after = [r for r in records if r.get('kind') in ('after', 'after_error')]
        if len(before) != 1 or len(after) != 1:
            faults.append({'segment': str(path), 'kind': 'missing_or_duplicate_stamp', 'before': len(before), 'after': len(after)})
        for record in before + after:
            try:
                actual_identity, actual_header = stamp_identity(record['data'])
                if identity is None:
                    identity, anchor_header = actual_identity, actual_header
                    anchor, genesis = actual_header['hash'], identity['genesisHash']
                    cursor = genesis
                elif identity != actual_identity or anchor_header != actual_header:
                    raise ValueError('original identity or anchor header changed')
                if record.get('data', {}).get('anchor', {}).get('requestedHash', anchor) != anchor:
                    raise ValueError('requested anchor changed')
            except (KeyError, TypeError, ValueError) as error:
                faults.append({'segment': str(path), 'kind': 'stamp_invalid', 'error': str(error)})
        started, ended = False, False
        for record in iter_records(path, size):
            kind = record.get('kind')
            if kind == 'before':
                if started or ended:
                    faults.append({'segment': str(path), 'kind': 'stamp_order'})
                started = True
            elif kind in ('after', 'after_error'):
                if not started or ended:
                    faults.append({'segment': str(path), 'kind': 'stamp_order'})
                ended = True
            elif kind == 'error':
                info['errors'].append(record)
                # The original bounded collector emitted budget exhaustion as an error.
                if str(record.get('error', '')).startswith(('bounded 2400 second budget exhausted', 'bounded 1200 page budget exhausted')):
                    info['bounded_stops'].append(record)
                else:
                    faults.append({'segment': str(path), 'kind': 'segment_error', 'error': record.get('error')})
            elif kind == 'stopped':
                info['bounded_stops'].append(record)
            elif kind == 'page':
                if not started or ended:
                    faults.append({'segment': str(path), 'kind': 'page_outside_stamps'})
                blocks = record.get('blocks', [])
                page_index = info['pages']
                info['pages'] += 1
                if record.get('page') != page_index or record.get('cursor') != cursor or not blocks or record.get('count') != len(blocks):
                    faults.append({'segment': str(path), 'kind': 'page_sequence_or_count', 'page': record.get('page')})
                if blocks and blocks[0].get('hash') != cursor:
                    faults.append({'segment': str(path), 'kind': 'noninclusive_cursor', 'page': record.get('page')})
                for block in blocks:
                    try:
                        block_hash, header = block['hash'], block['header']
                        if header['hash'] != block_hash:
                            raise ValueError('hash/header mismatch')
                        header_digest = hashlib.sha256(json.dumps(header, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
                        if block_hash in rows and rows[block_hash]['header_sha256'] != header_digest:
                            raise ValueError('same hash has conflicting headers')
                        verbose = block.get('verboseData') or {}
                        rows.setdefault(block_hash, {'header_sha256': header_digest,
                            'header': {key: header.get(key) for key in ('hash', 'daaScore', 'parentsByLevel', 'powAlgoId')},
                            'verboseData': {key: verbose.get(key) for key in ('selectedParentHash', 'blockKind', 'exec', 'palwMergeView')}})
                        observations[block_hash].append({'segment': str(path), 'page': record.get('page'),
                            'utc': record.get('utc'), 'verboseData': {key: verbose.get(key) for key in
                            ('selectedParentHash', 'blockKind', 'exec', 'palwMergeView')}})
                    except (KeyError, TypeError, ValueError) as error:
                        faults.append({'segment': str(path), 'kind': 'header_conflict_or_invalid', 'error': str(error)})
                if blocks:
                    next_cursor = blocks[-1].get('hash')
                    if next_cursor == cursor and anchor not in rows:
                        faults.append({'segment': str(path), 'kind': 'no_progress'})
                    cursor = next_cursor
            elif kind not in ('before', 'after', 'after_error', 'stopped'):
                faults.append({'segment': str(path), 'kind': 'unknown_record', 'value': kind})
        check_digest, check_storage = hashlib.sha256(), hashlib.sha256()
        try:
            for line in segment_lines(path, size, check_storage):
                check_digest.update(line)
        except INTEGRITY_ERRORS as error:
            faults.append({'segment': str(path), 'kind': 'compression_or_storage_integrity', 'error': str(error)})
        if check_digest.hexdigest() != digest or check_storage.hexdigest() != storage_sha:
            faults.append({'segment': str(path), 'kind': 'segment_changed_during_analysis'})
        info['last_complete_page_cursor'] = cursor
    closure, holes, stack = set(), set(), [anchor] if anchor in rows else []
    while stack:
        block_hash = stack.pop()
        if block_hash in closure:
            continue
        if block_hash not in rows:
            holes.add(block_hash)
            continue
        closure.add(block_hash)
        header = rows[block_hash]['header']
        parents = header.get('parentsByLevel')
        if not isinstance(parents, list):
            faults.append({'kind': 'missing_parent_levels', 'hash': block_hash})
        elif parents:
            stack.extend(parents[0])
    selected, selected_holes, selected_seen = [], [], set()
    block_hash = anchor
    while block_hash:
        if block_hash in selected_seen:
            faults.append({'kind': 'selected_parent_cycle', 'hash': block_hash})
            break
        selected_seen.add(block_hash)
        if block_hash not in rows:
            selected_holes.append(block_hash)
            break
        selected.append(block_hash)
        if block_hash == genesis:
            chain_complete = True
            break
        block_hash = (rows[block_hash].get('verboseData') or {}).get('selectedParentHash')
        if block_hash is None:
            faults.append({'kind': 'missing_selected_parent', 'hash': selected[-1]})
    # Rows keep immutable-header/first-metadata identity; all later metadata is
    # retained explicitly, never silently substituted into that first snapshot.
    verdict_observations, verdict_transitions, known_conflicts = {}, {}, []
    for observed_hash, values in observations.items():
        trace = []
        for value in values:
            verdict = (value['verboseData'].get('exec') or {}).get('verdict')
            trace.append({'segment': value['segment'], 'page': value['page'], 'utc': value['utc'], 'verdict': verdict})
        if any(v['verdict'] is not None for v in trace):
            verdict_observations[observed_hash] = values
            transitions = []
            previous = None
            for index, value in enumerate(trace):
                if index and value['verdict'] != previous:
                    transitions.append({'from': previous, 'to': value['verdict'], 'observation': index})
                previous = value['verdict']
            if transitions:
                verdict_transitions[observed_hash] = transitions
            known = {v['verdict'] for v in trace if v['verdict'] in ('granted', 'refused')}
            if len(known) > 1:
                known_conflicts.append(observed_hash)
    kinds, verdicts, merge_rounds = collections.Counter(), collections.Counter(), set()
    missing_merge_views, round_without_exec = [], []
    round_header_count = 0
    scope = closure if anchor in rows else set(rows)
    for block_hash in scope:
        verbose = rows[block_hash].get('verboseData') or {}
        kinds[verbose.get('blockKind', 'missing')] += 1
        execution = verbose.get('exec')
        if rows[block_hash]['header'].get('powAlgoId') == 10:  # pow_layer0::POW_ALGO_ID_PALW_ROUND_V1
            round_header_count += 1
            if not execution:
                round_without_exec.append(block_hash)
        if execution:
            verdicts[execution.get('verdict', 'missing')] += 1
    for block_hash in selected:
        view = (rows[block_hash].get('verboseData') or {}).get('palwMergeView')
        if view is None:
            missing_merge_views.append(block_hash)
        else:
            merge_rounds.update(view.get('roundBlocks', []))
    closed = bool(identity and anchor in rows and not faults and not holes and chain_complete and not known_conflicts)
    return {'schema': 'misaka.public-history.coverage.v1', 'coverage_status': 'COMPLETE_TO_ANCHOR' if closed else 'UNKNOWN_PARTIAL',
            'compatibility_status': 'UNKNOWN', 'original_identity': identity, 'anchor': anchor,
            'anchor_header': anchor_header, 'segments': segments, 'faults': faults,
            'unique_blocks': len(rows), 'max_daa': max((b['header'].get('daaScore', 0) for b in rows.values()), default=None),
            'anchor_present': anchor in rows, 'ancestor_closure_count': len(closure), 'ancestor_holes': sorted(holes),
            'selected_chain_count': len(selected), 'selected_chain_holes': selected_holes, 'selected_chain_reaches_genesis': chain_complete,
            'last_complete_page_cursor': cursor, 'block_kinds': dict(kinds), 'observed_node_verdicts': dict(verdicts), 'round_header_count': round_header_count,
            'round_headers_without_exec': sorted(round_without_exec),
            'verdict_observation_policy': 'All observations in segment/page order. Counts and merge traversal use first observed metadata; no latest substitution.',
            'verdict_observations': verdict_observations, 'verdict_transitions': verdict_transitions,
            'conflicting_known_verdicts': sorted(known_conflicts),
            'selected_chain_round_merge_hashes': sorted(merge_rounds), 'missing_merge_views': missing_merge_views,
            'limits': ['Coverage is header ancestry only; granted/refused/unknown is bounded volatile node telemetry.',
                       'Round merge membership is not a permit verdict. Missing merge view remains unknown.',
                       'No derived EVM gas-header read or upgraded-binary replay; EVM compatibility always UNKNOWN.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('segments', nargs='+')
    args = parser.parse_args()
    print(json.dumps(analyze(args.segments), ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
