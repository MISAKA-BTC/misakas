// Read-only snapshot of public t12 capacity-relevant state. One WebSocket, sequential calls.
import fs from 'node:fs';
const URL = 'wss://misakascan.com/kaspa'; // read-only public endpoint
const OUT = process.argv[2];
const TX = '5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669';
const FLOOR = 'f1c5635c6e47e96e7af864789c94523335dc56584af297cb8cc19021c228b897bee1a50145597e45f8ca2727349bf4aa352a98cc05274b7f059a176642f623c8';
const K8 = 'ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01';
const ws = new WebSocket(URL);
let id = 0; const pending = new Map();
function call(method, params) { return new Promise((res, rej) => { const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); setTimeout(() => { if (pending.has(i)) { pending.delete(i); rej(new Error('timeout ' + method)); } }, 30000); }); }
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); const p = pending.get(m.id); if (p) { pending.delete(m.id); p.res(m.params ?? m.result ?? m.error ?? m); } };
ws.onerror = () => { console.error('ws error'); process.exit(1); };
ws.onopen = async () => {
  const snap = { t: Date.now() };
  try {
    const dag = await call('getBlockDagInfo', {}); snap.daa = dag.virtualDaaScore; snap.blocks = dag.blockCount; snap.pmt = dag.pastMedianTime;
    const reg = await call('getPalwModelRegistry', {});
    snap.classes = reg.classes.map(c => ({ id: c.classId.slice(0, 8), state: c.state, inflight: c.inflightClaims, room: c.panelRoom, max: c.maxInflightClaims, ready: c.readySeatsNow, util: c.utilizationPermille, ncp: c.noCapablePanelVoids }));
    snap.bonds = [];
    for (let i = 0; i < 8; i++) {
      const f = await call('getPalwProducerFacts', { classId: FLOOR, bondTransactionId: TX, bondIndex: i, withBond: true });
      const f8 = await call('getPalwProducerFacts', { classId: K8, bondTransactionId: TX, bondIndex: i, withBond: true });
      const cl = await call('getPalwClaims', { bond: `${TX}:${i}`, role: 'executor', includeTerminal: true, limit: 0 });
      snap.bonds.push({ i, committed: f.bondCommitted, reserved: f.bondReservedExposure, accuser: f.bondAccuserExposure, ceiling: f.bondExposureCeiling, collateral: f.bondCollateral, refusalFloor: f.notReadyReason ?? null, refusal8k: f8.notReadyReason ?? null, claimExp8k: f8.bondClaimExposure, claimExpFloor: f.bondClaimExposure,
        claims: (cl.claims || []).map(c => [c.claimId.slice(0, 12), c.classId.slice(0, 4), c.phase[0] + c.phase.split('_')[1]?.[0], c.acceptedDaa, c.boundDaa ?? -1, c.phaseDaa, c.voidReason || '', (c.seats || []).length]) });
    }
  } catch (e) { snap.err = String(e); }
  fs.writeFileSync(OUT, JSON.stringify(snap));
  console.log('snap daa', snap.daa, snap.err || '');
  process.exit(0);
};
setTimeout(() => { console.error('global timeout'); process.exit(2); }, 240000);
