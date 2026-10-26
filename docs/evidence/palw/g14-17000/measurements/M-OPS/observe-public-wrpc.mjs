#!/usr/bin/env node
// Read-only public t12 observation. No local node, wallet, key, or write RPC.
import { writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, realpathSync } from 'node:fs';

const url = 'wss://wallet.misakascan.com/t12/wrpc/json';
const out = process.argv[2];
if (!out) throw new Error('usage: node observe-public-wrpc.mjs OUTPUT.json');
const startedAt = new Date().toISOString();
const binary = realpathSync(process.execPath);
const binarySha256 = createHash('sha256').update(readFileSync(binary)).digest('hex');
const shell = (command, args = []) => execFileSync(command, args, { encoding: 'utf8' }).trim();
const environment = {
  host: shell('hostname'),
  hardware: shell('sysctl', ['hw.model', 'hw.memsize', 'hw.ncpu']),
  sourceCommit: shell('git', ['rev-parse', 'HEAD']),
  sourceDirty: shell('git', ['status', '--short']),
  nodeVersion: process.version,
  binary,
  binarySha256,
  command: process.argv,
  uptimeBefore: shell('uptime'),
};
const requests = [
  { id: 1, method: 'getBlockDagInfo', params: {} },
];
const observations = [];
let timer;
let completed = false;
let ws;

function finish(exitCode, error = null) {
  if (completed) return;
  completed = true;
  clearTimeout(timer);
  const result = { source: url, startedAt, finishedAt: new Date().toISOString(), environment: { ...environment, uptimeAfter: shell('uptime') }, requests, observations, error };
  writeFileSync(out, `${JSON.stringify(result, null, 2)}\n`);
  if (ws && ws.readyState < WebSocket.CLOSING) ws.close();
  process.exitCode = exitCode;
}

ws = new WebSocket(url);
timer = setTimeout(() => finish(2, 'timeout'), 10000);
ws.addEventListener('open', () => ws.send(JSON.stringify(requests[0])));
ws.addEventListener('message', event => {
  try {
    const value = JSON.parse(String(event.data));
    observations.push({ receivedAt: new Date().toISOString(), response: value });
    if (value.id === 1 && value.method === 'getBlockDagInfo') {
      if (value.params?.network !== 'testnet-12') throw new Error('network identity mismatch');
      if (!value.params.sink) throw new Error('missing sink hash');
      const request = { id: 2, method: 'getBlock', params: { hash: value.params.sink, includeTransactions: true } };
      requests.push(request);
      ws.send(JSON.stringify(request));
    } else if (value.id === 2 && value.method === 'getBlock') {
      if (!value.params?.block?.header || !Array.isArray(value.params.block.transactions)) throw new Error('missing sink block');
      finish(0);
    } else {
      throw new Error('unexpected response');
    }
  } catch (error) {
    finish(1, String(error));
  }
});
ws.addEventListener('error', () => {
  if (!completed) finish(1, 'websocket error before response');
});
ws.addEventListener('close', () => {
  if (!completed) finish(1, 'websocket closed before response');
});
