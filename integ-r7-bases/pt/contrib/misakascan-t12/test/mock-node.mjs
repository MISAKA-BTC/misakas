// Local test harness for the explorer: node test/mock-node.mjs [port] [--new-rpc]
// Serves contrib/misakascan-t12 and a fake JSON wRPC node on /kaspa. Read-only fixtures, no real node.
// --new-rpc adds the node-side lane fields (verboseData.laneClass/exec, getPalwRoundLane.health/recentExecutions).
import http from "node:http"; import crypto from "node:crypto"; import fs from "node:fs"; import path from "node:path"; import { fileURLToPath } from "node:url";
const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.argv[2] || 8765), NEW = process.argv.includes("--new-rpc");
const now = Date.now();
const h64 = (n) => (n.toString(16).padStart(8, "0")).repeat(16);
const BOND_A = { tx: h64(0xa1), ix: 0 }, BOND_B = { tx: h64(0xb2), ix: 1 };
const CLASS_8K = "ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01";
const le = (v, n) => { const b = Buffer.alloc(n); let x = BigInt(v); for (let i = 0; i < n; i++) { b[i] = Number(x & 0xffn); x >>= 8n; } return b; };
const pxr = (round, permit, bond) => Buffer.concat([Buffer.from("PXR1"), Buffer.from([1]), Buffer.alloc(64, 7), le(round, 8), le(permit, 2), Buffer.from(bond.tx, "hex"), le(bond.ix, 4), le(0, 4), le(0, 4)]).toString("hex");
const pav2 = (cls) => Buffer.concat([Buffer.from('PAV2'), le(2, 2), Buffer.alloc(64, 1), Buffer.alloc(64, 2), Buffer.from(cls, 'hex'), Buffer.alloc(64, 3), le(0, 4), le(0, 4), Buffer.alloc(64, 4), Buffer.alloc(64, 5), Buffer.alloc(64, 6), Buffer.alloc(64, 7), le(1, 8), Buffer.alloc(64, 8), le(1, 4), le(1, 8), Buffer.alloc(64, 9)]).toString('hex');
const FLOOR = 'f1c5635c6e47e96e7af864789c94523335dc56584af297cb8cc19021c228b897bee1a50145597e45f8ca2727349bf4aa352a98cc05274b7f059a176642f623c8';
const claims = [
  { claimId: h64(0xc1), bond: BOND_A, tickets: 120, spent: 87, first: 1000, last: 1400 },
  { claimId: h64(0xc2), bond: BOND_B, tickets: 120, spent: 120, first: 700, last: 900 },
];
const blocks = [];
for (let i = 0; i < 70; i++) {
  const ts = now - i * 1000, round = 1400 - i;
  const c = claims[i % 2 === 0 ? 0 : 1];
  const isRound = i % 3 !== 0;
  const hash = (0x1000 + i).toString(16).padStart(8, "0").repeat(16);
  const rb = isRound ? (c === claims[0] ? BOND_A : BOND_B) : null;
  const rnd = c === claims[0] ? 1000 + (i * 3) : 700 + i;
  blocks.push({ header: { hash, daaScore: 5000 - i, blueScore: 9000 - i, timestamp: ts, parentsByLevel: [[h64(1)]], powAlgoId: isRound ? 10 : (i % 4 === 1 ? 8 : 6), palwCommitment: isRound ? pxr(rnd, 0, rb) : (i % 4 === 1 ? "" : pav2(i % 5 === 0 ? FLOOR : CLASS_8K)) },
    transactions: [], verboseData: Object.assign({ hash, transactionIds: [], isChainBlock: !isRound && i % 6 !== 0, selectedParentHash: h64(1) },
      NEW ? (isRound ? { laneClass: i % 7 === 0 ? "RED" : "EXEC", exec: { round: rnd, permitIndex: 0, bond: rb.tx + ":" + rb.ix, verdict: i % 7 === 0 ? "refused" : "granted", refusal: i % 7 === 0 ? "permit_already_used" : null, claimId: c.claimId, classId: CLASS_8K, quantumIndex: 3 + i, quantumId: h64(9) } } : { laneClass: i % 6 === 0 ? "RED" : "BLUE" }) : {}) });
}
for (let i = 0; i < blocks.length; i++) { const nx = blocks.slice(i + 1).find((b) => b.header.powAlgoId !== 10); blocks[i].verboseData.selectedParentHash = nx ? nx.header.hash : h64(1); }   // a round block's selected parent is its anchor, never itself on the chain
const claimRow = (c) => ({ claimId: c.claimId, isFreePrompt: false, classId: CLASS_8K, executorBond: c.bond.tx + ":" + c.bond.ix, phase: "final", voidReason: "", phaseDaa: 4400, acceptedDaa: 4000, acceptedBlock: blocks[3].header.hash, boundDaa: 4020, seats: ["x:0", "y:0", "z:0"], deadlineDaa: null, openCourts: 0, execStage: "scheduled", execCredit: 900000, execSpan: 12, execTickets: c.tickets, execTicketsSpent: c.spent, execFirstRound: c.first, execLastRound: c.last, vestingStage: "maturing" });
const lane = { armed: true, open: true, scheduleSpanDaa: 120, maxPerMergeset: 400, stages: [], virtualDaa: 5000, round: 1400, span: 41, permitsPerRound: 1, permits: [{ index: 0, bond: "a:0", operatorId: h64(5), domain: CLASS_8K, used: false }], domains: [{ domain: CLASS_8K, credits: 123456, quotaPermille: 1000, parity: 0, bonds: 2 }], acceptedInSpan: 87, finalsSpan: 41, finals: 2, nextRoundPermits: 0 };
if (NEW) Object.assign(lane, { health: { acceptedTotal: 913, refusedTotal: 21, refusedSinceLastAccepted: 2, lastAccepted: { hash: blocks[1].header.hash, round: 1399, daaScore: 4999, timestampMs: now - 1000, bond: BOND_A.tx + ":0", claimId: claims[0].claimId, verdict: "granted" }, latest: { hash: blocks[0].header.hash, round: 1400, daaScore: 5000, timestampMs: now, bond: "", claimId: null, verdict: "refused" }, topRefusals: [{ reason: "permit_already_used", count: 12 }, { reason: "header_mergeset_rule", count: 9 }], stale: false, staleAfterMs: 600000, nowMs: now, ledgerSinceMs: now - 3600000 }, executionsTotal: 2, recentExecutions: claims.map((c) => ({ claimId: c.claimId, classId: CLASS_8K, executorBond: c.bond.tx + ":" + c.bond.ix, domain: CLASS_8K, stage: "scheduled", status: c.spent >= c.tickets ? "complete" : "running", credit: 1, span: 12, tickets: c.tickets, ticketsSpent: c.spent, firstRound: c.first, lastRound: c.last, finalDaa: 4400 })) });
const STALE = process.argv.includes("--stale");
if (STALE) for (const b of blocks) b.header.timestamp -= 3600000;
let tick = 0;
const handlers = {
  getInfo: () => ({ p2pId: "x", mempoolSize: 0, serverVersion: "mock", isUtxoIndexed: true, isSynced: true, hasNotifyCommand: true, hasMessageId: true }),
  getBlockDagInfo: () => ({ networkName: "misaka-testnet-12", blockCount: 9000, headerCount: 9000, tipHashes: [blocks[(++tick) % 2].header.hash], difficulty: 1, pastMedianTime: now, virtualParentHashes: [blocks[0].header.hash], pruningPointHash: h64(1), virtualDaaScore: 5000, sink: blocks[tick % 2].header.hash }),
  getSinkBlueScore: () => ({ blueScore: 9000 }), getConnectedPeerInfo: () => ({ peerInfo: [] }),
  getBlocks: () => ({ blockHashes: blocks.map((b) => b.header.hash), blocks: blocks.slice().reverse() }),
  getBlock: (p) => ({ block: blocks.find((b) => b.header.hash === p.hash) }),
  getPalwRoundLane: () => lane,
  getPalwClaims: (p) => ({ available: true, claims: claims.filter((c) => c.bond.tx + ":" + c.bond.ix === p.bond).map(claimRow) }),
  getPalwModel: () => ({ available: true, found: true, classId: CLASS_8K, modelName: "Qwen 2.5 1.5B @8k", nCtx: 8192, artifactRoot: h64(3), classStatus: "Active", registryState: "Active", readySeats: 7, requiredReadySeats: 7, inflightClaims: 3, admissionPermille: 1000, sharePermille: 500, certifiedFamily: "", fenceActive: true, reason: "" }),
  getPalwPanelSeats: () => ({ available: true, seats: [0, 1, 2].map((i) => ({ seatId: h64(i), bondOutpoint: h64(0xd0 + i) + ":0", classId: CLASS_8K, ready: i !== 2, eligible: true, assigned: i, hold: null })) }),
  getPalwPanelAssignments: (p) => ({ available: true, assignments: [{ claimId: p.claimId, classId: CLASS_8K, licensedState: "licensed", deadlineDaa: 4300, coverageMask: 3, fullSeat: h64(1), validReceiptSeats: 3, selectedPanelSeats: 3, seats: [] }] }),
  getPalwModelRegistry: () => ({ available: true, classes: [{ classId: CLASS_8K, state: "Active", readySeatsNow: 7 }] }),
  getSinkBlueScore2: () => ({}), subscribe: () => ({}),
};
const mime = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" };
const srv = http.createServer((req, res) => {
  const f = path.join(root, req.url.split("?")[0] === "/" ? "index.html" : req.url.split("?")[0]);
  if (fs.existsSync(f) && fs.statSync(f).isFile()) { res.writeHead(200, { "content-type": mime[path.extname(f)] || "application/octet-stream" }); return res.end(fs.readFileSync(f)); }
  res.writeHead(200, { "content-type": "text/plain" }); res.end("");   // stubs (sha3.min.js, logos, readability.css)
});
const frame = (s) => { const b = Buffer.from(s); const h = b.length < 126 ? Buffer.from([0x81, b.length]) : b.length < 65536 ? Buffer.from([0x81, 126, b.length >> 8, b.length & 255]) : Buffer.concat([Buffer.from([0x81, 127]), le(b.length, 8).reverse()]); return Buffer.concat([h, b]); };
srv.on("upgrade", (req, sock) => {
  const key = req.headers["sec-websocket-key"];
  sock.write("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: " + crypto.createHash("sha1").update(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest("base64") + "\r\n\r\n");
  let buf = Buffer.alloc(0);
  sock.on("data", (d) => {
    buf = Buffer.concat([buf, d]);
    for (;;) {
      if (buf.length < 2) return; let len = buf[1] & 127, off = 2;
      if (len === 126) { if (buf.length < 4) return; len = buf.readUInt16BE(2); off = 4; } else if (len === 127) { if (buf.length < 10) return; len = Number(buf.readBigUInt64BE(2)); off = 10; }
      const masked = buf[1] & 128; if (buf.length < off + (masked ? 4 : 0) + len) return;
      const mask = masked ? buf.subarray(off, off + 4) : null; if (masked) off += 4;
      const payload = Buffer.from(buf.subarray(off, off + len)); if (mask) for (let i = 0; i < len; i++) payload[i] ^= mask[i % 4];
      buf = buf.subarray(off + len); const op = buf.length >= 0 ? null : null;
      const opcode = d[0] & 15; if (opcode === 8) return sock.end(); if (opcode !== 1) continue;
      let m; try { m = JSON.parse(payload.toString()); } catch { continue; }
      const h = handlers[m.method] || (!/^getPalw/.test(m.method) ? () => ({}) : null);
      if (!h) { sock.destroy(); return; }   // an unknown op closes the socket, like an old node
      sock.write(frame(JSON.stringify({ id: m.id, method: m.method, params: h(m.params || {}) })));
    }
  });
});
srv.listen(port, () => console.log("explorer + mock node on http://localhost:" + port + (NEW ? " (new rpc)" : "") + (STALE ? " (stale)" : "")));
