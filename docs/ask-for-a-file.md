# Asking the network for a file — and checking a file someone hands you

You want a MIDI file, a 3D model, an STL you can print. This page is how you ask the network for
one, where it lands, and what a stranger can check about it afterwards without you sending them
anything but the answer.

Two halves, and they are for two different people:

* **[Part 1 — asking](#part-1--asking-for-a-file)** is for whoever runs the model.
* **[Part 2 — checking](#part-2--checking-a-file-someone-handed-you)** is for whoever was handed
  the result and wants to know it is what it claims to be. They need no model, no bond, and no
  permission from you.

Read [§0](#0-what-fits-today) before either. Whether the live path runs at all depends on which
classes the chain registers, and a page that let you find that out at the end would have wasted
your afternoon.

---

## 0. What fits today

**Two paths lead to a file, and only one of them depends on the chain.**

| | what runs | biggest thing it makes | on the chain? |
|---|---|---|---|
| **the offline path** ([§1](#1-the-offline-path-a-file-today-at-full-size)) | the transformer alone, over an answer you already have | the format's own ceiling — a 4 MiB MIDI description, a 64 KiB CAD description | no, unless you also have a claim to attach it to |
| **the live path** ([§2](#2-the-live-path-a-prompt-a-model-a-claim)) | a model on the network answers your prompt through the free-prompt gateway, and the answer is turned into the file | what fits in the class's width, **prompt and answer together** | yes: a claim, and a record of what was made from it |

The live path's limit is not a policy, a quota or a rate limit. It is the width (`n_ctx`) of the
class the gateway's worker serves, and a job is refused when `prompt + decode ceiling > n_ctx`.

**The numbers:**

| | tokens |
|---|---|
| the dense free-prompt worker's row (`Qwen/Qwen2.5-1.5B/graph-v5@512`) — **prompt and answer together** | **512** |
| the conversation wrapper the gateway sends before your first word | 8 |
| the shortest MIDI description that ships in this repository (one note) | 118 |
| the shortest CAD description that ships (one box) | 76 |
| the shortest 3D scene description that ships (one cube) | 255 |

`palw-derive width --tokenizer <tokenizer.json> --n-ctx <n> --prompt <text> --dsl <file>` answers
the question for your own prompt and description: whether the wrapped prompt plus the description
fits the width (exit 6 when it does not).

**On testnet-12, check the class before anything else.** `palw-a16-fp-worker` embodies the
`graph-v5@512` row, and the testnet-12 genesis registers the floor and the dense `graph-v7@8192`
and `graph-v7@2097152` rows, not that one
([testnet-12-regenesis-2026-09-23.md](testnet-12-regenesis-2026-09-23.md)). A gateway in front of
that worker answers, reports `registered: false` or `fp_certified: false` in `/health`, and commits
nothing — so it makes no claim to attach a file to. Ask the chain rather than trusting this
paragraph:

```bash
kaspad --testnet --netsuffix=12 --palw-dump-classes   # what this build registers
misaka --network testnet-12 palw registry              # every class's lifecycle state on the chain
misaka --network testnet-12 palw certified <class-id>  # is it on the free-prompt lane
```

So: if you want a file **today**, use the offline path in §1 — it is the same transformer code,
the same file formats, the same hashes, and it needs no chain. Use §2 where a network registers
and certifies a class your worker serves.

<sub>Where the numbers come from: the class's registered width is `n_ctx` in the class row, and a
job is refused when `prompt + decode ceiling > n_ctx`
(`misaka-palw-base0/src/fp_worker.rs`). The token counts are the shipped Qwen2.5 tokenizer over
`misaka-palw-derive/corpus/` (measured 2026-09-03).</sub>

---

## Part 1 — asking for a file

## 1. The offline path: a file today, at full size

The thing that turns an answer into a file is a **transformer**. It is a plain, deterministic
program: same input, same bytes out, on any machine. It does not need a chain, a bond, a model or
a network connection, and you can run it right now.

```bash
cargo build -p misaka-palw-derive --bin palw-evm-runner --bin palw-derive
./target/debug/palw-derive list
```

`list` prints what this build can make, and the ceilings each one enforces:

```
transformers:
  music/smf/v1  kind 6 (music)  grammar music/v1  discipline integer  writer standard-midi-file/1.0/canonical-v1  id <16 hex — this build's>
      bounds: dsl 4194304 B  artifact 16777216 B  work 65536 midi-note  named inputs 0 (0 B)
  cad/stl/v1  kind 3 (cad)  grammar cad/v1  discipline exact-rational  writer stl-binary/1.0/zero-normal-rh-winding-sorted-v1  id <16 hex — this build's>
      bounds: dsl 65536 B  artifact 1048576 B  work 4000000 exact-predicate  named inputs 0 (0 B)
  scene/glb/v1  kind 1 (scene)  grammar scene/v1  discipline integer  writer gltf-binary/2.0/canonical-v1  id <16 hex — this build's>
      bounds: dsl 262144 B  artifact 2097152 B  work 65536 mesh vertices  named inputs 0 (0 B)
```

**The ids are elided above on purpose, and the ones this page used to print were already
wrong.** A `transformer_id` is a hash of the transformer's SOURCE TREE — every file under
`misaka-palw-derive/src/`, comments included — so it moves whenever that code is edited, including
by an edit that changes no behaviour at all. The repository's own `transformer_id_pin` test exists
to record each move. A page that prints today's ids as though they were constants is a page that
is wrong after the next commit, and the reader who compares them concludes their build is broken.

**Take yours from `palw-derive list`, which is the only answer that cannot be stale**, and expect
it to differ from any id written down anywhere. What does NOT move with the source tree is
everything the artifact is made of: `dsl_hash`, `artifact_hash` and the artifact bytes are
functions of the DSL and the writer, so a 157-byte MIDI stays a 157-byte MIDI with the same
content hash across builds whose ids differ. If those disagree, something real disagrees.

Eight are registered in this build: music (`.mid`), CAD (`.stl`), 3D scenes (`.glb`), images
(`.png`), maps, simulations, and two that compile and run contract code.

The input is a **description** — structured JSON in the transformer's own vocabulary. That
description is what a model is meant to write for you; until a class is wide enough to write one,
you can write it yourself, or start from the worked examples in
`misaka-palw-derive/corpus/<kind>/`.

```bash
./target/debug/palw-derive derive \
  --transformer music/smf/v1 \
  --answer misaka-palw-derive/corpus/music/03-overlapping-melody.json \
  --out ./out
```

It prints one JSON line and writes three files:

```json
{"artifact_bytes":157,
 "artifact_hash":"6e27611c2ee15af0…",
 "dsl_hash":"b2600b65189e277d…",
 "derived_id":"<16 hex — this build's>",
 "kind":6,"kind_name":"music","transformer":"music/smf/v1",
 "files":{"artifact":"./out/derived-<16 hex>.artifact.mid",
          "dsl":"./out/derived-<16 hex>.dsl",
          "object":"./out/derived-<16 hex>.derived-unsigned.borsh"}}
```

And it is a real MIDI file — not a blob with the right extension:

```console
$ file ./out/derived-<16 hex>.artifact.mid
./out/derived-<16 hex>.artifact.mid: Standard MIDI data (format 1) using 2 tracks at 1/192
```

The same for a 3D scene: `file` reports `glTF binary model, version 2`, and the container holds up
to an independent parse — the declared length equals the file's, the `JSON` and `BIN` chunks are
both present, the single buffer's `byteLength` equals the `BIN` chunk, all three accessors fit
inside their buffer views, every index is inside the vertex count, the primitive is `TRIANGLES`,
and one node carrying the mesh is wired into scene 0. That is what a loader checks before it draws
anything.

<sub>Checked with a stand-alone parser (python3's stdlib over the GLB container and its JSON
chunk), not with our own code, because "our verifier says our file is fine" is not evidence about
the format. What is *not* claimed: nobody has opened these in a DAW or a 3D viewer. The MIDI was
parsed the same independent way — `MThd` format 1, two `MTrk` chunks, both ending in an
end-of-track meta event, consuming the file exactly to its last byte — and the STL is 12 triangles
at the exact 84 + 50 × n binary layout, with zero normals, which is what the writer's own name
(`stl-binary/1.0/zero-normal-rh-winding-sorted-v1`) says it emits.</sub>

### When it refuses

A transformer refuses rather than guessing, and the refusal names the wall it hit:

```console
$ palw-derive derive --transformer cad/stl/v1 --answer over-long.json --out ./out
[palw-derive] fatal: derivation refused: grammar: the answer is 70105 bytes; at most 65536 (ADR-0078 SA-2)

$ palw-derive derive --transformer scene/glb/v1 --answer inexact.json --out ./out
[palw-derive] fatal: derivation refused: inexact: 16777217/2^8 needs 25 significant bits; binary32 holds 24
```

The second one is worth understanding, because it is the whole reason this is checkable at all:
the CAD and scene writers do exact arithmetic and refuse a number they cannot place exactly.
Rounding here would be silent, and two machines rounding differently would produce two different
files from one description — which would make everything in Part 2 meaningless.

**These ceilings are part of the transformer's name.** `music/smf/v1` is not a label; it is a
short hash of a document — the format it writes, the arithmetic it does, and every limit above.
Raise a limit and you have a *different* transformer with a different id, and files made under the
old one stay checkable against the old one. `palw-derive manifest --transformer music/smf/v1`
prints that document.

---

## 2. The live path: a prompt, a model, a claim

This is the path where the network is involved. Setting it up is the miner's path — a key, some
test coins, a registered bond, a node, and a gateway in front of a model — written out in
[testnet12-join-mining.md](testnet12-join-mining.md) and
[palw-freeprompt-gateway.md](palw-freeprompt-gateway.md#from-a-committed-job-to-a-paid-block). Do
that first; this section is only the part that asks for a *file* instead of text.

Once your gateway is up, one extra field turns an answer into an artifact:

```bash
curl -s localhost:8790/v1/chat/completions -H 'content-type: application/json' \
  -d '{"messages":[{"role":"user","content":"one quiet note"}],
       "max_tokens":200,
       "derive":"music/smf/v1"}'
```

Size `max_tokens` for the description you want: the shortest MIDI description is 118 tokens, and
prompt plus `max_tokens` must fit the class's width ([§0](#0-what-fits-today)). Ask for more and
the worker refuses the whole job by name rather than trimming it.

What comes back, beside the ordinary chat reply:

| field | what it is |
|---|---|
| `misaka.derivation.dsl` | the canonical description the model produced |
| `misaka.derivation.artifact` | the file itself — `inline_base64` if it is small, otherwise a `url` you fetch from the same gateway |
| `misaka.derivation.dsl_hash`, `artifact_hash`, `artifact_bytes` | the three fingerprints the network will carry |
| `misaka.derivation.object_borsh_hex` | the small record that goes on the chain |
| `misaka.output_token_ids`, `misaka.job_context_hash`, `misaka.family` | what the answer *was*, for Part 2 |
| `misaka.fp_claim_id` | the claim id — the one thing a checker needs from you |
| `misaka.derivation.status` | `derived`, or `refused` with the reason. A refusal changes nothing about the claim |

The gateway also drops the same things in its outbox directory, so nothing depends on you having
kept the HTTP response:

```
<outbox>/fp-job-<id>.dsl
<outbox>/fp-job-<id>.artifact.mid
<outbox>/fp-job-<id>.derived-unsigned.borsh
<outbox>/fp-job-<id>.derived.json
<outbox>/artifacts/<derived-id>.mid          # what GET /v1/artifacts/<derived-id> serves
```

(A `.derived-object.borsh` appears beside those once the record is signed — either by the gateway,
if you gave it `--derive-seed`, or by the rail command below. That signed file is the one that
goes on the chain.)

Then two commands put the record on the chain — one to sign it with the same key the claim was
made under, one to carry it:

```bash
misaka-palw-fp-rail --derive-artifact <outbox>/fp-job-<id> --bond-key-seed ~/.misaka/miner.seed

misaka --network testnet-12 \
  palw submit-object --key-file ~/.misaka/miner.seed \
  --object <outbox>/fp-job-<id>.derived-object.borsh --yes
```

(Leave off `--yes` for a dry run that prints what it would send.)

### What goes on the chain, and what never does

**The file never rides.** Not whole, not in pieces, not compressed. What the chain stores is one
small record beside the claim:

* which claim it came from, and that claim's own fingerprint of the answer;
* which description grammar and which transformer were used, by their ids;
* the fingerprint of the description, the fingerprint of the file, and the file's length in bytes;
* the signature of the key that made the claim.

Measured on this build, that record is **3,056 bytes** unsigned and about **7.7 KB** signed —
almost all of it the post-quantum public key and signature. The MIDI file it describes was 157
bytes and could have been 16 MiB; neither number changes the record.

The chain accepts the record only if: the claim exists on this chain, the record's fingerprint of
the answer is the claim's own, the signer is the key that made the claim, this claim does not
already carry a record from this same transformer, and the claim has fewer than four records
already. It pays nothing for it, counts no work for it, and **never looks inside the file** — it
cannot run a MIDI writer and this network does not pretend it can. What the record buys is that
one specific story about where the file came from is now written somewhere you cannot quietly
revise.

The prompt and the answer's tokens are a separate matter. On the weight-bearing setting the
prompt is **public**: it is carried in the commitment transaction so that anyone can replay the
job. Do not point a private question at a gateway whose outbox feeds a chain. The answer's tokens
are on no chain at all, by design — the chain carries a fingerprint of them, and the tokens
themselves stay with whoever ran the job and whoever they gave them to.

### The four stages — a block does not follow a prompt

Your prompt does not produce a block. It produces a **claim**, and a claim walks four stages
before any of the work behind it can pay for anything. Anything that shows you a job on this lane
should show you which stage it is in, by these names:

| stage | what has happened | what it is called on the chain |
|---|---|---|
| **submitted** | the commitment transaction was accepted; the claim exists | `provisional` |
| **bound** | a panel of validators was drawn to check it | `panel_bound` |
| **certified** | the panel replayed the job, agreed, and licensed a receipt | `receipt_licensed`, then `final` |
| **spent** | the work behind it paid for a block | one of the claim's quanta is spent |

The windows are DAA counts and the walk takes days, not seconds. It is a challenge period, not a
progress bar someone forgot to speed up: the time is what gives anyone the room to dispute the job
before it is paid for. `misaka palw claim <claim-id>` prints the stage a claim is in and what can
still happen next.

Two things follow that matter to you:

* **Your file exists immediately.** It is in the response and in the outbox the moment the model
  finishes. The stages are about the *claim*, not about the file.
* **A claim can also fail.** It can time out waiting for a panel, time out waiting for receipts,
  lose in court, or be voided because its executor did not serve the material a validator asked
  for. A record attached to a claim that later fails is a record of a failed claim, and it says so
  when read — see the `phase` line in Part 2.

---

## Part 2 — checking a file someone handed you

Someone gives you a MIDI file and says "the network made this". Here is how you find out, holding
nothing but what they gave you.

**You need two things from them, and neither is the file:**

1. the **claim id** (their gateway called it `fp_claim_id`);
2. the **answer** — the gateway's JSON response, or at minimum its `misaka` block, which carries
   the answer's token ids, the job's context hash, the model family, and the canonical
   description.

You need no bond, no model, no key, and nothing from them beyond that. You do need a node on
the network the claim was made on, or the address of someone else's.

### The two commands

```bash
# 1. what the chain actually holds about this claim
misaka --network testnet-12 palw derived <claim-id>

# 2. re-run the derivation over the answer they gave you and compare
misaka --network testnet-12 \
  palw derived-verify <claim-id> --answer their-response.json
```

The first is a read: it prints the claim's own fingerprint of the answer, its phase, and each
record attached to it — which transformer, which description fingerprint, which file fingerprint,
what length, and where on the chain it was accepted.

The second does the real work. It takes the answer, runs the *same* grammar and the *same*
transformer named on the chain, and compares. It checks every field below and reports all of them
(`all_mismatches`), leading with the first in this order (`first_mismatch`) — because the first one
to disagree is the one that tells you *what kind* of wrong you are looking at:

| what disagrees | what it means |
|---|---|
| `output_root` | **the answer is not this claim's answer.** Those tokens, that context and that model do not produce the fingerprint the chain holds. They handed you someone else's answer, or an edited one |
| `dsl_hash` | the description on the chain is not the description that produced their answer |
| `artifact_hash` / `artifact_bytes` | the description is right, but re-running the transformer over it does not produce the file they claim it produced |
| `kind` | the record calls itself one kind of thing and its own transformer says another — a disagreement the chain itself cannot catch, because it never interprets a kind |
| `derived_id` | a check on **you**, not on them: the record you rebuilt is not the one the chain accepted, so you used the wrong network or the wrong executor key |

Exit 0 and `consistent`, or exit 1 with the first disagreeing field printed with both values. A
pass also states which of the three comparisons it was actually able to make, because "consistent"
over one of them is a smaller sentence than "consistent" over all three.

**Two answers that are not a pass and not a failure:**

* `UNVERIFIABLE` — the record names a transformer this build does not publish. That is not a
  verdict on the file; it is the tool refusing to say "fine" about something it cannot re-run.
  Get a build that publishes it.
* `claim …: not on this chain` — this chain does not hold that claim. An honest answer, not a
  connection problem. A malformed claim id *is* an error, so the two cannot be confused.

### Checking a file directly

If they gave you the description and the small record file as well, you can skip the chain
entirely:

```console
$ palw-derive verify --object their.derived-object.borsh --answer their.json --artifact their.mid
{"verdict":"consistent","dsl_hash_matches":true,"artifact_hash_matches":true,
 "artifact_bytes_matches":true,"artifact_file_matches":true,"kind_matches":true, …}
$ echo $?
0
```

Change one byte of the file and it says so:

```console
$ palw-derive verify --object their.derived-object.borsh --answer their.json --artifact tampered.mid
{"verdict":"MISMATCH — a demonstrable false object","artifact_file_matches":false, …}
$ echo $?
2
```

Add `--output-token-ids ids.json --job-context <file> --tokenizer tokenizer.json --family <the
response's misaka.family> --network testnet-12` and it also renders the answer from the claim's ids and
recomputes the claim's own fingerprint of the answer (`binding_checked: true`), which is the check
that ties the file to the inference rather than only to the description. Without all four of the
ids, the context, the tokenizer and the family it reports `binding_checked: false`. `--network`
matters: testnet-12 computes `output_root` by core's `CoreV1` rule and the retired networks by the
family's own rendering, so the tool refuses to guess (`misaka-palw-derive/src/bin/palw-derive.rs`).

### What a mismatch costs the person who made it

Nothing on the chain. No stake is lost and no penalty is applied, and this page will not tell you
otherwise: the network cannot run a MIDI writer, so it cannot referee a dispute about one. What a
mismatch costs is that it is **publicly demonstrable** — anyone holding the answer can reproduce
the disagreement, with the command above, and show that a named key signed a false story about
where a file came from.

That is also why your verification has to draw on two sources. The chain's side comes from the two
commands above. The answer's token ids, the job's context hash, the model family and the
description come from the *gateway's own response* — from the person making the claim. A checker
who took both halves from one source would be checking nothing.

---

## Where to go next

* [testnet12-join-mining.md](testnet12-join-mining.md) — the bond, the node, and running a
  producer at all.
* [palw-freeprompt-gateway.md § From a committed job to a paid block](palw-freeprompt-gateway.md#from-a-committed-job-to-a-paid-block)
  — the watcher that submits, following a claim, and where the reward comes from.
* [palw-freeprompt-gateway.md](palw-freeprompt-gateway.md) — the gateway's own options, the
  worker protocol, and what it refuses.
* [palw-derived-artifacts.md](palw-derived-artifacts.md) — the same subject at protocol depth:
  every transformer's manifest, the two chain reads, the cross-architecture drill.

<sub>Background: the derivation rules are ADR-0078, the claim lattice and the four stage names are
ADR-0077 Decision 9, and the exact-arithmetic refusal is ADR-0078's SA-2 bounds.</sub>
