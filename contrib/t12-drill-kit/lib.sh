#!/usr/bin/env bash
# drill-t12/lib.sh — configuration and helpers for the testnet-12 pre-publication drill.
# Sourced by drill.sh. Runs ON the drill host (default 5.104.81.23, as root). Nothing here touches a
# systemd unit, a public node, a port outside 28601–28693, /root/perm-drill, or root's own ~/.misaka.
#
# THE BUILD: 005e5c5f = c8652a97 (the t12 launch candidate: the panel-room review, the licence-stall fix
# and the operator's 100M community row — public genesis a27f8f44…) + ONE drill-only commit that salts
# the chain's identity (premine salt …/DRILL-PRE-c8652a97 → premine txid 269a354e…, genesis 32a665d6…)
# and gives the drill chain a spendable main wallet: TESTNET_MAIN_ADDRESS, the key regenerable from the
# PUBLIC test seed `tests::TESTNET_MAIN_SEED` (premine.rs), in place of the operator's
# PALW_PUBLIC_MAIN_ADDRESS. It is NOT the shipping binary: the drill is evidence about c8652a97's rules on
# a chain whose outpoints and genesis no public testnet-12 node shares. Never ship bin/ from this directory.
#
# One chain, seven persistent nodes (genesis bonds 0-6), two transient slots (7: IBD follower,
# 8: bond registrar). Every node is 127.0.0.1-only and --nodnsseed.
#
# TOPOLOGY (review 2, finding "--connect makes a node outbound-only"): at 005e5c5f a node given ANY
# --connect runs with inbound_limit = 0 (kaspad/src/daemon.rs:1046), which selects
# Adaptor::client_only (protocol/flows/src/service.rs:68) — it never opens its P2P listener. So:
#   * the two HUBS (n0, n3) take NO --connect: they listen on 127.0.0.1 with --outpeers=0 (no
#     discovery dialing), and n0 reaches n3 with --addpeer (a permanent connection request, dialed
#     whatever --outpeers says: components/connectionmanager/src/lib.rs handle_connection_requests);
#   * every LEAF (n1 n4 n5 n7 n8 → n0; n2 n6 → n3) keeps --connect to its hub: client-only, it dials
#     exactly one peer and nobody can dial it.
# Loopback addresses never enter the address manager (components/addressmanager/src/lib.rs:271), so
# the dial graph is exactly the edges named here. The partition is n0 restarted without its --addpeer:
# no side-B node dials side A, and no side-A leaf can be dialed. A node never gets both flags
# (daemon.rs:127 refuses --connect with --addpeer).

set -u
# ---- a clean environment ------------------------------------------------------------------------
# kaspad reads ~100 KASPAD_* variables (args.rs .env(...)) — KASPAD_ADDPEERS, KASPAD_CONFIGFILE,
# KASPAD_PALW_PRODUCE/_PRODUCER_PAY_ADDRESS, KASPAD_PALW_DRILL_* … — and both binaries read MISAKA_*.
# A stray export in the operator's shell could dial a public host or put a non-drill script in a
# coinbase. Every one is dropped here, and kaspad is additionally started under `env -i`.
for _v in $({ compgen -e 2>/dev/null || env | cut -d= -f1; } | grep -E '^(KASPAD_|MISAKA_)[A-Za-z0-9_]*$' | sort -u); do unset "$_v"; done
unset _v
export MISAKA_NETWORK=testnet-12

# ---- where things are ------------------------------------------------------------------------
DRILL_ROOT="${DRILL_ROOT:-/root/drill-t12}"
BIN_DIR="$DRILL_ROOT/bin"                         # written by build-ship.sh; NOT overridable (item 5)
SRC_DIR="$DRILL_ROOT/src"                         # the checkout build-ship.sh built
KASPAD="$BIN_DIR/kaspad"
CLI="$BIN_DIR/misaka"                             # the drill build's own CLI: it signs over params.genesis.hash
PALW_CLASS="$BIN_DIR/palw-class"
RUN="$DRILL_ROOT/run"                             # appdirs, logs, pids
EV="$DRILL_ROOT/ev"                               # evidence, one directory per checklist item
KEYS="/root/t12-private/keys"                     # card keys t12-bond-N.key, N = 0..7 — READ ONLY, never copied, never printed
FRESH="$DRILL_ROOT/keys"                          # drill-only wallets `drill.sh keygen` creates (item 6)
# The 8k artifact is placed here by the operator (PLAN step A2: scp from the Mac). The kit never reads
# /root/perm-drill (HARD RULE) — `art_path_ok` refuses any ART8K that resolves under it.
ART8K="${ART8K:-$DRILL_ROOT/art/qwen25-1.5b-a16-8k.palwart}"
ART8K_BYTES=1799359436
ART8K_SHA256="${ART8K_SHA256:-b73600cfeef3f54fd6e9f6a831c504588aa3b20ea2506ae824799206201e8ac8}"          # the Mac copy, deploy-t12/.cache/art
ART8K_MANIFEST_SHA256="${ART8K_MANIFEST_SHA256:-c3cc66bc140a7fcbc949500f6ddcb6641a4d63f411c2f943e65750cd3875df4a}"
FORBIDDEN_TREE="/root/perm-drill"
CHAINSTATE="python3 $DRILL_ROOT/chainstate.py"

# ---- HOME: the drill's own (review 2, finding "endpoints.json") -------------------------------
# kaspad writes its loopback RPC endpoints to ~/.misaka/<network-id>/endpoints.json on every start
# (kaspad/src/daemon.rs:2032-2049, misaka-endpoints/src/lib.rs:60, dirs::home_dir = $HOME), and every
# `misaka --network testnet-12` run without --rpc reads it. /root/.misaka/testnet-12/endpoints.json is
# the live t12 node's. The drill runs kaspad AND the CLI with HOME=$DRILL_ROOT/home, so its registry
# (and any ~/.misaka/config.toml / mining.toml the CLI would read) lives under the drill.
PUBLIC_HOME="$(getent passwd "$(id -un)" 2>/dev/null | cut -d: -f6)"; PUBLIC_HOME="${PUBLIC_HOME:-/root}"
PUBLIC_ENDPOINTS="$PUBLIC_HOME/.misaka/testnet-12/endpoints.json"
DRILL_HOME="$DRILL_ROOT/home"
mkdir -p "$DRILL_HOME" 2>/dev/null && chmod 700 "$DRILL_HOME" 2>/dev/null
export HOME="$DRILL_HOME"

# ---- the build and its identity (asserted by preflight and C1) ---------------------------------
SHIP_REV_EXPECTED="005e5c5f2d42131dcdeaaa633312aafa34f84bdd"
SHIP_REV="${SHIP_REV:-$SHIP_REV_EXPECTED}"
DRILL_SALT="misaka-palw-t12/premine/v2/2026-09-24/DRILL-PRE-c8652a97"
DRILL_GENESIS="32a665d622baba4cf97741da2fb68896463d668f794867d2ebd9db49a92634dc381b4250b3d734b251a2d161f3238820b4c3877807621e557fdf17cba994705f"
# genesis hashes that must NEVER appear on the drill chain (full hashes, read from each commit's genesis.rs):
#   a27f8f44… public testnet-12 as c8652a97 launches it (the 100M community row, 5bf72b46)
#   d73dbf44… public t12 before the 100M row (e93be0f2)       f6cc9576… its retired predecessor (b791b460)
#   fb1074b0… the collateral-unit regenesis (2bd134ec)        a8cabac4… the 09-23 genesis
#   1eaa6c0f… the FIRST pre-drill's chain (71c0399e, salt …/DRILL-PRE-e93be0f2)
PUBLIC_GENESIS="a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23"
RETIRED_GENESES="d73dbf44dbae3522c05de7aada567f9448221bc5c832393c9eb2996698e91aba2e638f0eea1f89bccbf72268750bb62deb1e958440136ca748727747f230fe18
f6cc957686f7047dc9fe6c9619d8f487237574b0596573af9d337c8f1f1980151b26256967a9a81f7ae31a5a352e249dd5426c0ce9144f75eb26a3ca3a963a30
fb1074b0e7dd303158c0a3c79725a55700d731fd0bc654f739534341f566ae495f7aeae9ecbecc33bb997a6908d2adbf97dc5d3def6f5fad5d255e77e47469aa
a8cabac47b96fe30d9675ce08a355f62e6d57aac84865b0d6295c024c6d8ff61fb2d93943952e8da4c36ff87ea7f17f6c8de643fa7b02ecf7512892d590777dd
1eaa6c0fb5e2223cf148ca64ef336a31f408be3f1d491c1edbf1f0da65f56b60806d1e0a264434bcca150288d951d41485fb1574abb79a273094e778b89d6d7c"
FORBIDDEN_GENESES="$(echo $PUBLIC_GENESIS $RETIRED_GENESES)"
FORBIDDEN_GENESIS_PREFIXES="$(for _h in $FORBIDDEN_GENESES; do printf '%s ' "${_h:0:8}"; done)"; unset _h
csv() { echo $* | tr ' ' ','; }   # csv <space-separated words> → a,b,c (chainstate.py's list form)
# premine txids: the drill's (BLAKE2b-512 keyed "misaka-premine-txid/v1" over sentinel ‖ "testnet-12" ‖ DRILL_SALT,
# recomputed from premine.rs), public t12's (same with the public salt), the FIRST pre-drill's (salt
# …/DRILL-PRE-e93be0f2), and the pre-salt sentinel every earlier t12 used ("misaka-premine" zero-padded)
PREMINE="269a354e1bf394af23bdec64ac3b68fd69dfc3cc238d449b5eda96a5aee599175674afe3b6fafd85b88bea6d336f4dabe243fd6bff290870555571111ac0918d"
PUBLIC_PREMINE="5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669"
FIRST_DRILL_PREMINE="65093cb6ac4a7f889de9994a467801c293c6a98eaae8654a7598840cece65d6cf1d3acb2eba5abe7fee14d4790bc6daec1bcc1d5be9e0e7080be5a476e8ff07b"
SENTINEL_PREMINE="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"
FORBIDDEN_PREMINES="$PUBLIC_PREMINE $SENTINEL_PREMINE $FIRST_DRILL_PREMINE"
# params fingerprints public testnet-12 nodes have printed (8-hex prefixes; the c8652a97 release's own is
# not recorded yet — pass it as PUBLIC_FP when it is). The drill's cannot equal any of them: the
# fingerprint hashes genesis.hash (params.rs consensus_params_id).
PUBLIC_FP_DENY="fb8f378d c746f07c 30848c6b bcfbf2a3 f66bf139 88a9aee8 ${PUBLIC_FP:-}"
# the operator's PUBLIC PALW main wallet (premine.rs PALW_PUBLIC_MAIN_ADDRESS; build-ship.sh checks the
# literal): public testnet-12's main wallet at c8652a97. On the DRILL chain it owns nothing — the drill
# commit moved the main wallet to TESTNET_MAIN_ADDRESS. The kit never holds its key and never sends from
# it; a key that resolved to it is refused.
PALW_PUBLIC_MAIN_ADDRESS="misakatest:qf7hzj76mg0wrch9mm89ag8s8apgrz7qgkk77j5z0ypykngrl2ayd2rnvleafk0fxhaxl70kr29x6fakav79jax9ul6jghrcs42nmlqx0tawqn8x"
# THE DRILL CHAIN'S MAIN WALLET (005e5c5f premine.rs main_address_for: testnet-12 → TESTNET_MAIN_ADDRESS):
# it owns $PREMINE:40 and the eight genesis-bond collaterals $PREMINE:0..7 (locked; the CLI never selects
# them). Its key is regenerable from the PUBLIC, value-less test seed `tests::TESTNET_MAIN_SEED`
# (testnet_main_key_is_reproducible: seed = BLAKE2b-256(TESTNET_MAIN_SEED), ML-DSA-87 keygen). `drill.sh
# mainkey` reads the seed from the built tree ($SRC_DIR), requires it to equal the copy below, and imports
# it with the drill CLI from STDIN (never argv, never the environment) into the 0600 file $MAIN_KEY.
# The kit may sign with this key ONLY because the drill premine txid is salted: before any spend it proves
# the node booted the drill genesis (assert_drill_chain), so every outpoint the key spends is $PREMINE:40 or
# a drill-chain descendant of it — outpoints no other chain has.
TESTNET_MAIN_ADDRESS="misakatest:qtpflz03z576h02mtpn2vtwg5npj8fhlau3fgmsjl2a2uw0venj3573l07uahcs4gnsl8eqc7nlq5phakthxy606q2jyuxh2a08weduxa2yqlxuz"
TESTNET_MAIN_SEED_EXPECTED="misaka-testnet-premine-9b-claude-managed"   # premine.rs `tests::TESTNET_MAIN_SEED` (public)
MAIN_PREMINE_INDEX=40
MAIN_KEY="$FRESH/main.key"

# ---- the ruleset's facts the checks depend on (verified in the source at 005e5c5f) --------------
FEE_FLOAT_BASE=41                                  # card N's fee float is premine index MAIN_PREMINE_INDEX+1+N = 41+N
C_FLOOR="${C_FLOOR:-f1c5635c6e47e96e7af864789c94523335dc56584af297cb8cc19021c228b897bee1a50145597e45f8ca2727349bf4aa352a98cc05274b7f059a176642f623c8}"
C_8K="${C_8K:-ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01}"
# the 8k row is a HELD class at c8652a97 (class_is_held_v1; f8c91f19/0533e1de, ADR-0152 T-2(b)): verification
# window 3 spans, max_inflight_claims 5, and every claim of it stays on the budget until FINAL (a licence
# does not release it). So at most 5 8k claims are in flight before the first 8k Final (≈ its licence +
# 120 DAA): then the gate refuses "class … has 5 claims in flight against the registry's cap of 5", op 186
# shows panelRoom 0, and n2 holds "class … has no panel room left in the network's verification budget".
C_8K_MAX_INFLIGHT=5
C_2M_PREFIX="${C_2M_PREFIX:-74c67e63}"             # the held 2M row: registered, never seatable here
SHARE_3_5_GIB=3758096384                           # --palw-host-memory-share for every 8k seat
# a3c5db22 (mainnet-assumed bonds): PALW_MAINNET_MIN_COLLATERAL_SOMPI = 13,000 MSK is t12's producer floor
# AND its registration floor (palw_bond_registration_floor_v1); the seat floor is 10× (130,000 MSK,
# palw_panel_collateral_floor_v1); readiness needs 3× the floor free (39,000 MSK).
PRODUCER_FLOOR_SOMPI=1300000000000                 # 13,000 MSK
PANEL_FLOOR_SOMPI=13000000000000                   # 130,000 MSK
READINESS_FREE_SOMPI=3900000000000                 # 39,000 MSK
CHEAP_COLLATERAL_SOMPI=10000000                    # 0.1 MSK: kaspad refuses it before looking for funds (C3a)
FLOOR_MINUS_ONE_SOMPI=$((PRODUCER_FLOOR_SOMPI - 1)) # refused too: the floor is exact
CONTROL_COLLATERAL_SOMPI="${CONTROL_COLLATERAL_SOMPI:-13500000000000}"  # 135,000 MSK: above the seat floor (x2)
# Drill money comes from the drill chain's own main wallet (MAIN_KEY, the TESTNET_MAIN_SEED key — see
# above): F0 (`drill.sh fund`; `drill.sh b1` runs it once C3a has seen x1 unfunded) sends from its
# $PREMINE:40, one confirmed send at a time, to the keygen-made third-party wallets:
#   x1  13,001 MSK   the 13,000 MSK producer-floor bond (C3b, C4-cheap-bond) + its fee
#   x2  135,001 MSK  the control bond, above the 130,000 MSK seat floor (CONTROL=0 skips it)
#   x3  20 MSK       C11-fee's five payments (+ fees)
# FUND_KEY is the wallet the kit's other payments fall back to (C11-fee's pay_source); it must be MAIN_KEY
# or a keygen-made key under $FRESH (assert_drill_key).
FUND_KEY="${FUND_KEY:-$MAIN_KEY}"
F0_X1_MSK=$(( PRODUCER_FLOOR_SOMPI / 100000000 + 1 ))
F0_X2_MSK=$(( CONTROL_COLLATERAL_SOMPI / 100000000 + 1 ))
F0_X3_MSK="${F0_X3_MSK:-20}"
COINBASE_MATURITY_DAA=600                          # PALW_T12_DNS_PARAMS.coinbase_settlement_long_maturity_daa (Decision A: DAA only)
LIC_QUORUM="${LIC_QUORUM:-3}"                      # a floor panel licenses on 3 Valid of 5

# ---- ports: node i listens on P2P base+10i+1, wRPC-borsh +2, wRPC-json +3 ---------------------
DRILL_PORT_BASE="${DRILL_PORT_BASE:-28600}"
p2p()   { echo $((DRILL_PORT_BASE + 10 * $1 + 1)); }
borsh() { echo $((DRILL_PORT_BASE + 10 * $1 + 2)); }
json()  { echo $((DRILL_PORT_BASE + 10 * $1 + 3)); }

# ---- wall time ---------------------------------------------------------------------------------
# This build arms the clock floor at DAA 0 (palw_clock_floor, H3/H5): the clock steps at most once per
# 120 s slot, so the expected pace is ~2.0–2.1 min/DAA (the 2.0–3.7 min/DAA once measured on live t12
# predates the floor). C1 measures the real pace from n0's "advanced the clock to DAA N" lines
# (`drill.sh pace` re-measures it from the observer). Timeouts stay sized for 6 min/DAA: they only
# matter when something is stuck, and STALL_S catches a stuck clock first.
SLOW_S_PER_DAA="${SLOW_S_PER_DAA:-360}"
daa_wall_s() { echo $(( $1 * SLOW_S_PER_DAA + ${2:-900} )); }
# measured seconds per DAA (C1 writes $RUN/pace.s); 123 s until measured
pace_s() { local p; p=$(cat "$RUN/pace.s" 2>/dev/null); echo "${p:-123}"; }
eta()    { local n=$1 p; p=$(pace_s); echo "~$(( n * p / 60 )) min at $(awk -v p="$p" 'BEGIN{printf "%.2f", p/60}') min/DAA"; }

# ---- memory guard: never start a node when the host would drop below this ---------------------
# The live floor seats (misaka-t12f-b2..b5, ~3.4 GiB RSS) share this host and serve the PUBLIC chain.
MIN_AVAIL_GIB="${MIN_AVAIL_GIB:-5}"

# ---- the roster -----------------------------------------------------------------------------
# side A (hub n0): n0 n1 n4 n5      side B (hub n3): n3 n2 n6      bridge: n0 --addpeer n3
# n0: clock A + observer (json)     n1: floor producer   n2: 8k producer (+canonical FP)
# n3: clock B + floor producer      n4 n5 n6: seats       every one: 8k seat, round lane
SIDE_A="0 1 4 5"; SIDE_B="3 2 6"; HUBS="0 3"
is_side_b() { case " $SIDE_B " in *" $1 "*) return 0;; esac; return 1; }
is_hub()    { case " $HUBS " in *" $1 "*) return 0;; esac; return 1; }
hub_of()    { if is_side_b "$1"; then echo 3; else echo 0; fi; }

bond_of()  { echo "$PREMINE:$1"; }
float_of() { echo "$PREMINE:$((FEE_FLOAT_BASE + $1))"; }
key_of()   { echo "$KEYS/t12-bond-$1.key"; }
cli()      { local i=$1; shift; "$CLI" --network testnet-12 --rpc "127.0.0.1:$(borsh "$i")" "$@"; }
addr_of()  { "$CLI" --network testnet-12 key address --key-file "$1" 2>/dev/null | tail -1 | awk '{print $NF}'; }

# ---- drill-only wallets (item 6: coinbase txids do not depend on the genesis) ------------------
# Every block the drill mines names a drill-only script in its coinbase payload — producers through
# --palw-producer-pay-address, the clocks through --palw-heartbeat-miner-address — so no coinbase on
# this chain can repeat a public testnet-12 coinbase txid, and every wallet send is signed by a
# drill-only key or by the drill main wallet on the proven drill chain. Round blocks (algo 10) name the
# bond's registered payout, but a round block is never a chain block, so its coinbase never enters the
# UTXO set. x1..x3 are the third-party keys; F0 fills them from the drill main wallet (MAIN_KEY, not in
# this list: it is imported from the public test seed, not generated).
DRILL_WALLETS="pay-0 pay-1 pay-2 pay-3 pay-4 pay-5 pay-6 hb-0 hb-3 x1 x2 x3"
wallet_key()  { echo "$FRESH/$1.key"; }
wallet_addr() { local f="$FRESH/$1.addr"; [ -s "$f" ] || { log "no $f — run \`drill.sh keygen\` first"; return 1; }; cat "$f"; }
pay_addr()    { wallet_addr "pay-$1"; }
hb_addr()     { wallet_addr "hb-$1"; }
# assert_drill_key <key-file> [offline] — the ONLY keys that may sign a wallet send on this chain:
#   * a key keygen made under $FRESH (its sha256 is in keygen.sha256), or
#   * the drill main wallet $MAIN_KEY (the public TESTNET_MAIN_SEED key `drill.sh mainkey` imported: its
#     sha256 is in mainkey.sha256 and its address IS TESTNET_MAIN_ADDRESS) — and then only after
#     assert_drill_chain has proven, on the node the send goes through, that the booted genesis is the
#     drill's (`offline` skips that proof; preflight uses it before any node runs, and sends nothing);
# and in both cases a key whose address is no card address (= every card's payout, params.rs "The payout
# is the bond key's own address … like every other card's") and not the operator's public main wallet.
assert_drill_key() {
  local k=$1 mode=${2:-} real a s
  real=$(readlink -f "$k" 2>/dev/null) || die "refusing: cannot resolve $k"
  case "$real" in "$FRESH"/*) ;; *) die "refusing: $k is not a drill wallet under $FRESH — the drill sends only from keygen's wallets and the imported drill main wallet";; esac
  case "$real" in "$KEYS"/*) die "refusing: $k is a card key";; esac
  a=$(addr_of "$real"); [ -n "$a" ] || die "cannot read an address from $k"
  [ "$a" = "$PALW_PUBLIC_MAIN_ADDRESS" ] && die "refusing: $k pays to the operator's PUBLIC main wallet address"
  if [ -s "$FRESH/card-addresses.txt" ] && grep -qxF "$a" "$FRESH/card-addresses.txt"; then
    die "refusing: $k pays to a card (bond = payout) address"
  fi
  if [ "$real" = "$(readlink -f "$MAIN_KEY" 2>/dev/null)" ] || [ "$a" = "$TESTNET_MAIN_ADDRESS" ]; then
    # the public test key: allowed ONLY because this chain's premine txid is salted — so the chain it
    # would sign for is proven to be the drill's before every use
    assert_main_key_file "$real"
    [ "$mode" = offline ] || assert_drill_chain 0
    return 0
  fi
  s=$(sha256sum "$real" 2>/dev/null | awk '{print $1}')
  [ -n "$s" ] && [ -s "$FRESH/keygen.sha256" ] && grep -q "^$s " "$FRESH/keygen.sha256" \
    || die "refusing: $k was not generated by \`drill.sh keygen\` (its sha256 is not in $FRESH/keygen.sha256)"
  return 0
}

# assert_main_key_file <key-file> — the file is $MAIN_KEY as `drill.sh mainkey` wrote it (sha256 recorded in
# mainkey.sha256, mode 0600) and its address is TESTNET_MAIN_ADDRESS
assert_main_key_file() {
  local real a s
  real=$(readlink -f "$1" 2>/dev/null) || die "refusing: cannot resolve $1"
  [ "$real" = "$(readlink -f "$MAIN_KEY" 2>/dev/null)" ] || die "refusing: $1 resolves to TESTNET_MAIN_ADDRESS but is not the imported drill main wallet $MAIN_KEY"
  [ "$(stat -c %a "$real" 2>/dev/null)" = 600 ] || die "refusing: $real is not mode 0600"
  s=$(sha256sum "$real" 2>/dev/null | awk '{print $1}')
  [ -n "$s" ] && [ -s "$FRESH/mainkey.sha256" ] && grep -qx "$s main" "$FRESH/mainkey.sha256" \
    || die "refusing: $1 is not the key \`drill.sh mainkey\` imported (its sha256 is not in $FRESH/mainkey.sha256)"
  a=$(addr_of "$real")
  [ "$a" = "$TESTNET_MAIN_ADDRESS" ] || die "refusing: $1's address (${a:-none}) is not TESTNET_MAIN_ADDRESS"
  return 0
}

# assert_drill_chain [node] — THE guard before any spend: the node the send goes through (n0 unless named)
# is this kit's running drill kaspad, it booted the DRILL genesis ${DRILL_GENESIS:0:8}… (getBlock answers it
# at DAA 0 — "pruned past genesis" is not accepted here), its bonds sit on the DRILL premine txid
# ${PREMINE:0:8}…, and it knows no public/retired genesis and no bond on a public, sentinel or first-drill
# premine. The drill main wallet signs only when this holds: then every outpoint it can spend is
# $PREMINE:$MAIN_PREMINE_INDEX or a drill-chain descendant of it, and no other chain has one.
# The answer is written to a file of this process's own ($SPEND_GUARD_FILE), not identity-n<i>.json: a
# command running in parallel (alarm's invariants beside b1) may be rewriting that one.
SPEND_GUARD_FILE=""
assert_drill_chain() {
  local i=${1:-0} t ok=""
  SPEND_GUARD_FILE="$RUN/spend-guard-n$i.${BASHPID:-$$}.json"
  case " $FORBIDDEN_PREMINES " in *" $PREMINE "*) die "refusing to spend: the kit's PREMINE is a public/sentinel/first-drill premine";; esac
  [ "$(cat "$BIN_DIR/REV" 2>/dev/null)" = "$SHIP_REV_EXPECTED" ] || die "refusing to spend: bin/REV is not the drill build $SHIP_REV_EXPECTED"
  alive "$i" || die "refusing to spend: n$i is not this drill's running kaspad"
  for t in 1 2 3; do
    if $CHAINSTATE genesis --port "$(json "$i")" --drill-genesis "$DRILL_GENESIS" --forbidden "$(csv $FORBIDDEN_GENESES)" \
         --forbidden-prefixes "$(csv $FORBIDDEN_GENESIS_PREFIXES)" \
         --drill-premine "$PREMINE" --forbidden-premines "$(csv $FORBIDDEN_PREMINES)" > "$SPEND_GUARD_FILE" 2>&1 \
       && python3 - "$SPEND_GUARD_FILE" <<'PY'
import json,sys
v=json.load(open(sys.argv[1]))
sys.exit(0 if v.get("ok") and v.get("drillGenesisPresent") is True and v.get("drillGenesisDaa")==0 and not v.get("prunedPastGenesis") else 1)
PY
    then ok=1; break; fi
    [ "$t" -lt 3 ] && sleep 10
  done
  [ -n "$ok" ] || die "refusing to spend: n$i does not prove the drill chain — genesis ${DRILL_GENESIS:0:8}… at DAA 0, bonds on premine ${PREMINE:0:8}…, nothing public ($SPEND_GUARD_FILE)"
  return 0
}

log()  { printf '[drill %s] %s\n' "$(date -u +%FT%TZ)" "$*" | tee -a "$DRILL_ROOT/timeline.log" >&2; }
die()  { log "FATAL: $*"; exit 1; }
pass() { log "PASS $1 — $2"; echo "PASS $(date -u +%FT%TZ) $2" > "$EV/$1/VERDICT"; }
fail() { log "FAIL $1 — $2"; echo "FAIL $(date -u +%FT%TZ) $2" > "$EV/$1/VERDICT"; return 1; }
# a check the chain (or the funding) could not reach in this run: neither PASS nor FAIL
notreached() { log "NOT-REACHED $1 — $2"; echo "NOT-REACHED $(date -u +%FT%TZ) $2" > "$EV/$1/VERDICT"; return 0; }
ev()   { mkdir -p "$EV/$1"; echo "$EV/$1"; }

avail_gib() { awk '/MemAvailable/{printf "%.1f", $2/1048576}' /proc/meminfo; }
mem_guard() {
  local need="${1:-0}" a
  a=$(avail_gib)
  awk -v a="$a" -v n="$need" -v m="$MIN_AVAIL_GIB" 'BEGIN{exit !(a - n >= m)}' \
    || die "MemAvailable ${a} GiB minus ${need} GiB for this step would leave less than ${MIN_AVAIL_GIB} GiB — the live floor seats share this host"
}

# art_path_ok <path> — the path (resolved) is not under /root/perm-drill
art_path_ok() { local r; r=$(readlink -f "$1" 2>/dev/null || echo "$1"); case "$r" in "$FORBIDDEN_TREE"|"$FORBIDDEN_TREE"/*) return 1;; esac; return 0; }

# ---- root's own endpoint registry: recorded before, compared after -------------------------------
endpoints_fingerprint() { # "<sha256> <mtime>" of /root/.misaka/testnet-12/endpoints.json, or "absent"
  [ -e "$PUBLIC_ENDPOINTS" ] || { echo absent; return; }
  echo "$(sha256sum "$PUBLIC_ENDPOINTS" | awk '{print $1}') $(stat -c %Y "$PUBLIC_ENDPOINTS")"
}
# endpoints_untouched — true when root's registry is as preflight recorded it, or (a live t12 unit
# restarted and rewrote it) when it names no drill port; the drill's own registry is under $DRILL_HOME
endpoints_untouched() {
  local rec="$EV/C0-preflight/public-endpoints-before.txt" now
  [ -s "$rec" ] || return 0
  now=$(endpoints_fingerprint)
  [ "$now" = "$(cat "$rec")" ] && return 0
  [ -e "$PUBLIC_ENDPOINTS" ] && grep -qE "127\.0\.0\.1:286[0-9][0-9]" "$PUBLIC_ENDPOINTS" && return 1
  log "note: $PUBLIC_ENDPOINTS changed since preflight but names no drill port (a live t12 unit restarted)"
  return 0
}

# ---- node launcher ----------------------------------------------------------------------------
# node_args <i> <mode>  — mode: normal | partition (hub A drops its --addpeer to hub B)
node_args() {
  local i=$1 mode=${2:-normal} a=()
  a+=(--testnet --netsuffix=12 --yes --appdir="$RUN/n$i"
      --listen="127.0.0.1:$(p2p "$i")" --rpclisten-borsh="127.0.0.1:$(borsh "$i")" --rpclisten-json="127.0.0.1:$(json "$i")"
      --nogrpc --utxoindex --unsaferpc --nodnsseed --disable-upnp)
  case "$i" in
    7) # IBD follower: no PALW duties, a bounded cache, a client-only leaf of hub A
       a+=(--ram-scale=0.3 --connect="127.0.0.1:$(p2p 0)"); printf '%s\n' "${a[@]}"; return ;;
    8) # registrar: a fresh key's --palw-register-bond, nothing else (REG_KEY/REG_COLL/REG_FUND set by the caller).
       # Its pay address is the fresh key's own (drill-only): the collateral is drawn from it and the bond pays to it.
       local reg_addr; reg_addr=$(addr_of "$REG_KEY"); [ -n "$reg_addr" ] || die "cannot read an address from $REG_KEY"
       a+=(--ram-scale=0.1 --connect="127.0.0.1:$(p2p 0)"
           --palw-register-bond --palw-producer-key="$REG_KEY" --palw-producer-pay-address="$reg_addr"
           --palw-bond-collateral="$REG_COLL")
       [ -n "${REG_FUND:-}" ] && a+=(--palw-fee-outpoint="$REG_FUND")
       printf '%s\n' "${a[@]}"; return ;;
  esac
  # a genesis-bond node: 8k seat + round lane, its own card and fee float, a DRILL-ONLY pay address.
  # `$RUN/n$i.noart` (the silence phase) drops the 8k artifact and keeps every floor duty.
  local pa hb
  pa=$(pay_addr "$i") && [ -n "$pa" ] || { log "n$i: no drill-only pay address (drill.sh keygen)"; return 1; }
  a+=(--palw-panel --palw-chain-classes --palw-host-memory-share="$SHARE_3_5_GIB" --palw-round-lane
      --palw-producer-key="$(key_of "$i")" --palw-producer-bond="$(bond_of "$i")" --palw-fee-outpoint="$(float_of "$i")"
      --palw-producer-pay-address="$pa")
  [ -e "$RUN/n$i.noart" ] || a+=(--palw-class-artifact="$ART8K")
  case "$i" in
    0|3) hb=$(hb_addr "$i") && [ -n "$hb" ] || { log "n$i: no drill-only heartbeat address (drill.sh keygen)"; return 1; }
         a+=(--enable-unsynced-mining --palw-heartbeat-miner-address="$hb") ;;
  esac
  # topology (see the header): hubs listen and dial nothing by discovery; hub A adds hub B unless
  # partitioned; leaves are client-only and dial their hub
  if [ "$i" = 0 ]; then a+=(--outpeers=0); [ "$mode" = partition ] || a+=(--addpeer="127.0.0.1:$(p2p 3)")
  elif [ "$i" = 3 ]; then a+=(--outpeers=0)
  else a+=(--connect="127.0.0.1:$(p2p "$(hub_of "$i")")"); fi
  # the producer switches a phase turned on for this node persist across restarts in `$RUN/n$i.extra`
  if [ -s "$RUN/n$i.extra" ]; then
    # shellcheck disable=SC2207
    a+=($(cat "$RUN/n$i.extra"))
  fi
  printf '%s\n' "${a[@]}"
}
set_extra() { local i=$1; shift; printf '%s\n' "$*" > "$RUN/n$i.extra"; }

# ---- pids: a pid file is believed only when the process is provably this drill's (review 2) -------
# drill_pid_ok <i> <pid> — /proc/<pid> runs $KASPAD with --appdir=$RUN/n<i>. A pid file left behind by
# a node that died, then reused by the kernel, must never earn a SIGINT/SIGKILL: on this host the
# reused pid could be a live t12 floor seat, the t11 kaspad or the :26313 poller.
drill_pid_ok() {
  local i=$1 p=$2 exe want
  [ -n "$p" ] && [ -r "/proc/$p/cmdline" ] || return 1
  tr '\0' '\n' < "/proc/$p/cmdline" 2>/dev/null | grep -qxF -- "--appdir=$RUN/n$i" || return 1
  exe=$(readlink "/proc/$p/exe" 2>/dev/null); exe=${exe% (deleted)}
  want=$(readlink -f "$KASPAD" 2>/dev/null)
  [ -n "$exe" ] && [ "$exe" = "$want" ]
}
observer_pid_ok() { # observer_pid_ok <pid> — a bash running this kit's `drill.sh observe`
  local p=$1 c
  [ -n "$p" ] && [ -r "/proc/$p/cmdline" ] || return 1
  c=$(tr '\0' '\n' < "/proc/$p/cmdline" 2>/dev/null)
  echo "$c" | grep -qE '(^|/)drill\.sh$' && echo "$c" | grep -qx observe
}
pid_of()  { cat "$RUN/n$1.pid" 2>/dev/null; }
alive()   { local p; p=$(pid_of "$1"); [ -n "$p" ] && kill -0 "$p" 2>/dev/null && drill_pid_ok "$1" "$p"; }
# a pid file whose process is gone or not ours is removed, never signalled
forget_stale_pid() { local p; p=$(pid_of "$1"); [ -n "$p" ] || return 0; drill_pid_ok "$1" "$p" && return 0; rm -f "$RUN/n$1.pid"; log "n$1: stale pid file ($p is not this drill's kaspad) removed, nothing signalled"; }
logf()    { echo "$RUN/n$1.log"; }
# the byte offset of a node's log NOW — evidence for an action is read only past it
mark()    { wc -c < "$(logf "$1")" 2>/dev/null || echo 0; }
since()   { tail -c +"$(( $2 + 1 ))" "$(logf "$1")" 2>/dev/null; }
p2p_listening() { ss -ltn "( sport = :$(p2p "$1") )" 2>/dev/null | grep -q LISTEN; }

start_node() {
  local i=$1 mode=${2:-normal} args=() x p t
  forget_stale_pid "$i"
  alive "$i" && die "n$i is already running (pid $(pid_of "$i"))"
  case "$i" in 7|8) mem_guard 0.5 ;; *) mem_guard 1.5 ;; esac
  # node_args dies (in its subshell) on a missing drill-only address; read it into a file first so the
  # failure stops THIS shell instead of launching a node with half its flags
  node_args "$i" "$mode" > "$RUN/n$i.args" || die "n$i: node_args failed (see above)"
  while IFS= read -r x; do args+=("$x"); done < "$RUN/n$i.args"
  printf '%q ' env -i PATH="$PATH" HOME="$DRILL_HOME" RUST_BACKTRACE=1 "$KASPAD" "${args[@]}" > "$RUN/n$i.cmd"; echo >> "$RUN/n$i.cmd"
  echo "=== start $(date -u +%FT%TZ) mode=$mode" >> "$(logf "$i")"
  # env -i: no KASPAD_*/MISAKA_* can reach kaspad; HOME is the drill's (its endpoints.json lands there)
  nohup setsid env -i PATH="$PATH" HOME="$DRILL_HOME" RUST_BACKTRACE=1 "$KASPAD" "${args[@]}" >> "$(logf "$i")" 2>&1 < /dev/null &
  p=$!
  sleep 3
  # setsid/env exec in place, so $! is kaspad; if a wrapper forked, find the process by its own appdir
  if ! drill_pid_ok "$i" "$p"; then
    p=$(for q in $(pgrep -f -- "--appdir=$RUN/n$i" 2>/dev/null); do drill_pid_ok "$i" "$q" && echo "$q"; done | head -1)
  fi
  [ -n "$p" ] || { tail -20 "$(logf "$i")" >&2; die "n$i exited at startup"; }
  echo "$p" > "$RUN/n$i.pid"
  alive "$i" || { tail -20 "$(logf "$i")" >&2; die "n$i exited at startup"; }
  # a hub must LISTEN (the --connect trap), a leaf must not (it is client-only by design)
  if is_hub "$i"; then
    for t in $(seq 1 20); do p2p_listening "$i" && break; sleep 3; done
    p2p_listening "$i" || { tail -20 "$(logf "$i")" >&2; die "hub n$i does not LISTEN on 127.0.0.1:$(p2p "$i") — no peer could reach it"; }
  elif [ "$i" -le 6 ] && p2p_listening "$i"; then
    log "note: leaf n$i listens on $(p2p "$i") although it has --connect (the client-only premise changed?)"
  fi
  log "n$i up (pid $(pid_of "$i"), mode $mode)"
}

stop_node() {
  local i=$1 p t=0
  p=$(pid_of "$i"); [ -n "$p" ] || return 0
  drill_pid_ok "$i" "$p" || { forget_stale_pid "$i"; return 0; }
  kill -INT "$p" 2>/dev/null || { rm -f "$RUN/n$i.pid"; return 0; }
  while kill -0 "$p" 2>/dev/null; do
    sleep 1; t=$((t + 1))
    if [ $t -ge 120 ]; then
      drill_pid_ok "$i" "$p" && { kill -KILL "$p" 2>/dev/null; log "n$i needed SIGKILL"; }
      break
    fi
  done
  rm -f "$RUN/n$i.pid"
  log "n$i stopped after ${t}s"
}

restart_node() { stop_node "$1"; start_node "$1" "${2:-normal}"; }

# ---- chain reads (wRPC-json through chainstate.py) --------------------------------------------
daa_of()  { $CHAINSTATE dag --port "$(json "$1")" --field virtualDaaScore 2>/dev/null; }
sink_of() { $CHAINSTATE dag --port "$(json "$1")" --field sink 2>/dev/null; }
# the params fingerprint the node booted with, over RPC (getPalwNodeStatus.consensusParamsId)
fp_of()   { $CHAINSTATE status --port "$(json "$1")" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("consensusParamsId",""))' 2>/dev/null; }

# wait_until "<predicate>" <what> [max_wall_s] — gives up when node 0's virtual DAA has not moved
# for STALL_S seconds (a stalled chain is a finding, not a reason to keep waiting) or at max_wall_s.
# The clock steps at most once per 120 s slot, so 30 min without a step is 15 missed slots.
STALL_S="${STALL_S:-1800}"
wait_until() {
  local pred=$1 what=$2 max=${3:-864000} began=$SECONDS last="" moved=$SECONDS d
  while :; do
    eval "$pred" && return 0
    d=$(daa_of 0 || true)
    if [ -n "$d" ] && [ "$d" != "$last" ]; then last=$d; moved=$SECONDS; fi
    [ $((SECONDS - moved)) -ge "$STALL_S" ] && die "gave up on: $what — virtual DAA stuck at ${last:-?} for $((SECONDS - moved)) s"
    [ $((SECONDS - began)) -ge "$max" ] && die "gave up on: $what — ${max} s elapsed (DAA ${last:-?})"
    sleep 20
  done
}

# the global invariants every phase ends with (C0)
invariants() {
  local tag=$1 bad=0 i n
  for i in 0 1 2 3 4 5 6; do   # a node with a pid file was started and not stopped by the drill
    [ -e "$RUN/n$i.pid" ] || continue
    alive "$i" || { log "INVARIANT[$tag]: n$i exited or its pid is not this drill's (see $(logf "$i"))"; bad=1; }
  done
  n=$(cat "$RUN"/n*.log 2>/dev/null | grep -cE "panicked at|disqualified from virtual chain \(PALW state root\)|FATAL" || true)
  [ "$n" = 0 ] || { log "INVARIANT[$tag]: $n panic/state-root-disqualification/FATAL lines (grep them in $RUN/n*.log)"; bad=1; }
  # the booted genesis is the drill's, on every node that answers (item 4)
  for i in 0 1 2 3 4 5 6; do
    alive "$i" || continue
    identity_ok "$i" 3 || { log "INVARIANT[$tag]: n$i does not prove the drill genesis/premine (chainstate.py genesis --port $(json "$i"))"; bad=1; }
  done
  endpoints_untouched || { log "INVARIANT[$tag]: $PUBLIC_ENDPOINTS names a drill port — a drill process wrote root's registry"; bad=1; }
  return $bad
}

# identity_ok <node> [tries] — the node answers `chainstate genesis` with ok (drill genesis + drill premine,
# no public/retired/first-drill genesis, no bond on a public/sentinel/first-drill premine). Retried 10 s
# apart: a node that has just started may not have its PALW state tip yet.
identity_ok() {
  local i=$1 n=${2:-3} t
  for t in $(seq 1 "$n"); do
    $CHAINSTATE genesis --port "$(json "$i")" --drill-genesis "$DRILL_GENESIS" --forbidden "$(csv $FORBIDDEN_GENESES)" \
      --forbidden-prefixes "$(csv $FORBIDDEN_GENESIS_PREFIXES)" \
      --drill-premine "$PREMINE" --forbidden-premines "$(csv $FORBIDDEN_PREMINES)" > "$RUN/identity-n$i.json" 2>&1 && return 0
    [ "$t" -lt "$n" ] && sleep 10
  done
  return 1
}

# measure_pace <node> <out-file> — seconds per DAA from the node's own heartbeat lines
# ("… — granted: … advanced the clock to DAA N", timestamps in the kaspad log format), over the last hour
measure_pace() {
  python3 - "$(logf "$1")" <<'PY' > "$2" 2>&1
import re,sys
from datetime import datetime
pts=[]
for line in open(sys.argv[1],errors="replace"):
    m=re.search(r"advanced the clock to DAA (\d+)",line)
    if not m: continue
    try: t=datetime.fromisoformat(line[:29].strip())
    except ValueError: continue
    pts.append((t.timestamp(),int(m.group(1))))
if len(pts)<2: print("pace: fewer than 2 granted heartbeats"); sys.exit(1)
last=pts[-1][0]; win=[p for p in pts if p[0]>=last-3600] or pts
if len(win)<2: win=pts[-2:]
dt=win[-1][0]-win[0][0]; dd=win[-1][1]-win[0][1]
if dd<=0: print("pace: the clock did not advance in the window"); sys.exit(1)
print(f"pace {dt/dd:.1f} s/DAA over DAA {win[0][1]}..{win[-1][1]} ({len(win)} granted beats, {dt/60:.1f} min)")
print(int(round(dt/dd)))
PY
}
