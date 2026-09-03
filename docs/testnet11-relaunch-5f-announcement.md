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
- Qwen3.6's free-prompt lane is certified on chain. `[PENDING]` The dense tier's free-prompt lane binding
  (`ClassLaneCertified`) is built and is submitted as soon as the first coinbase matures.

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

Block time, finality, node counts beyond the fleet's six, and anything about VLT credit. The first hour's blocks are all
heartbeat blocks; the first attempt block from a model class is expected within hours at the previous chain's measured
rate and will be reported when read, not before.
