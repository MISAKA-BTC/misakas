import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

// Exercise the actual browser classification functions without opening a websocket or polling.
const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const context = vm.createContext({
  scanIsRound: b => Number(b.algo) === 10,
  llmIsFloor: id => id === 'floor',
  esc: value => String(value),
});
const section = (start, end) => source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));
vm.runInContext(section('function scanKindOf(', '// ---- claims of an executor bond'), context);
vm.runInContext(section('function scanTypeOf(', 'function scanBlockContext('), context);
const classify = block => context.scanTypeOf(block);

test('a refused round from an older node is execution, never RED', () => {
  for (const laneClass of ['RED', 'ROUND', 'EXEC', '']) {
    const result = classify({ algo: 10, laneClass, exec: { verdict: 'refused' } });
    assert.equal(result.cat, 'exec');
    assert.match(result.pill, /E \/ ROUND/);
    assert.match(result.pill, /permit refused/);
    assert.doesNotMatch(result.pill, /RED|BLUE/);
  }
});

test('a genuine red is red and an unclassified off-chain block stays unknown', () => {
  assert.equal(classify({ algo: 6, laneClass: 'RED' }).cat, 'red');
  assert.equal(classify({ algo: 6, laneClass: '' }).cat, 'unknown');
  assert.equal(classify({ algo: 10, laneClass: '', exec: null }).cat, 'exec');
  assert.match(classify({ algo: 10 }).pill, /verdict unknown/);
});

test('heartbeat and BASE-0 keep separate labels beside ordinary consensus blocks', () => {
  assert.match(classify({ algo: 8, isChain: true, nodeKind: 'LEGACY_HEARTBEAT' }).pill, /HEARTBEAT/);
  assert.match(classify({ algo: 6, classId: 'floor', isChain: true, nodeKind: 'LEGACY_FLOOR' }).pill, /BASE-0/);
  assert.match(classify({ algo: 6, classId: 'model', laneClass: 'BLUE' }).pill, /C-BLUE/);
  assert.equal(classify({ algo: 8, laneClass: 'RED' }).cat, 'red');
});

test('accepted rounds remain execution even if an older kind says REAL_ROUND', () => {
  const result = classify({ algo: 10, nodeKind: 'REAL_ROUND', laneClass: 'EXEC', exec: { verdict: 'granted' } });
  assert.equal(result.cat, 'exec');
  assert.match(result.pill, /accepted/);
  assert.doesNotMatch(result.pill, /BLUE/);
});
