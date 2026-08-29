# MISAKA Studio — PALW mining demonstration (2026-08-29)

MISAKA Studio's Network tab claims that a machine running it can join the MISAKA network:
observe a node, verify by running one, and **mine** by producing PALW blocks under a bonded
key. Claims in a README are cheap. This document records the run that backed them — what was
demonstrated, on what chain, with the exact commands, and precisely what was *not*
demonstrated and why.

**Summary of what happened:** a `ConsensusV2` chain was minted with the same ceremony that
launched public testnet-11, a bonded producer was configured and launched **through the
Studio's UI/API**, and it produced hundreds of real PALW-BASE-0 blocks (block 418 sampled at
shutdown, difficulty retargeting from 550 to ~197,000 as the DAA window filled) that a second,
independently-bonded node accepted — while that second node's panel service **verified the
producer's claims and filed 270 "Valid" receipts**, which is verifier participation in the
strong, bonded sense. The Studio's class list showed the chain's own `--palw-dump-classes`
rows against its catalog: the QWEN36 class id matched byte for byte.

---

## 1. Why a locally minted chain and not the public testnet-11

The environment this was built in has no P2P route to the public network
(`169.58.232.113:26311` and the other published peers are unreachable from the sandbox —
measured, not assumed; DNS resolves, TCP does not connect). A demonstration that cannot reach
the public chain has two honest options: fake it, or **run the real software on a chain minted
the same way the public one was**. This demo does the second:

* The chain is the **testnet-11 preset** — the only preset whose `palw_consensus_mode` is
  `ConsensusV2`, i.e. blocks are won by verified LLM inference, not hash-grinding.
* The genesis bond card was minted with `palw-rc-genesis`, the same tool and the same §1
  procedure as `docs/palw-rc-testnet11-launch-runbook.md` — six ML-DSA-87 bonds, each backed
  by a premine vault, assembled and **ACCEPTED** by the same gates the genesis loader runs.
* The two local edits this requires are the ones the runbook itself names: paste the emitted
  card into `consensus/core/src/config/params.rs`, then re-pin
  `PALW_RC_GENESIS.{utxo_commitment, hash}` with the `repin::print` ceremony test (the M-07
  guard refuses to boot until you do — the demo hit that guard and was corrected by it, which
  is the guard working). Neither edit is committed; both files were restored after the build.

Everything else — the binary, the producer, the panel, the consensus rules, the Studio — is
exactly what ships.

## 2. Minting the demo chain

```bash
# 6 bond keys + 6 operator keys (bond i is seat identity i; its address is also the pay address)
for i in 0 1 2 3 4 5; do
  ./target/release/misaka --network testnet-11 key gen --out keys/bond$i.seed
  ./target/release/misaka --network testnet-11 key gen --out keys/op$i.seed
done

# One genesis row per bond, then the assembled card
for i in 0 1 2 3 4 5; do
  ./target/release/palw-rc-genesis --emit-row --bond-index $i \
      --bond-seed keys/bond$i.seed --operator-seed keys/op$i.seed >> rows.txt
done
./target/release/palw-rc-genesis --rows rows.txt > card.out
# → "ACCEPTED — every gate the genesis loader runs has passed", bonds at premine #[0..5]

# Paste card.out's block into consensus/core/src/config/params.rs, then re-pin:
cargo test -p kaspa-consensus --lib repin::print -- --ignored --nocapture
# → REPIN utxo_commitment + REPIN hash → into PALW_RC_GENESIS in config/genesis.rs
cargo build --release -p kaspad     # the demo binary; then restore both source files
```

A genesis bond's outpoint is `premine_outpoint(i)` — the fixed premine txid
(`"misaka-premine"` ASCII, zero-padded to 64 bytes, hex-encoded) at index *i*. That is the
`--palw-producer-bond` value; nothing needs to be registered at runtime because the card
already seats it.

## 3. The verifier (panel seat)

A second node, bonded as seat 1, launched manually — deliberately *not* through the Studio,
so the two participants are independent processes with independent keys:

```bash
kaspad --testnet --netsuffix=11 --appdir=n1 --nodnsseed --disable-upnp \
  --listen=127.0.0.1:26412 --rpclisten=127.0.0.1:36411 --addpeer=127.0.0.1:26411 \
  --palw-panel --palw-producer-key=keys/bond1.seed \
  --palw-producer-bond=6d6973616b612d7072656d696e6500…00:1 --utxoindex
```

Its log at startup:

```
[palw-panel] starting (bond=(6d6973616b612d7072656d696e6500…00, 1), submitter=off — receipts only, register=off)
```

## 4. The producer, through the Studio

The Studio's node settings (Network tab → Node configuration, or `PUT /api/v1/settings`)
were set to role **producer**, network **testnet-11**, with bond 0's seed, pay address, and
outpoint, plus the isolation flags in *Extra arguments*. Then one call:

```
POST /api/v1/network/node/start
```

The Studio launched and supervised the node, and — by design — reported the **exact command
line** it ran, reproducible without the Studio:

```
kaspad --testnet --netsuffix=11 --appdir=n0 --rpclisten-json=127.0.0.1:28210 --utxoindex
  --palw-dump-classes --palw-produce --palw-panel
  --palw-producer-key=keys/bond0.seed
  --palw-producer-pay-address=misakatest:q2qkey7gs5tcwzk9pvaphq02470s2mla0h4xhxvvfgakuw7grw3eu83428xjul2t2gfuuzl03vdqnsdxkszhgxa7zkqhyfyhjshxsy7u43z8vye4
  --palw-producer-bond=6d6973616b612d7072656d696e6500…00:0
  --nodnsseed --disable-upnp --listen=127.0.0.1:26411 --rpclisten=127.0.0.1:36410
  --enable-unsynced-mining
```

(`--enable-unsynced-mining` waives only the stale-sink hold — a freshly minted chain's tip
*is* fifteen months behind wall clock. The peer requirement is not waivable: a lone node
never produces, which is why the seat exists before the producer.)

## 5. What the chain did

Within seconds of the seat connecting, the producer began winning blocks — each one a real
BASE-0 inference anchored on the template's pre-pow hash, nonce-ground, ML-DSA-87-signed, and
validated by the peer that accepted it:

```
[palw-producer] produced block #4 3458d1e1a8bd0d0a…    [palw-producer] palw weight=0 live_total=4623 final_claims=0 unresolved=3
[palw-producer] produced block #330 b8b94929662e4a1f…  (difficulty by then ~1.2k and climbing)
```

Sampled through the Studio's own `/api/v1/network` a few minutes in:

```
blocks 418   daa 418   peers 1   synced true   difficulty 196721
```

The seat, meanwhile (`n1.log`):

```
Accepted 2 blocks …via relay
[palw-panel] filed a "Valid" receipt for claim 66c8f343041a4952…   × 270
```

And the node's one-shot class dump, parsed and displayed by the Studio's Network tab:

```
[palw-dump] 3 class(es) at daa 16
[palw-dump]   class=ec7bbcbffe13f36f…3f02fb3f base=false status=Active share=200  ← QWEN36 (id matches the catalog byte-for-byte)
[palw-dump]   class=f1c5635c6e47e96e…42f623c8 base=true  status=Active share=600  ← the floor (chain-specific id; the UI matches it by its base flag)
[palw-dump]   class=f942e268f43f0546…aa7902c1 base=false status=Active share=200  ← QWEN25-A16
```

Shutdown was through the Studio (`POST /api/v1/network/node/stop`, SIGTERM first): clean
RocksDB close, `{"stopped": true}`.

## 6. What this demonstrates — and what it does not

Demonstrated, with the shipped software:

* **Mining participation through the Studio**: configure a bonded producer in the UI, start
  it, watch `[palw-producer] produced block #N` in the activity feed, blocks accepted by an
  independent node, rewards addressed to the configured ML-DSA-87 pay address.
* **Verifier participation** at both rungs: a full node accepting and validating relayed
  PALW blocks (on this chain, syncing *is* verifying), and a **bonded panel seat** filing
  `Valid` receipts against the producer's claims.
* **The class UX**: the chain's own class table (QWEN36 et al.) rendered with share,
  readiness, artifact provenance, and the machine's honest fit verdict (a 34 GiB artifact
  against 15.7 GiB of RAM says so, in amber).

Not demonstrated, stated plainly:

* **Joining the public testnet-11** — no P2P egress from this environment. The Studio's
  producer flags are the runbook's flags unchanged, and nothing in the flow is
  local-chain-specific except the minted card, but the live join itself remains unexercised
  from here.
* **The claims → quorum → licensing → weight pipeline.** Receipts were filed, but quorum
  needs 5 live seats and licensing needs ~40 hours of chain time (`w_challenge`); every
  produced block's `weight=0 … unresolved=N` line above is that fact being reported
  honestly. Nothing in this demo shortcut it.
* **Model-class production** (QWEN36 / QWEN25-A16): needs the converted artifact and the
  memory to hold it, which this machine does not have. The floor class the demo mined under
  is the class the network's design designates for exactly this situation (600‰ share,
  epoch-budget-exempt).
