// Read-only: for every executor claim of the given genesis bonds, fetch panel assignment summary.
import fs from 'node:fs';
const URL = 'wss://misakascan.com/kaspa'; // read-only public endpoint
const TX = '5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669';
const bonds = process.argv[3].split(',').map(Number);
const ws = new WebSocket(URL);
let id = 0; const pending = new Map();
function call(method, params) { return new Promise((res, rej) => { const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); setTimeout(() => { if (pending.has(i)) { pending.delete(i); rej(new Error('timeout ' + method)); } }, 30000); }); }
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); const p = pending.get(m.id); if (p) { pending.delete(m.id); p.res(m.params ?? m.result ?? m.error ?? m); } };
ws.onopen = async () => {
  const out = [];
  for (const b of bonds) {
    const cl = await call('getPalwClaims', { bond: `${TX}:${b}`, role: 'executor', includeTerminal: true, limit: 0 });
    for (const c of cl.claims) {
      const a = await call('getPalwPanelAssignments', { claimId: c.claimId, seatId: '' });
      const x = (a.assignments || [])[0];
      out.push({ bond: b, claim: c.claimId.slice(0, 16), cls: c.classId.slice(0, 4), phase: c.phase, acc: c.acceptedDaa, bound: c.boundDaa ?? null, pd: c.phaseDaa, dl: c.deadlineDaa ?? null, void: c.voidReason,
        valid: x ? x.validReceiptSeats : null, sel: x ? x.selectedPanelSeats : null, cov: x ? x.coverageMask : null, lic: x ? x.licensedState : null });
    }
  }
  fs.writeFileSync(process.argv[2], JSON.stringify({ t: Date.now(), rows: out }));
  console.log('rows', out.length); process.exit(0);
};
setTimeout(() => { console.error('global timeout'); process.exit(2); }, 600000);
