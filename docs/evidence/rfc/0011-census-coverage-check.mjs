// Read-only arithmetic check of 0011-hf-census-coverage-2026-10-08.json. It does NOT run models, enumerate HF or register anything:
// it recomputes what the evidence file says about itself — that the buckets partition D_all, that external + software-closable +
// shape-ready equal D_all, that every point estimate lies above its lower bound, that the ceiling is 1 - external/D_all, that the
// "registered" column never exceeds a shape-ready count, and that the acceptance bar is stated against the registered count only.
// Run from any directory: node docs/rfc/evidence/0011-census-coverage-check.mjs [--check|--self-test]
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const file = resolve(here, '0011-hf-census-coverage-2026-10-08.json');
const EXTERNAL = ['MISSING_WEIGHTS', 'GATED', 'ADAPTER_BASE_MISSING', 'NO_MODEL_TASK'];
const CLOSABLE = ['FRONTEND', 'NEW_KERNEL', 'QUANT_FORMAT', 'RESOURCE', 'UNTESTED'];

function near(a, b, tol, what) {
  assert(Math.abs(a - b) <= tol, `${what}: ${a} vs ${b} (tol ${tol})`);
}

export function checkRuleset(name, r, dAll) {
  const by = Object.fromEntries(r.buckets.map(b => [b.bucket, b.repos_est]));
  const total = Object.values(by).reduce((x, y) => x + y, 0);
  near(total, dAll, 2, `${name}: the buckets partition D_all`);
  const external = EXTERNAL.reduce((x, b) => x + (by[b] ?? 0), 0);
  const closable = CLOSABLE.reduce((x, b) => x + (by[b] ?? 0), 0);
  near(external + closable + by.SHAPE_READY, dAll, 2, `${name}: external + closable + shape-ready`);
  near(r.external_failures.repos_est, external, 2, `${name}: external failures`);
  near(r.software_closable_failures.repos_est, closable, 2, `${name}: software-closable failures`);
  near(r.ceiling_if_every_software_closable_fixed.ceiling, 1 - external / dAll, 1e-6, `${name}: ceiling = 1 - external/D_all`);
  near(r.d_b.total, dAll - external, 2, `${name}: D_b = D_all - external`);
  const s = r.shape_ready_over_d_all;
  assert(s.lb95 <= s.share && s.share <= 1, `${name}: LB <= point`);
  near(s.share * dAll, by.SHAPE_READY, 2, `${name}: shape-ready point`);
  const sb = r.shape_ready_over_d_b;
  assert(sb.lb95 <= sb.share && sb.share <= 1, `${name}: LB over D_b <= point`);
  near(sb.share * r.d_b.total, by.SHAPE_READY, 2, `${name}: shape-ready over D_b`);
  // The ceiling is the most any rate over D_all could reach: no shape-ready or registered share may exceed it.
  assert(s.share <= r.ceiling_if_every_software_closable_fixed.ceiling + 1e-9, `${name}: shape-ready under the ceiling`);
  // Context split partitions shape-ready.
  const cs = r.shape_ready_context_split_over_d_all;
  near(cs.at_declared_context.total + cs.at_cap_8192_declared_wider.total, by.SHAPE_READY, 2, `${name}: context split`);
}

function checkAll(e) {
  assert.equal(e.schema, 'misaka.rfc11.hf-census-coverage.v1');
  const dAll = e.snapshot.d_all;
  assert(Number.isInteger(dAll) && dAll > 0);
  for (const [name, r] of Object.entries(e.rulesets)) checkRuleset(name, r, dAll);
  const reg = e.registration_columns;
  // Only the last column is "registered": nothing weaker than a full-task Active/Final registration may be counted as it.
  assert.equal(reg.registered_full_task_active_or_final.count, 0, 'no full-task Active/Final registration is evidenced');
  for (const [name, r] of Object.entries(e.rulesets)) {
    assert(reg.registered_full_task_active_or_final.count <= r.buckets.find(b => b.bucket === 'SHAPE_READY').repos_est, `${name}: registered <= shape-ready`);
  }
  assert.equal(e.acceptance_bar.numerator, 'registered_full_task_active_or_final');
  assert.equal(e.acceptance_bar.met, false);
  assert.equal(e.network.requests, e.network.requests | 0);
}

if (process.argv[2] === '--self-test') {
  const ok = { buckets: [{ bucket: 'SHAPE_READY', repos_est: 10 }, { bucket: 'MISSING_WEIGHTS', repos_est: 30 }, { bucket: 'FRONTEND', repos_est: 60 }],
    external_failures: { repos_est: 30 }, software_closable_failures: { repos_est: 60 }, ceiling_if_every_software_closable_fixed: { ceiling: 0.7 },
    d_b: { total: 70 }, shape_ready_over_d_all: { share: 0.1, lb95: 0.05 }, shape_ready_over_d_b: { share: 10 / 70, lb95: 0.05 },
    shape_ready_context_split_over_d_all: { at_declared_context: { total: 4 }, at_cap_8192_declared_wider: { total: 6 } } };
  checkRuleset('ok', ok, 100);
  assert.throws(() => checkRuleset('x', { ...ok, buckets: [...ok.buckets, { bucket: 'GATED', repos_est: 5 }] }, 100), /partition/);
  assert.throws(() => checkRuleset('x', { ...ok, shape_ready_over_d_all: { share: 0.1, lb95: 0.2 } }, 100), /LB/);
  assert.throws(() => checkRuleset('x', { ...ok, ceiling_if_every_software_closable_fixed: { ceiling: 0.9 } }, 100), /ceiling/);
  assert.throws(() => checkRuleset('x', { ...ok, shape_ready_context_split_over_d_all: { at_declared_context: { total: 4 }, at_cap_8192_declared_wider: { total: 20 } } }, 100), /context split/);
  console.log('PASS: buckets that do not partition D_all, an inverted bound, a wrong ceiling or a wrong context split are refused.');
} else {
  assert([undefined, '--check'].includes(process.argv[2]), 'unknown option');
  checkAll(JSON.parse(readFileSync(file)));
  console.log('PASS: the census coverage evidence is arithmetically consistent. Registered (full-task Active/Final) coverage: 0; the 90% bar is not met.');
}
