import test from 'node:test';
import assert from 'node:assert/strict';
import { makeRepository, validateRepository, validStoredRepository, workspaceOwner, exportRepository, LocalRepositoryProvider } from './repository-workspace.js';
import { avatarSvg } from './avatar.js';
import { wrapAccountWorkspace } from './me.js';
const input = {name:'my-model', description:'Small model', visibility:'public', readme:true, gitignore:'Rust', license:'MIT'};
test('names and descriptions are bounded; private is refused', () => {
  for (const name of ['../secret','.git','bad/name','x.git','x'.repeat(101),'a b']) assert.throws(() => validateRepository({...input,name}));
  assert.throws(() => validateRepository({...input,description:'😀'.repeat(351)}));
  assert.equal(validateRepository({...input,description:'😀'.repeat(350)}).description.length,700);
  assert.throws(() => validateRepository({...input,visibility:'private'}));
  assert.throws(() => validateRepository({...input,gitignore:'__proto__'}));
  assert.throws(() => validateRepository({...input,license:'unlicensed-template'}));
});
test('initialize real files with content hashes; exported draft never claims publication', async () => {
  const record=await makeRepository(input,'device:local','2026-10-06T00:00:00.000Z');
  assert.deepEqual(record.files.map(f=>f.path),['README.md','.gitignore','LICENSE']);
  assert.match(record.files[0].content,/# my-model/);
  assert.match(record.files[1].content,/target\//);
  assert.match(record.files[2].content,/Copyright \(c\) 2026 \[copyright holder\]/);
  assert.equal(record.files[0].sha256,Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(record.files[0].content))),b=>b.toString(16).padStart(2,'0')).join(''));
  const exported=JSON.parse(exportRepository(record));
  assert.equal(exported.schema,'misaka/repository-draft/v1');
  assert.deepEqual(exported.publication,{signed:false,seeded:false,chainIdentityVerified:false});
  assert.equal(validStoredRepository(record,'device:local'),true);
  assert.equal(validStoredRepository(record,'wallet:0x'+'a'.repeat(40)),false);
  assert.equal(validStoredRepository({...record,name:'../bad'},'device:local'),false);
  assert.equal(validStoredRepository({...record,files:[{path:'../key',content:'secret',sha256:'a'.repeat(64)}]},'device:local'),false);
});
test('owner scope is explicit, case-insensitive collision key; no files means no fake README', async () => {
  const owner=workspaceOwner('0x'+'A'.repeat(40));
  assert.equal(owner,'wallet:0x'+'a'.repeat(40));
  assert.equal(workspaceOwner(null),'device:local');
  const a=await makeRepository({...input,name:'MODEL',readme:false,gitignore:'None',license:'None'},owner);
  const b=await makeRepository({...input,name:'model'},owner);
  assert.equal(a.key,b.key); assert.equal(a.files.length,0);
  await assert.rejects(makeRepository(input,'verified-user:misaka'));
});
test('missing persistent storage fails rather than reporting a saved repository', async () => {
  await assert.rejects(new LocalRepositoryProvider().db(),/Browser storage is unavailable/);
});
test('auto-generated avatar is deterministic and does not interpolate hostile markup', () => {
  assert.equal(avatarSvg('0xAbCd'),avatarSvg('0xabcd'));
  assert.notEqual(avatarSvg('alice'),avatarSvg('bob'));
  assert.notEqual(avatarSvg('alice',{network:'testnet-12'}),avatarSvg('alice',{network:'testnet-13'}));
  assert.notEqual(avatarSvg('alice',{namespace:'evm'}),avatarSvg('alice',{namespace:'model-line'}));
  const icon=avatarSvg('alice');
  assert.equal((icon.match(/<path /g)||[]).length,1);
  assert.equal((icon.match(/<rect /g)||[]).length,1);
  assert.ok(icon.length<900);
  const cells=[...icon.matchAll(/M(\d) (\d)h1v1h-1z/g)].map(m=>[Number(m[1]),Number(m[2])]);
  for (const [x,y] of cells) assert.ok(cells.some(([otherX,otherY])=>otherX===6-x && otherY===y));
  assert.doesNotMatch(avatarSvg('<script>alert(1)</script>'),/<script/);
  const html=wrapAccountWorkspace({account:'0x'+'a'.repeat(40),content:''});
  assert.match(html,/href="#\/me\/new"/); assert.match(html,/viewBox="0 0 7 7"/);
});
