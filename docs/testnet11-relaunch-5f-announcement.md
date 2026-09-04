# MISAKA testnet-11 — Relaunch 5f (2026-09-03)

*Draft for the operator to publish. Every sentence below is tied to a reading recorded in
`docs/testnet11-relaunch-5f-genesis-card.md`; the two lines marked `[PENDING]` are published only once their reading exists.*

## What happened

testnet-11 was relaunched at **2026-09-03 17:17 UTC** from a new genesis. Every earlier chain is retired.

    genesis      ad30b5cb965ad305dfa1dc7516935763ea2623105581b83bb9359c7247157d36b0f8003b337cdad366e3895c8f159e99332be16e258b144dddf483bf9b33edb7
    fingerprint  2222e054f87bed7a33e9c017f5403cd52070d0778776b5bd78143e7f82ff92b7   (network testnet-11)
    binary       kaspad from 6e01ba07 — sha256 14065c9348a958a111b4bc083e543ef47389e10931bf7f284c3ef64e6e2cd9c4 (53,913,240 B)

**If you run a node:** stop it, move your appdir aside, install the new `kaspad`, start. A node on the old genesis is
refused at handshake (`Genesis mismatch on network misaka-testnet-11`); nine community nodes were refused in the first
minute after the relaunch.

## What the chain carries at genesis

- The premine with 13 community allocations (648M) and eight bonded seats; bond and float outpoints are unchanged
  from the previous chain (`misaka-premine:0–7`, `:41–48`).
- Three classes with budget from epoch 0: the floor (BASE-0), the dense A16 tier (`4277d84f…`, Qwen2.5-1.5B graph-v5 @ 512)
  and Qwen3.6 (`5bd9ae3d…`). The court certified the four families end-to-end (`court_e2e_root e649e7c0…`).
- Qwen3.6's free-prompt lane is certified on chain — but its job shape holds **8 tokens in total**, which the chat
  template alone fills, so no practical prompt fits it today; practical free prompts go to the dense tier.
  The dense tier's free-prompt lane is **certified on chain** as of 2026-09-04 01:03 UTC (family `FamilyCertified`
  carried in block `f9af403c…`, class `ClassLaneCertified` in `5e265a6e…`): a free-prompt commitment on class
  `4277d84f…` now enters the state.

## What runs

Six fleet nodes on the new binary (ibm ×2, `.113` ×3, one on 5.104), every panel seat holding all three class
artifacts; the testnet-11 seeder; the explorer at misakascan indexing from block 0; the faucet (unfunded until the first
coinbase matures — it says so on its status page).

## Free prompts, 3D and MIDI

On the relaunch binaries, a free prompt to the dense tier's gateway was answered, committed (D5-bound), and derived to a
**3D artifact** (STL, `cad/stl/v1`, 684 B) and to a **MIDI file** (`music/smf/v1`, 91 B). Each derivation was verified
bound by `palw-derive verify`; the MIDI was recomputed byte for byte by an independent verifier that links none of the
node's code; the STL's bytes are identical across two builds on two CPU architectures.

The condition that comes with it: today a derivation succeeds when the answer fills the decode budget **exactly**
(56 tokens for that STL, 97 for that MIDI). The amendment that lets a shorter answer derive (ADR-0078 EOG cut) is
scheduled after this relaunch.

`[PENDING]` A free-prompt job committed **on** the public chain and its artifact derived from the chain's own record —
not claimed until it has been read from the chain.

## Adding a model

Registration is permissionless (ADR-0054). A class registered in the middle of an epoch has **no block budget until the
next epoch boundary** — "no blocks yet" after registering is arithmetic, not a refusal.

## What is not claimed

Block time, finality, node counts beyond the fleet's six, and anything about VLT credit.

## Qwen3.6 produced blocks

At 18:18 and 18:43 UTC the Qwen3.6 seat produced the chain's first two attempt blocks (`c90b028c…`, `7e11fa05…`), each
the result of one ~25-minute inference on an 8-core CPU host, both draws won. Because a block's parents are fixed when
its job starts, a 25-minute job lands a block whose parents are 25 minutes old, and the DAG merged both as **red**
blocks; the protocol counts merged work (ADR-0058), so they register against the class's budget and the seat's bond.
Their adjudication by the other seats cannot complete on this chain yet: a Qwen3.6 job's retained material is 253 MB and
a dense-tier job's 748 MB, and the peer transport that carries material to the verifying seats caps a message at
16 MiB, so seats answer "Unavailable" at the half-window. The transport fix (chunked or interval-based serving, or a
cap sized to the largest class) is scheduled after this relaunch; until it lands, LLM claims on the public chain are
produced and counted but not verified or finalized, and the dense tier's blocks do not yet reach the other nodes
(the same transport path). The floor and heartbeat lanes are unaffected. The dense tier's first block is expected when its first
512-context job completes; the floor's when its draw wins. Everything else on the chain so far is heartbeat blocks.
