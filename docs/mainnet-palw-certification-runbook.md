# Mainnet: certification objects — deploy and verify (ADR-0075 §7)

This runbook covers the mainnet side of ADR-0075: the build, the card, the fleet swap, and how to
verify on the live chain that a family certification and a class binding are accepted. It assumes
ADR-0042's discipline — mainnet is born from the final RC's git tag, changing only network-scoped
constants — and ADR-0036's — mainnet PALW is a new network identity, not a fork of the hash-only
chain.

## 0. What has to be true before anything is deployed

| Check | How |
|---|---|
| The RC (testnet-11 5e) runs this ruleset and has accepted at least one `FamilyCertified` and one `ClassLaneCertified` on the live chain | `[palw-lifecycle]` / "PALW lifecycle carried 1× FamilyCertified" in the RC producers' logs |
| The mainnet card is pinned: `PALW_MAINNET_GENESIS_BONDS` (≥ `palw_v2_min_genesis_bonds_v1`, ≥ the maturity-armable count if ADR-0065 D1 is armed), `PALW_MAINNET_GENESIS_ARTIFACT_ROOT`, `PALW_MAINNET_QWEN36_ARTIFACT_ROOT`, `PALW_MAINNET_QWEN25_A16_ARTIFACT_ROOT` | `cargo test -p kaspa-consensus-core --lib -- shipped_presets_have_pinned_fingerprints mainnet` after pinning; the mainnet fingerprint in that test is re-pinned from the output and recorded in the release notes |
| **`palw_uncertified_weightless` is armed in the genesis being minted (ADR-0069 Decision 7).** This is the ONLY moment the option exists: `validate_palw_v2` refuses the fence above `genesis.daa_score` and the node will not start, and the standing doctrine forbids a mainnet re-genesis — so a card set without it yields a mainnet that can NEVER carry Decision 7, where an uncertified family's fabricated block outweighs the honest network permanently, with no path to a fix. Absence is the default and nothing fails, so this is the field an assembler copying the RC's shape will not notice is missing. | assemble the card, then assert the field is `Some(ForkActivation::always())` on the params the binary actually builds — read it off `mainnet_shipped_params()`, not off the source |
| Every operator key on the card was generated on the host that holds it | operators attest; a fabricated key is a permanently silent seat |
| The genesis free-prompt set is the drilled one | `cargo test -p misaka-palw-base0 --lib -- the_rc_free_prompt_set_is_the_one_this_build_drilled the_rc_free_prompt_classes_are_the_covered_ones` |
| The whole route works on a chain with this ruleset | §2 below, on devnet, with the release binaries |

## 1. Build

```bash
git checkout <final RC tag>
cargo build --release -p kaspad -p misaka-cli -p misaka-palw-base0 --bins
./target/release/kaspad --version
sha256sum target/release/kaspad target/release/misaka target/release/palw-certify \
          target/release/palw-a16-fp-worker target/release/palw-qwen36-fp-worker
```

Record the four sha256 values and the fingerprint the throwaway boot announces
(`kaspad --mainnet --appdir /tmp/fp-probe` prints it; stop it before it syncs). The fingerprint
covers the state version, the genesis free-prompt set and every consensus constant in this ADR.

## 2. Rehearse the route on devnet with the release binaries

Devnet carries the same ruleset and, since ADR-0075 §7, a genesis bond registry derived from
PUBLIC seeds (`palw_devnet_genesis_bond_seed_v1(n)` = blake2b-256 of
`misaka-devnet-genesis-bond-v1/<n>`, six seats), so a bonded, producing, multi-validator devnet
runs from genesis on any machine — no card of real keys. The fee wallet is devnet's regenerable
main key (blake2b-256 of `misaka-testnet-premine-9b-claude-managed`). The whole rehearsal is one
script:

```bash
cargo build --release -p kaspad -p misaka-cli -p misaka-palw-base0 --bins
NODES=3 scripts/misaka-palw-certify-devnet-e2e.sh
```

It starts N validators from the one build in one window (each producing under bond n with the
fixture PoW, node-0 listening, the rest connected to it), waits for blocks, submits a
`FamilyCertified` (the floor's free-prompt drill), a `ClassLaneCertified` (the floor bound to it)
and a burst of three more family drills for the per-block cap, then reads every validator's log:
each must carry the same objects (`PALW lifecycle carried 1× FamilyCertified`, `1×
ClassLaneCertified`), and a dropped object prints `a PALW lifecycle object was dropped, and the
block stands: <reason>` — the reason is the transition's (`CertificationRefused`,
`FamilyAlreadyCertified`, `NoCertifiedFamilyCovers`, …) and must be understood before deploying.
By hand, the same three steps are:

```bash
MISAKA_PALW_POW_FIXTURE=1 kaspad --devnet --appdir <dir> --listen=127.0.0.1:16310 --rpclisten-borsh=127.0.0.1:17610 \
   --utxoindex --nodnsseed --palw-produce --palw-producer-key <bond-0.seed> \
   --palw-producer-bond 6d6973616b612d7072656d696e6500…00:0 --palw-producer-pay-address <address of bond-0.seed>
palw-certify drill --family base0 --lane fp --out base0-fp.obj
misaka --network devnet --rpc 127.0.0.1:17610 palw submit-object --key-file <main.seed> --object base0-fp.obj --yes
palw-certify bind --model-id "PALW-BASE-0/rc" --lane fp --out base0-bind.obj
misaka --network devnet --rpc 127.0.0.1:17610 palw submit-object --key-file <main.seed> --object base0-bind.obj --yes
```

## 3. The fleet swap — every validator, one build, one window

The certification objects are consensus: a validator on the previous build treats a block
carrying one as carrying an undecodable lifecycle payload (dropped, block stands) and computes a
different PALW state root from then on. So the swap is simultaneous, not rolling:

1. Announce the window and the sha256 values to every operator; nobody starts early.
2. Stop every validator (the same stop-all discipline as a re-genesis: a peer left running
   re-feeds the old chain).
3. Wipe the datadir if the fingerprint moved (it does for this ADR); a node that keeps an old
   datadir refuses at boot with the reason.
4. Start the seeders, then the producers in class order (floor first), then the seats.
5. Each operator confirms the announced fingerprint in their own log before the window closes.

## 4. Verify on the live chain

1. **Fingerprint:** every validator's `announcing fingerprint` line equals the recorded one.
2. **First family:** an operator posts the floor's free-prompt drill (`palw-certify drill --family
   base0 --lane fp`), submits it, and every validator logs `PALW lifecycle carried 1×
   FamilyCertified` for the same block hash. Any validator that logs a drop instead is on another
   build — stop it.
3. **First binding:** `palw-certify bind --model-id "PALW-BASE-0/rc" --lane fp`; the same check.
   (On a carded mainnet the floor is already in the genesis free-prompt set, so the binding is
   refused as `ClassLaneAlreadyCertified`? No: the genesis set is a parameter, the chain set is
   state; the binding is accepted and recorded — a harmless duplicate that proves the route.)
4. **A model tier:** the QWEN36 attempt-lane drill and a `ClassLaneCertified` for a weightless
   entrant, exactly as `docs/palw-certify-a-new-model.md` describes; confirm the entrant's share in
   `getPalwProducerFacts` on two validators.
5. **The cap:** three `FamilyCertified` objects in one block — the third is logged as dropped for
   `PALW_CERTIFICATION_MAX_PER_BLOCK` on every validator, and the block stands.

## 5. If it goes wrong

* A validator that computes a different PALW state root is on a different build: stop it, wipe,
  restart on the recorded sha256.
* A family that should not have certified is a court defect: freeze the affected classes
  (`ClassFrozen`) on the live chain, and treat the fix as a ruleset change — new identity, full
  re-certification (ADR-0075 Decision 10).
