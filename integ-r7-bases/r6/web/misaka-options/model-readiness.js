/* Chain-backed production readiness, independent of a store's Open/Not open state.
 * Classic script for the existing no-build frontend; injected RPC transport, no VPS DB.
 */
(function (root) {
  'use strict';
  const cache = new Map();
  let pending = null, lastRefresh = 0;
  const escape = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const positive = value => { try { return BigInt(value || 0) > 0n; } catch (_) { return false; } };
  const seatCount = model => model?.available === true && model?.found === true && Number.isSafeInteger(model.readySeats) && model.readySeats >= 0 ? model.readySeats : null;
  function classify(model, probes, economics, probeComplete) {
    const ledger = economics?.ledger;
    const finals = ledger?.available === true ? ledger.finals : economics?.claimsFinal;
    const rewardRecorded = ledger?.available === true && positive(ledger.producerNamedSompi ?? ledger.producerPaidSompi);
    const history = finals == null ? 'Final unknown' : `Final ${finals}${rewardRecorded ? ' · reward recorded' : ''}`;
    if (model?.available !== true || model?.found !== true) return {label:'Unknown', tone:'dim', history, reason:'The node has not confirmed this class registration.'};
    const passing = probes.find(p => p?.available === true && p.bondKnown === true && p.notReadyReason === '');
    if (passing) return {label:'Ready', tone:'ok', history, reason:`The node permits an inspected registered bond to produce at DAA ${passing.daaScore}. This is eligibility, not proof that a producer is running or that the next claim will reach Final.`};
    const required = Number(model.requiredReadySeats), ready = Number(model.readySeats);
    const seats = Number.isFinite(required) && Number.isFinite(ready) ? `${ready}/${required} seats` : 'Seats unknown';
    const refusal = probes.find(p => p?.available === true && p.bondKnown === true && p.notReadyReason)?.notReadyReason;
    if (refusal) return {label:'Not ready', tone:'warn', history, reason:`${seats}. Inspected bonds are not eligible: ${refusal}`};
    return {label:probeComplete ? 'Registered only' : 'Unknown', tone:'dim', history, reason:`${seats}. No inspected bond has a confirmed ready-to-produce verdict. ${model.reason || model.registryState || ''}`};
  }
  function cell(rec, compact = false) {
    const entry = cache.get(rec.classId || rec.lineId);
    const state = entry && Date.now() - entry.at < 45000 ? entry.state : {label:'Checking…',tone:'dim',history:'',reason:'Reading registration, producer eligibility and Final records from the node.'};
    const explanation = state.reason + ' Final counts are the node’s retained records, not a lifetime total. Reward recorded means named/vested by Final; it does not prove spendable payout. Market opening and member benefits are separate.';
    return `<span class="model-production ${escape(state.tone)}" title="${escape(explanation)}"><span class="production-label">${escape(state.label)}</span><small>${escape(compact ? state.history.replace(/ · reward recorded$/, '') : state.history)}</small></span>`;
  }
  function panelCell(rec) {
    const entry = cache.get(rec.classId || rec.lineId);
    const fresh = entry && Date.now() - entry.at < 45000;
    const count = fresh ? entry.readySeats : null;
    const required = fresh ? entry.requiredReadySeats : null;
    const explanation = 'Chain-confirmed ready Panel seats for this model' + (Number.isSafeInteger(required) ? `; required: ${required}` : '') + '. This counts readiness records, not live connections or distinct operators. It does not by itself prove production eligibility.';
    return `<span class="panel-ready-count" title="${escape(explanation)}">${count == null ? '—' : count}</span>`;
  }
  async function parallel(items, fn, width = 3) {
    let index = 0;
    await Promise.all(Array.from({length:Math.min(width,items.length)}, async () => { while (index < items.length) { const item = items[index++]; await fn(item); } }));
  }
  function refresh(records, rpc, notify = () => {}) {
    if (pending) return pending;
    const ids = [...new Set(records.map(r => r.classId || r.lineId).filter(Boolean))];
    if (Date.now() - lastRefresh < 20000 && ids.every(id => cache.has(id))) return Promise.resolve();
    pending = (async () => {
      const safe = (method, params) => Promise.resolve().then(() => rpc(method, params)).catch(() => null);
      const [seats, economics] = await Promise.all([safe('getPalwPanelSeats', {classId:''}), safe('getPalwClassEconomics', {})]);
      // The fleet is a discovery source, never authority for readiness. Probe registered bonds
      // with the chain's own ready_to_produce verdict; a seat count alone must not turn green.
      const fleet = [...new Set((seats?.seats || []).map(s => s.bondOutpoint).filter(Boolean))].slice(0,16);
      const econ = new Map((economics?.available ? economics.classes || [] : []).map(e => [e.classId,e]));
      await parallel(ids, async classId => {
        const model = await safe('getPalwModel', {classId});
        const owners = records.filter(r => (r.classId || r.lineId) === classId).map(r => r.row?.owner).filter(o => o?.transactionId).map(o => `${o.transactionId}:${o.index}`);
        const bonds = [...new Set([...owners,...fleet])].slice(0,16);
        const probes = [];
        const entry = {at:Date.now(),readySeats:seatCount(model),requiredReadySeats:model?.requiredReadySeats,state:classify(model,[],econ.get(classId),false)};
        cache.set(classId,entry);
        notify(); // Panel count and registration/Final history never wait for all bond probes.
        // Always ask the consensus verdict. The built-in floor is explicitly not gated
        // by Panel quorum, so readySeats < requiredReadySeats is not a universal refusal.
        if (model?.found) {
          let permitted = false;
          await parallel(bonds, async bond => {
            if (permitted) return;
            const match = /^([0-9a-f]{128}):(\d+)$/.exec(bond);
            if (!match) return;
            const probe = await safe('getPalwProducerFacts', {classId,bondTransactionId:match[1],bondIndex:Number(match[2]),withBond:true});
            probes.push(probe);
            if (probe?.available === true && probe.bondKnown === true && probe.notReadyReason === '') permitted = true;
          },4);
        }
        cache.set(classId,{at:Date.now(),readySeats:seatCount(model),requiredReadySeats:model?.requiredReadySeats,state:classify(model,probes,econ.get(classId),seats?.available === true && probes.every(Boolean))});
        notify();
      });
      lastRefresh = Date.now();
    })().finally(() => { pending = null; });
    return pending;
  }
  const phoneCell = rec => `<div class="production-phone">${cell(rec,true)} <small>· Panel ${panelCell(rec)}</small></div>`;
  const api = {cell,panelCell,phoneCell,refresh,classify,seatCount};
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.MISAKA_READINESS = api;
})(typeof window === 'undefined' ? globalThis : window);
