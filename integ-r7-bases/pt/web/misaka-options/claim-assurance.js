/* RFC-0015 §11.3: one kernel claim's settlement mode and assurance, read from `getPalwKernelClaim` (op 210).
 * Classic script for the no-build frontend; the RPC transport is injected. Nothing here judges a claim: every figure is the
 * node's read, and an OptimisticPublicVerification Final is never presented as a proof that the computation is correct.
 */
(function (root) {
  'use strict';
  const escape = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const NOT_PROOF = 'No accepted challenge is not a proof of correctness: optimistic public verification assumes an honest, capable '
    + 'verifier checked the claim in time. Watcher or receipt counts are not shown as evidence of independence or honesty.';
  const SETTLEMENT = {
    'pending': ['Pending', 'dim', 'Not Final. Nothing about this claim is settled.'],
    'final-revocable': ['Final · still convictable', 'warn',
      'Final, but a valid fraud proof can still convict it until its liability horizon ends. Not safe for an irreversible release.'],
    'settled': ['Settled', 'ok', 'Final and past its liability horizon with no conviction or default.'],
    'convicted': ['Convicted', 'bad', 'An objective fraud proof convicted this claim; it is void.'],
    'defaulted': ['Defaulted', 'bad', 'Required material was not served (availability default, not a fraud conviction), or it timed out.'],
  };
  const num = v => { const n = Number(v); return Number.isSafeInteger(n) ? n : null; };
  /** The display facts of one `getPalwKernelClaim` response (pure; tested by claim-assurance.test.cjs). */
  function describe(read) {
    if (!read || read.available !== true) return {found:false, label:'Unavailable', tone:'dim', reason:'The node serves no kernel route (dormant on this network).'};
    if (read.found !== true) return {found:false, label:'Not found', tone:'dim', reason:'The node holds no kernel claim with this id.'};
    const opv = read.mode === 'OptimisticPublicVerification';
    const [label, tone, why] = SETTLEMENT[read.settlement] || ['Unknown', 'dim', 'This node did not report a settlement standing (an older node).'];
    const tip = num(read.tipDaa);
    const clockFacts = opv && read.opv === true ? {
      admitted: num(read.opvAdmittedDaa),
      startCutoff: num(read.opvVerifierStartCutoffDaa),
      finalFloor: num(read.opvFinalFloorDaa),
      hardDeadline: num(read.opvHardDeadlineDaa),
      open: tip != null && num(read.opvFinalFloorDaa) != null && tip < num(read.opvFinalFloorDaa),
    } : null;
    const openDemands = Array.isArray(read.demands) ? read.demands.length : 0;
    const assurance = opv
      ? 'Optimistic public verification (Panel=0): permissionless verifiers, a fixed public challenge window, objective fraud proofs.'
      : 'Panel-licensed: a fixed Panel covered the claim before its challenge window.';
    return {
      found: true, opv, label, tone, reason: why, assurance, window: clockFacts, openDemands,
      mode: opv ? 'OptimisticPublicVerification' : 'PanelLicensed',
      state: String(read.state || ''),
      settledFrom: num(read.settledFromDaa) || null,
      statement: String(read.finalStatement || ''),
      notProof: opv ? NOT_PROOF : '',
      safe: read.settlement === 'settled',
    };
  }
  function render(read) {
    const d = describe(read);
    if (!d.found) return `<div class="claim-assurance ${escape(d.tone)}"><b>${escape(d.label)}</b> <span class="dim">${escape(d.reason)}</span></div>`;
    const w = d.window;
    const clock = w ? `<dl class="claim-clock">
        <dt>Challengeable from</dt><dd>DAA ${escape(w.admitted)}</dd>
        <dt>Verifier start cutoff</dt><dd>DAA ${escape(w.startCutoff)}</dd>
        <dt>Earliest Final (window end)</dt><dd>DAA ${escape(w.finalFloor)}${w.open ? ' · window open' : ''}</dd>
        <dt>Hard deadline</dt><dd>DAA ${escape(w.hardDeadline)}</dd>
      </dl>` : '';
    return `<div class="claim-assurance ${escape(d.tone)}">
      <div><span class="tag">${escape(d.mode)}</span> <b>${escape(d.label)}</b>${d.settledFrom ? ` <small>settles from DAA ${escape(d.settledFrom)}</small>` : ''}</div>
      <p>${escape(d.reason)}</p>
      <p class="dim">${escape(d.assurance)} State: ${escape(d.state)}. Open demands: ${escape(d.openDemands)}.</p>
      ${clock}
      ${d.notProof ? `<p class="note"><b>No challenge ≠ proof of correctness.</b> ${escape(d.notProof)}</p>` : ''}
      ${d.statement ? `<p class="dim tiny">${escape(d.statement)}</p>` : ''}
    </div>`;
  }
  /** Read one claim through the injected transport (`rpc(method, params)`). */
  async function load(rpc, claimId) {
    return rpc('getPalwKernelClaim', {claimId});
  }
  const api = {describe, render, load, NOT_PROOF};
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.MISAKA_CLAIM_ASSURANCE = api;
})(typeof window === 'undefined' ? globalThis : window);
