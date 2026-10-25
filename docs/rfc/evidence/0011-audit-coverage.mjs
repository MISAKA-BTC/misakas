// Read-only audit of existing reports. This does NOT run models or measure HF coverage.
// Run from any directory: node docs/rfc/evidence/0011-audit-coverage.mjs [--check|--self-test]
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../../..');
const sources = [
  'misaka-palw-tir-lower/tools/corpus/report.json',
  'misaka-palw-tir-lower/tools/corpus/census_report.json',
  'misaka-palw-sdk/tests/golden/corpus_preflight_v1.json',
  'misaka-palw-tir-lower/tests/corpus_v2.rs',
  'misaka-palw-sdk/tests/corpus_preflight.rs',
  'misaka-palw-tir-lower/tools/corpus/corpus_v2.json',
  'misaka-palw-tir-lower/tools/corpus/census_v2.json',
];
const bytes = sources.map(p => readFileSync(resolve(root, p)));
const [corpus, tail, pins] = bytes.slice(0, 3).map(b => JSON.parse(b));
const [corpusManifest, tailManifest] = bytes.slice(5).map(b => JSON.parse(b));

function count(values) {
  return Object.fromEntries([...new Set(values)].sort().map(k => [k, values.filter(v => v === k).length]));
}

function entriesOf(report) {
  assert(Array.isArray(report.entries) && report.entries.length > 0, 'nonempty entries required');
  const ids = report.entries.map(e => e.id);
  assert(ids.every(id => typeof id === 'string' && id.length), 'entry id required');
  assert.equal(new Set(ids).size, ids.length, 'duplicate ids');
  assert(report.entries.every(e => ['A', 'B', 'C'].includes(e.level)), 'unknown support level');
  return report.entries;
}

function architectureSummary(report) {
  const entries = entriesOf(report);
  return {
    entries: entries.length,
    levels: count(entries.map(e => e.level)),
    a_or_b: entries.filter(e => ['A', 'B'].includes(e.level)).length,
    failed_stage: count(entries.map(e => e.failed_stage ?? 'not_recorded')),
  };
}

function okThrough(row, stages) {
  return stages.every(stage => row?.[stage]?.status === 'ok');
}

if (process.argv[2] === '--self-test') {
  assert.throws(() => entriesOf({ entries: [] }));
  assert.throws(() => entriesOf({ entries: [{ id: 'x', level: 'A' }, { id: 'x', level: 'B' }] }));
  assert.throws(() => entriesOf({ entries: [{ id: 'x', level: 'unknown' }] }));
  assert(!okThrough({ register: { status: 'ok' } }, ['convert', 'register']));
  assert(!okThrough({ convert: { status: 'unknown' }, register: { status: 'ok' } }, ['convert', 'register']));
  assert(!okThrough({ convert: { status: 'blocked' }, register: { status: 'ok' } }, ['convert', 'register']));
  assert(okThrough({ convert: { status: 'ok' }, register: { status: 'ok' } }, ['convert', 'register']));
  console.log('PASS: missing, unknown and blocked stages cannot become joint successes; invalid entries refused.');
} else {
  assert([undefined, '--check'].includes(process.argv[2]), 'unknown option');
  const entries = entriesOf(corpus);
  assert.deepEqual(entries.map(e => e.id).sort(), corpusManifest.entries.map(e => e.id).sort(), 'curated manifest/report mismatch');
  assert.deepEqual(entriesOf(tail).map(e => e.id).sort(), tailManifest.entries.map(e => e.id).sort(), 'additional manifest/report mismatch');
  assert.deepEqual(Object.keys(pins).sort(), entries.map(e => e.id).sort(), 'corpus/preflight ID mismatch');
  const rows = Object.values(pins);
  const result = {
    schema: 'misaka.rfc11.existing-coverage-audit.v1',
    meaning: 'Recount of saved reports, not fresh execution or an HF probability sample.',
    inputs: sources.map((path, i) => ({ path, sha256: createHash('sha256').update(bytes[i]).digest('hex') })),
    architecture_corpus: architectureSummary(corpus),
    additional_architecture_report: architectureSummary(tail),
    pinned_shape_preflight: {
      entries: rows.length,
      default_context: 128,
      material: 'Header-only safetensors from curated light specs; no checkpoint tensor data.',
      stages: Object.fromEntries(['convert', 'register', 'mine'].map(s => [s, count(rows.map(row => row[s]?.status ?? 'missing'))])),
      convert_and_register_ok: rows.filter(row => okThrough(row, ['convert', 'register'])).length,
      corpus_a_or_b_and_convert_and_register_ok: entries.filter(e => ['A', 'B'].includes(e.level) && okThrough(pins[e.id], ['convert', 'register'])).length,
      registration_blocked: Object.entries(pins).filter(([, row]) => row.register?.status === 'blocked').map(([id, row]) => ({ id, blockers: row.register.blockers })),
    },
    verdict: {
      hub_registration_coverage: 'UNPROVEN',
      accepted_chain_evidence: 'Not established by these reports or their harnesses.',
      caveat: 'A/B is expressibility, register:ok is a saved preflight stage, and neither is accepted full-task registration. Missing stages are not passes; status counts are not a sequential funnel.',
    },
  };
  if (process.argv[2] === '--check') {
    assert.deepEqual(result, JSON.parse(readFileSync(resolve(here, '0011-existing-coverage-audit.json'))));
    console.log('PASS: saved audit matches source hashes and recomputed counts. HF registration coverage remains UNPROVEN.');
  } else {
    console.log(JSON.stringify(result, null, 2));
  }
}
