#!/usr/bin/env bash
# drill-t12/drill.sh — the testnet-12 pre-drill of c8652a97, one phase per subcommand.
#
# Run ON the drill host, as root, from $DRILL_ROOT (default /root/drill-t12), with the drill build
# (005e5c5f = c8652a97 + the drill-only commit: identity salt + TESTNET_MAIN_ADDRESS as the drill chain's
# main wallet) in $DRILL_ROOT/bin (build-ship.sh). One private chain from the DRILL genesis 32a665d6…,
# seven persistent nodes, every one 127.0.0.1-only: two listening hubs (n0, n3; n0 --addpeer n3) and
# client-only leaves that --connect to their hub (lib.sh header). See CHECKLIST.md (PLAN at the top) for
# the order, the pass/fail criteria and the evidence each subcommand leaves under $DRILL_ROOT/ev/.
#
#   keygen      drill-only wallets under $DRILL_ROOT/keys (pay-0..6, hb-0, hb-3, x1..x3); never a card key's;
#               then `mainkey`
#   mainkey     import the drill main wallet: the PUBLIC test seed tests::TESTNET_MAIN_SEED (read from the
#               built tree) → keys/main.key over STDIN with the drill CLI; its address must be TESTNET_MAIN_ADDRESS
#   verify-art  check the 8k artifact + sidecar the operator placed in $DRILL_ROOT/art (never /root/perm-drill)
#   preflight   C0  binaries = SHIP_REV, drill identity in the binaries, flags, keys, artifact, ports, memory
#   up          C1  boot n3, n0, then n1 n2 n4 n5 from the DRILL genesis (n6 held back for C7), hubs listen,
#                   clock ticks, one non-public fingerprint over RPC, cards match keys, the pace measured
#   pace            re-measure seconds per DAA (observer rows, else n0's heartbeat lines)
#   alarm       C2  heartbeat-only window → lane alarm + gate refuses; producers on → alarm clears
#   b1          C3a a below-floor bond is refused before any money moves; then F0 (fund) and C3b/C4 setup
#   fund        F0  after proving n0 booted the drill genesis: the drill main wallet ($PREMINE:40) funds x1 13,001 /
#                   x2 135,001 / x3 20 MSK, one confirmed send at a time (x1 only once C3a has run)
#   hold7       C7  8k producer holds (E-MODEL-NOT-ADMITTING); 7th seat → Probation → it produces
#               C8  every 8k seat proves readiness at a 3.5 GiB share
#   restart     C5  8k producer and one drawn seat restarted mid-claim; the claim still licenses, n2 is back
#                   (produces, or holds at the held 8k row's in-flight cap)
#   licence     C13 floor claims with ≥ 3 Valid receipts filed license (≥ 95 %; the licence-stall fix is in
#                   this build: expected PASS); the stuck ones, with their seats
#   room        C14 op 186's panelRoom agrees with the fold's gate (its room, or the held row's in-flight cap);
#                   panelInflightReplay × the gate's window agrees with the gate's replay; no room refusal; no utilization Held
#   second      C15 a second class passing audit does not hold the first (passive; usually not reachable)
#   maturity    C16 the NODE accepts a --coinbase-only send of a ≥ 600-DAA coinbase (drill-only wallets)
#   silence     C10 setup: three seats of one bound 8k claim lose the artifact (the claim cannot license).
#                   Needs a FRESH bound 8k claim: the held 8k row admits a new one only after an 8k Final (soak)
#   reorg MODE  C9  MODE = span | final | timeout | exec — partition, both sides advance, rejoin
#   ibd         C6  a fresh node syncs from genesis and agrees with the fleet
#   route       C11 Final → ticket → permit → algo-10 → fee, with drill-only payments traced
#   c2watch     C11b the first 8k Final: no memory blow-up, tickets inside one span's window
#   unsilence   give the C10 seats their artifact back (after the second timeout)
#   cheapcheck  C4  every floor claim binds; a floor-priced (13,000 MSK) bond is never drawn as a seat
#   gate        C12 the launch gate by lane counts, per class (GATE_SHORT=1: the in-window form)
#   observe     background sampler (read only): DAA, sinks, memory, registry, lane alarm, classes, room
#   down        stop every drill node (and nothing else)
set -u
DRILL_ROOT="${DRILL_ROOT:-/root/drill-t12}"
# shellcheck source=lib.sh
source "$DRILL_ROOT/lib.sh"
mkdir -p "$RUN" "$EV" "$FRESH"
chmod 700 "$FRESH"
touch "$DRILL_ROOT/timeline.log"

roster_bonds() {
  local b=() i
  for i in 0 1 2 3 4 5 6 7; do b+=("$(bond_of "$i")"); done
  for x in x1 x2; do [ -s "$FRESH/$x.bond" ] && b+=("$(cat "$FRESH/$x.bond")"); done
  (IFS=,; echo "${b[*]}")
}
all_json_ports() { local p=() i; for i in 0 1 2 3 4 5 6; do p+=("$(json "$i")"); done; (IFS=,; echo "${p[*]}"); }
reg8k() { $CHAINSTATE registry --port "$(json 0)" --class "$C_8K" | python3 -c "import json,sys; r=json.load(sys.stdin); r=r[0] if r else {}; print(r.get('$1',''))"; }
# the lifecycle VARIANT: getPalwModelRegistry renders `state` with {:?} (rpc/service/src/service.rs:1891),
# so Probation/ActiveLimited arrive as "Probation { probes_passed: 0 }" / "ActiveLimited { stable_epochs: N }"
reg8k_state() { reg8k state | awk '{print $1}'; }
grace() { $CHAINSTATE registry --port "$(json 0)" | python3 -c "import json,sys; print(json.load(sys.stdin).get('graceUntilDaa',0))"; }
fp_certified() { $CHAINSTATE facts --port "$(json 0)" --class "$1" --bond "$(bond_of 1)" | python3 -c "import json,sys; print(json.load(sys.stdin).get('fpCertified', False))"; }
claim_phase() { # claim_phase <node> <bond> <claim-id>
  $CHAINSTATE claims --port "$(json "$1")" --bond "$2" --terminal | python3 -c "
import json,sys
r=[x for x in json.load(sys.stdin)['claims'] if x['claimId']=='$3']
print(r[0]['phase'] if r else 'gone')"
}
has_claims() { # has_claims <chainstate claims args> — true when the filtered list is non-empty
  $CHAINSTATE claims "$@" | python3 -c 'import json,sys; sys.exit(0 if json.load(sys.stdin)["claims"] else 1)'
}
sinks_agree() {
  local s0 i s; s0=$(sink_of 0); [ -n "$s0" ] || return 1
  for i in "$@"; do s=$(sink_of "$i"); [ "$s" = "$s0" ] || return 1; done
}
# the drill-only wallet JSON view: mature / immature / bonded / reserved and the newest outputs.
# NOTE: `mature` is the CLI's own arithmetic (misaka-cli/src/wallet.rs:333 is_spendable_settled), not
# the node's verdict; only an accepted send is the node's word (C16, C11-fee).
utxo_json() { cli 0 --output json wallet utxo list --key-file "$1" --recent "${2:-200}" 2>/dev/null; }
utxo_ge() { # utxo_ge <key-file> <MSK> — a mature, unlocked, non-coinbase output of at least MSK (its outpoint), or nothing
  utxo_json "$1" | python3 -c '
import json,sys
need=int(round(float(sys.argv[1])*1e8))
try: v=json.load(sys.stdin)
except Exception: sys.exit(0)
for u in v.get("recent") or []:
    if u.get("mature") and not u.get("bonded") and not u.get("reserved") and not u.get("coinbase") and int(u.get("sompi",0))>=need:
        print(u["outpoint"]); break' "$2"
}
mature_msk() { utxo_json "$1" 0 | python3 -c 'import json,sys
try: print(int(json.load(sys.stdin)["mature"]["sompi"])/1e8)
except Exception: print(0)'; }
ge_msk() { awk -v a="$1" -v b="$2" 'BEGIN{exit !(a+0 >= b+0)}'; }

# ---- keygen / verify-art (run once, before preflight) --------------------------------------
cmd_keygen() {
  # Fresh ML-DSA-87 seeds, 0600, never used anywhere else: the producers' pay addresses, the clocks'
  # heartbeat addresses, and the third-party keys. `misaka key gen` refuses to overwrite, so a re-run
  # keeps what exists. The card keys are READ (their public addresses only) to prove the sets are disjoint.
  # Provenance: keygen records the sha256 of every key file it made (the hash of the file, never its
  # contents) in keygen.sha256; assert_drill_key accepts only those — and the imported drill main wallet —
  # as signers.
  local n k a
  touch "$FRESH/keygen.sha256"; chmod 600 "$FRESH/keygen.sha256"
  for n in $DRILL_WALLETS; do
    k=$(wallet_key "$n")
    if [ ! -s "$k" ]; then
      "$CLI" --network testnet-12 key gen --out "$k" >> "$FRESH/keygen.log" 2>&1 || die "misaka key gen --out $k failed ($FRESH/keygen.log)"
      chmod 600 "$k"
      echo "$(sha256sum "$k" | awk '{print $1}') $n" >> "$FRESH/keygen.sha256"
    fi
    grep -q " $n\$" "$FRESH/keygen.sha256" || die "$k exists but keygen did not make it (no provenance line) — move it away and re-run keygen"
    a=$(addr_of "$k"); [ -n "$a" ] || die "no address for $k"
    echo "$a" > "$FRESH/$n.addr"
  done
  : > "$FRESH/card-addresses.txt"
  for n in 0 1 2 3 4 5 6 7; do a=$(addr_of "$(key_of "$n")"); [ -n "$a" ] && echo "$a" >> "$FRESH/card-addresses.txt"; done
  [ "$(wc -l < "$FRESH/card-addresses.txt")" = 8 ] || die "could not derive all 8 card addresses (read-only) for the disjointness check"
  grep -qxF "$TESTNET_MAIN_ADDRESS" "$FRESH/card-addresses.txt" && die "a card pays to TESTNET_MAIN_ADDRESS (the drill main wallet)"
  for n in $DRILL_WALLETS; do
    grep -qxF "$(cat "$FRESH/$n.addr")" "$FRESH/card-addresses.txt" && die "drill wallet $n pays to a card address"
    [ "$(cat "$FRESH/$n.addr")" = "$PALW_PUBLIC_MAIN_ADDRESS" ] && die "drill wallet $n pays to the operator's public main wallet"
    [ "$(cat "$FRESH/$n.addr")" = "$TESTNET_MAIN_ADDRESS" ] && die "drill wallet $n pays to the drill main wallet"
  done
  [ "$(for n in $DRILL_WALLETS; do cat "$FRESH/$n.addr"; done | sort -u | wc -l)" = "$(echo $DRILL_WALLETS | wc -w)" ] || die "two drill wallets share an address"
  chmod 600 "$FRESH"/*.key
  log "keygen: $(echo $DRILL_WALLETS | wc -w) drill-only wallets in $FRESH, disjoint from the 8 card (= payout) addresses, the operator's public main wallet and TESTNET_MAIN_ADDRESS"
  cmd_mainkey
}

cmd_mainkey() {
  # THE DRILL CHAIN'S MAIN WALLET. At 005e5c5f testnet-12's main wallet — $PREMINE:$MAIN_PREMINE_INDEX and the
  # genesis-bond collaterals $PREMINE:0..7 — is TESTNET_MAIN_ADDRESS, whose key is regenerable from the PUBLIC,
  # value-less test seed `tests::TESTNET_MAIN_SEED` (premine.rs; testnet_main_key_is_reproducible: the ML-DSA-87
  # seed is BLAKE2b-256 of it). The seed is read from the tree build-ship.sh built ($SRC_DIR) and must equal
  # the kit's copy; the 64-hex seed goes to the drill CLI's `key import --hex-stdin` through a pipe — never
  # argv, never the environment, never a file other than the 0600 key file the CLI writes. The import is
  # accepted only when the key's address IS TESTNET_MAIN_ADDRESS and this build's premine names that address
  # for testnet-12. Its sha256 goes to mainkey.sha256 (assert_main_key_file).
  local k=$MAIN_KEY a src="$SRC_DIR/consensus/core/src/config/premine.rs"
  [ -x "$CLI" ] || die "no $CLI (build-ship.sh)"
  [ "$(git -C "$SRC_DIR" rev-parse HEAD 2>/dev/null)" = "$SHIP_REV_EXPECTED" ] || die "$SRC_DIR is not checked out at $SHIP_REV_EXPECTED (build-ship.sh)"
  [ -r "$src" ] || die "cannot read $src"
  grep -qF "\"$TESTNET_MAIN_ADDRESS\"" "$src" && grep -qxF "        return TESTNET_MAIN_ADDRESS;" "$src" \
    || die "the built premine.rs does not make TESTNET_MAIN_ADDRESS testnet-12's main wallet — not the drill build 005e5c5f"
  if [ ! -e "$k" ]; then
    python3 - "$src" "$TESTNET_MAIN_SEED_EXPECTED" <<'PY' | "$CLI" --network testnet-12 --output json key import --out "$k" --hex-stdin >> "$FRESH/mainkey.log" 2>&1
import hashlib,re,sys
m=re.search(r'const TESTNET_MAIN_SEED: &\[u8\] = b"([^"\\]+)";', open(sys.argv[1]).read())
if not m: sys.exit("TESTNET_MAIN_SEED is not in " + sys.argv[1])
if m.group(1) != sys.argv[2]: sys.exit("the built tree's TESTNET_MAIN_SEED differs from the kit's copy")
sys.stdout.write(hashlib.blake2b(m.group(1).encode(), digest_size=32).hexdigest())
PY
    local ps=("${PIPESTATUS[@]}")
    [ "${ps[0]}" = 0 ] && [ "${ps[1]}" = 0 ] || { rm -f "$k"; die "mainkey: the seed derivation (rc ${ps[0]}) or \`misaka key import --hex-stdin\` (rc ${ps[1]}) failed ($FRESH/mainkey.log)"; }
    chmod 600 "$k"
    a=$(addr_of "$k")
    [ "$a" = "$TESTNET_MAIN_ADDRESS" ] || { rm -f "$k"; die "mainkey: the imported key's address (${a:-none}) is not TESTNET_MAIN_ADDRESS — removed"; }
    echo "$(sha256sum "$k" | awk '{print $1}') main" > "$FRESH/mainkey.sha256"; chmod 600 "$FRESH/mainkey.sha256"
  fi
  assert_main_key_file "$k"
  echo "$TESTNET_MAIN_ADDRESS" > "$FRESH/main.addr"
  log "mainkey: the drill main wallet $k (0600) is TESTNET_MAIN_ADDRESS (${TESTNET_MAIN_ADDRESS:0:24}…), imported from the public test seed over stdin; it signs only after assert_drill_chain (genesis ${DRILL_GENESIS:0:8}…, premine ${PREMINE:0:8}…)"
}

cmd_stage_art() {
  # Review 2: the kit no longer reads /root/perm-drill at all (HARD RULE: never touch it). The operator
  # places the artifact in $DRILL_ROOT/art by an allowed route (PLAN A2: scp from the Mac); there is no
  # override for this, on purpose.
  die "stage-art is retired: place the 8k artifact and its .palwmanifest in $(dirname "$ART8K") (PLAN step A2), then run \`drill.sh verify-art\`"
}

cmd_verify_art() {
  local E bad=0 f; E=$(ev C0-preflight)
  for f in "$ART8K" "$ART8K.palwmanifest"; do
    art_path_ok "$f" || die "refusing: $f resolves under $FORBIDDEN_TREE (HARD RULE)"
    [ -s "$f" ] || { log "missing $f (PLAN A2)"; bad=1; }
  done
  [ $bad = 0 ] || return 1
  [ "$(stat -c %s "$ART8K")" = "$ART8K_BYTES" ] || { log "$ART8K is $(stat -c %s "$ART8K") bytes, not $ART8K_BYTES"; bad=1; }
  { sha256sum "$ART8K" "$ART8K.palwmanifest"; echo "expected $ART8K_SHA256 / $ART8K_MANIFEST_SHA256"; } > "$E/art8k-sha256.txt"
  [ "$(sha256sum "$ART8K" | awk '{print $1}')" = "$ART8K_SHA256" ] || { log "$ART8K sha256 differs from the Mac copy (ART8K_SHA256)"; bad=1; }
  [ "$(sha256sum "$ART8K.palwmanifest" | awk '{print $1}')" = "$ART8K_MANIFEST_SHA256" ] || { log "the sidecar sha256 differs (ART8K_MANIFEST_SHA256)"; bad=1; }
  grep -q "\"class_id\": \"$C_8K\"" "$ART8K.palwmanifest" || { log "the sidecar does not name class ${C_8K:0:16}…"; bad=1; }
  [ $bad = 0 ] && log "verify-art: $ART8K ($ART8K_BYTES bytes, sha256 ${ART8K_SHA256:0:16}…) + sidecar naming ${C_8K:0:16}…" || return 1
}

# ---- C0 --------------------------------------------------------------------------------------
cmd_preflight() {
  local E; E=$(ev C0-preflight); local bad=0
  for b in "$KASPAD" "$CLI" "$PALW_CLASS"; do [ -x "$b" ] || { log "missing $b (build-ship.sh)"; bad=1; }; done
  [ -s "$BIN_DIR/REV" ] && log "binary REV $(cat "$BIN_DIR/REV")"
  [ "$(cat "$BIN_DIR/REV" 2>/dev/null)" = "$SHIP_REV_EXPECTED" ] || { log "bin/REV is not $SHIP_REV_EXPECTED"; bad=1; }
  (cd "$BIN_DIR" && sha256sum -c SHA256SUMS) > "$E/sha256.txt" 2>&1 || { log "binaries changed since build-ship.sh"; bad=1; }
  for f in --palw-host-memory-share --palw-round-lane --palw-register-bond --palw-canonical-claims --rpclisten-json --ram-scale \
           --palw-chain-classes --palw-producer-pay-address --palw-heartbeat-miner-address --palw-bond-collateral \
           --addpeer --connect --outpeers; do
    "$KASPAD" --help 2>/dev/null | grep -q -- "$f" || { log "kaspad lacks $f"; bad=1; }
  done
  # a clean environment: lib.sh dropped every KASPAD_*/MISAKA_* (except the MISAKA_NETWORK it sets) and
  # kaspad starts under env -i; HOME is the drill's, so no drill process writes root's endpoint registry
  env | grep -E '^(KASPAD_|MISAKA_)' | grep -v '^MISAKA_NETWORK=testnet-12$' > "$E/env-leftovers.txt"
  [ -s "$E/env-leftovers.txt" ] && { log "KASPAD_/MISAKA_ variables survived the scrub ($E/env-leftovers.txt)"; bad=1; }
  [ "$HOME" = "$DRILL_HOME" ] || { log "HOME is $HOME, not $DRILL_HOME"; bad=1; }
  endpoints_fingerprint > "$E/public-endpoints-before.txt"
  log "root's endpoint registry $PUBLIC_ENDPOINTS: $(cat "$E/public-endpoints-before.txt") (recorded; compared by every invariants and by down)"
  # item 4, binary half: the drill salt is in kaspad; no public/retired/first-drill genesis byte string is in either binary
  python3 - "$KASPAD" "$CLI" "$DRILL_SALT" "$DRILL_GENESIS" "$(csv $FORBIDDEN_GENESES)" > "$E/identity-bytes.txt" 2>&1 <<'PY' || { log "binary identity check failed ($E/identity-bytes.txt)"; bad=1; }
import mmap,sys
kaspad,cli,salt,drill,forbidden=sys.argv[1:6]
forbidden=[h for h in forbidden.split(",") if h]
bad=0
for path in (kaspad,cli):
    with open(path,"rb") as f, mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as m:
        has=lambda b: m.find(b)>=0
        d=has(bytes.fromhex(drill)); found=[h[:8] for h in forbidden if has(bytes.fromhex(h))]
        s=has(salt.encode()) if path==kaspad else None
        print(f"{path}: drill-genesis-bytes={d} forbidden-genesis-bytes={found or 'none'} (checked {[h[:8] for h in forbidden]})" + (f" drill-salt={s}" if s is not None else ""))
        if found: bad=1; print("  FORBIDDEN genesis bytes present")
        if s is False: bad=1; print("  the drill premine salt is missing: not the drill build")
        if not d: print("  note: the drill genesis is not stored as one 64-byte run here (the compiler may split a const); C1 decides by RPC")
sys.exit(bad)
PY
  # the checkout build-ship.sh built is the drill build (`drill.sh mainkey` reads TESTNET_MAIN_SEED from it)
  [ "$(git -C "$SRC_DIR" rev-parse HEAD 2>/dev/null)" = "$SHIP_REV_EXPECTED" ] || { log "$SRC_DIR is not checked out at $SHIP_REV_EXPECTED"; bad=1; }
  for i in 0 1 2 3 4 5 6 7; do [ -s "$(key_of "$i")" ] || { log "no key file $(key_of "$i")"; bad=1; }; done
  for n in $DRILL_WALLETS; do [ -s "$(wallet_key "$n")" ] && [ -s "$FRESH/$n.addr" ] || { log "no drill-only wallet $n (drill.sh keygen)"; bad=1; }; done
  if [ -s "$FRESH/card-addresses.txt" ]; then
    for n in $DRILL_WALLETS; do grep -qxF "$(cat "$FRESH/$n.addr" 2>/dev/null)" "$FRESH/card-addresses.txt" && { log "drill wallet $n pays to a card address"; bad=1; }; done
  else log "no $FRESH/card-addresses.txt (drill.sh keygen)"; bad=1; fi
  # every drill wallet is keygen-made and passes the signer rule; the drill main wallet is the imported
  # TESTNET_MAIN_SEED key (its chain proof comes at every spend, not here: no node runs yet); a FUND_KEY
  # override must pass the same rule
  for n in $DRILL_WALLETS; do ( assert_drill_key "$(wallet_key "$n")" ) 2>>"$E/drill-keys.txt" || { log "drill wallet $n fails assert_drill_key ($E/drill-keys.txt)"; bad=1; }; done
  ( assert_main_key_file "$MAIN_KEY" ) 2>>"$E/drill-keys.txt" && echo "drill main wallet $MAIN_KEY = $TESTNET_MAIN_ADDRESS (sha256 in mainkey.sha256)" >> "$E/drill-keys.txt" \
    || { log "the drill main wallet $MAIN_KEY is missing or fails assert_main_key_file — run \`drill.sh mainkey\` ($E/drill-keys.txt)"; bad=1; }
  if [ -e "$FUND_KEY" ]; then ( assert_drill_key "$FUND_KEY" offline ) || { log "FUND_KEY $FUND_KEY fails assert_drill_key"; bad=1; }; fi
  # the artifact: never under /root/perm-drill, the Mac copy's bytes, and the drill build's own manifest check
  art_path_ok "$ART8K" && art_path_ok "$ART8K.palwmanifest" || { log "ART8K resolves under $FORBIDDEN_TREE — refused (HARD RULE)"; bad=1; }
  if art_path_ok "$ART8K"; then
    cmd_verify_art || { log "verify-art failed (PLAN A2)"; bad=1; }
    "$PALW_CLASS" manifest --network testnet-12 --check "$ART8K" > "$E/art8k-manifest-check.txt" 2>&1 || { log "palw-class manifest --check refuses $ART8K"; bad=1; }
  fi
  for i in 0 1 2 3 4 5 6 7 8; do
    for p in "$(p2p "$i")" "$(borsh "$i")" "$(json "$i")"; do
      ss -ltn "( sport = :$p )" | grep -q LISTEN && { log "port $p is in use"; bad=1; }
    done
  done
  pgrep -af -- "--appdir=$RUN/" > /dev/null && { log "a drill node is already running"; bad=1; }
  # the other session's private chain: it must be stopped (coordinated) before a 7-node drill starts
  local t12p; t12p=$(systemctl is-active misaka-t12p-0 2>/dev/null || true)
  [ "$t12p" = active ] && [ "${ALLOW_SHARED:-0}" != 1 ] && { log "misaka-t12p-* is running: coordinate its stop first (CHECKLIST.md §2)"; bad=1; }
  local a; a=$(avail_gib)
  awk -v a="$a" 'BEGIN{exit !(a >= 17)}' || { log "MemAvailable $a GiB < 17 GiB the drill peaks at plus the live floor seats' headroom"; bad=1; }
  df -BG --output=avail "$DRILL_ROOT" | tail -1 | awk '{g=$1+0; exit !(g >= 40)}' || { log "under 40 GB free under $DRILL_ROOT"; bad=1; }
  { date -u; uname -a; nproc; free -g; df -h "$DRILL_ROOT"; uptime; systemctl list-units --no-pager --plain 'misaka*'; } > "$E/host.txt" 2>&1
  [ $bad = 0 ] && pass C0-preflight "drill build $SHIP_REV_EXPECTED, drill salt in kaspad, no public/retired/first-drill genesis bytes, flags, clean env + drill HOME, card keys, keygen-made drill wallets + the imported drill main wallet (TESTNET_MAIN_ADDRESS), artifact (not perm-drill; sha256 + manifest check), ports, memory, disk" \
    || fail C0-preflight "see timeline.log"
}

# ---- C1 --------------------------------------------------------------------------------------
cmd_up() {
  local E; E=$(ev C1-boot)
  # hub B first (it dials nothing), then hub A, whose --addpeer to n3 then connects on its first attempt
  # (a permanent request that failed backs off 30 s · 2^n up to 8 min). The two hubs are each other's
  # peer, so both clocks may mint (the heartbeat miner holds while a node has no peer).
  start_node 3
  start_node 0
  wait_until '[ -n "$(daa_of 0)" ] && [ -n "$(daa_of 3)" ]' "n0 and n3 to answer wRPC" 600
  # item 4: the booted genesis is the DRILL genesis and nothing public — before any leaf joins
  # (retried for up to 10 min: before the first block the PALW state may not have a tip to read bonds from)
  local i
  for i in 0 3; do
    identity_ok "$i" 60; cp "$RUN/identity-n$i.json" "$E/identity-n$i.json"
    python3 -c "import json,sys; sys.exit(0 if json.load(open('$E/identity-n$i.json')).get('ok') else 1)" 2>/dev/null \
      || { fail C1-boot "n$i is not on the drill chain ($E/identity-n$i.json)"; cmd_down; return 1; }
  done
  { ss -ltn "( sport = :$(p2p 0) or sport = :$(p2p 3) )"; } > "$E/hub-listen.txt" 2>&1
  for i in 1 2 4 5; do start_node "$i"; done   # n6 is held back for C7
  wait_until "sinks_agree 1 2 3 4 5" "n0..n5 to share one sink" 1800
  local d0; d0=$(daa_of 0)
  wait_until "[ \"\$(daa_of 0)\" -ge $((d0 + 3)) ]" "the clock to tick 3 DAA ($(eta 3))" "$(daa_wall_s 3)"
  local bad=0
  for i in 1 2 4 5; do identity_ok "$i" 6 || { log "n$i: not the drill chain"; bad=1; }; cp "$RUN/identity-n$i.json" "$E/identity-n$i.json"; done
  # the P2P shape actually formed: both hubs LISTEN, and every leaf reached its hub
  { for i in 0 1 2 3 4 5; do printf 'n%s p2p %s listening=%s\n' "$i" "$(p2p "$i")" "$(p2p_listening "$i" && echo yes || echo no)"; done; } >> "$E/hub-listen.txt"
  for i in 0 3; do p2p_listening "$i" || { fail C1-boot "hub n$i is not listening ($E/hub-listen.txt)"; return 1; }; done
  # the fingerprint, read over RPC from EVERY live node (getPalwNodeStatus.consensusParamsId): ONE value,
  # and none a public testnet-12 node has printed. The startup log lines are kept as a second witness.
  : > "$E/fingerprints.txt"
  for i in 0 1 2 3 4 5; do echo "n$i consensusParamsId $(fp_of "$i")" >> "$E/fingerprints.txt"; done
  grep -hoE "Consensus params fingerprint: [0-9a-f]+ \(network [^)]+\)" "$RUN"/n[0-5].log | sort | uniq -c >> "$E/fingerprints.txt"
  local fps nfp; fps=$(awk '/consensusParamsId/ && NF==3 {print $3}' "$E/fingerprints.txt" | sort -u)
  nfp=$(awk '/consensusParamsId/ && NF==3' "$E/fingerprints.txt" | wc -l | tr -d ' ')
  [ "$nfp" = 6 ] || { fail C1-boot "only $nfp of 6 nodes answered getPalwNodeStatus.consensusParamsId ($E/fingerprints.txt)"; return 1; }
  [ "$(echo "$fps" | grep -c .)" = 1 ] || { fail C1-boot "live nodes report $(echo "$fps" | grep -c .) fingerprints over RPC ($E/fingerprints.txt)"; return 1; }
  grep -q "network testnet-12)" "$E/fingerprints.txt" || { fail C1-boot "the fingerprint log line does not name testnet-12"; return 1; }
  for p in $PUBLIC_FP_DENY; do
    case "$fps" in "$p"*) fail C1-boot "the drill fingerprint $fps is a PUBLIC testnet-12 fingerprint ($p…)"; return 1;; esac
  done
  echo "drill fingerprint $fps (RPC, 6 nodes); not one of: $PUBLIC_FP_DENY" >> "$E/fingerprints.txt"
  # every card's registered key is the key file this drill signs with — on the DRILL premine txid —
  # and no bond sits on the public or sentinel premine txid
  for i in 0 1 2 3 4 5 6 7; do
    local owned
    owned=$(cli 0 --output json bond status --bond "$(bond_of "$i")" --key-file "$(key_of "$i")" 2>/dev/null \
      | python3 -c 'import json,sys; v=json.load(sys.stdin); print(v.get("registered"), v.get("owned_by_supplied_key"))' 2>/dev/null)
    echo "card $i on the drill premine: registered/owned-by-key-file: $owned" >> "$E/cards.txt"
    [ "$owned" = "True True" ] || bad=1
  done
  for p in $FORBIDDEN_PREMINES; do
    local k; k=$(cli 0 --output json bond status --bond "$p:1" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("registered"))' 2>/dev/null)
    echo "bond at ${p:0:16}…:1 registered: $k" >> "$E/cards.txt"
    [ "$k" = "True" ] && bad=1
  done
  # the drill chain's main wallet is TESTNET_MAIN_ADDRESS (the drill commit): it holds the drill premine's
  # :40 until F0 spends it (evidence; F0 re-checks it before its first send)
  local mw="drill main wallet not imported (drill.sh mainkey)"
  if [ -s "$MAIN_KEY" ]; then
    mw="drill main wallet TESTNET_MAIN_ADDRESS holds :$MAIN_PREMINE_INDEX"
    utxo_json "$MAIN_KEY" 20 > "$E/main-wallet.json"
    python3 - "$E/main-wallet.json" "$PREMINE:$MAIN_PREMINE_INDEX" "$TESTNET_MAIN_ADDRESS" <<'PY' >> "$E/cards.txt" || { [ -s "$EV/F0-fund/sends.tsv" ] || bad=1; }
import json,sys
w=json.load(open(sys.argv[1])); rows=w.get("recent") or []
has=any(u.get("outpoint")==sys.argv[2] for u in rows)
print(f"drill main wallet {w.get('address')}: {w.get('total')} outputs, holds {sys.argv[2][:16]}…:{sys.argv[2].rsplit(':',1)[1]}: {has}; bonded {w.get('bonded')}")
sys.exit(0 if has and w.get("address")==sys.argv[3] else 1)
PY
  fi
  # H1: the clocks say whether each beat was granted; an honest miner waits for its slot
  for i in 0 3; do
    printf 'n%s heartbeats: granted=%s holding=%s NOT-granted=%s\n' "$i" \
      "$(grep -c 'heartbeat #[0-9]* [0-9a-f]* — granted' "$(logf "$i")")" \
      "$(grep -c 'holds the open slot, not granted yet' "$(logf "$i")")" \
      "$(grep -c 'NOT granted: stamped before its slot' "$(logf "$i")")" >> "$E/heartbeats.txt"
  done
  # the pace this build actually runs at (clock floor armed at 0 ⇒ ≈ 120–125 s/DAA expected); every
  # ETA the later phases log is computed from it (lib.sh eta). Re-measure any time: drill.sh pace
  if measure_pace 0 "$E/pace.txt"; then tail -1 "$E/pace.txt" > "$RUN/pace.s"; log "C1 pace: $(head -1 "$E/pace.txt")"
  else log "C1 pace: not measurable yet ($(head -1 "$E/pace.txt")); ETAs use 123 s/DAA"; fi
  $CHAINSTATE dag --port "$(json 0)" > "$E/dag.json"
  $CHAINSTATE registry --port "$(json 0)" > "$E/registry.json"
  # item 8: no 6 × 20M MSK validator set exists on this chain, so the DNS overlay stays in Bootstrap
  $CHAINSTATE dns --port "$(json 0)" > "$E/dns.json" 2>&1
  [ $bad = 0 ] || { fail C1-boot "a node is off the drill chain, a card's key/bond does not match, or the drill main wallet does not hold ${PREMINE:0:16}…:$MAIN_PREMINE_INDEX ($E/cards.txt, identity-n*.json)"; return 1; }
  invariants C1 && pass C1-boot "drill genesis ${DRILL_GENESIS:0:16}… on 6 nodes (public/retired/first-drill genesis and premine absent), hubs n0/n3 listening, one RPC fingerprint ${fps:0:16}… (not public), clock ticking ($(head -1 "$E/pace.txt" 2>/dev/null)), cards 0-7 on premine ${PREMINE:0:16}…, $mw"
}

cmd_pace() {
  # the observer's rows (utc, daa0) over the last hour, else n0's granted-heartbeat lines
  local out="$RUN/pace.txt"
  if [ -s "$RUN/observer.tsv" ] && python3 - "$RUN/observer.tsv" > "$out" 2>&1 <<'PY'
import sys
from datetime import datetime
rows=[]
for l in open(sys.argv[1]):
    p=l.rstrip("\n").split("\t")
    if len(p)<2 or not p[1].isdigit(): continue
    try: rows.append((datetime.strptime(p[0],"%Y-%m-%dT%H:%M:%SZ").timestamp(),int(p[1])))
    except ValueError: pass
if len(rows)<2: sys.exit(1)
last=rows[-1][0]; w=[r for r in rows if r[0]>=last-3600]
if len(w)<2 or w[-1][1]<=w[0][1]: sys.exit(1)
s=(w[-1][0]-w[0][0])/(w[-1][1]-w[0][1])
print(f"pace {s:.1f} s/DAA ({s/60:.2f} min/DAA) over DAA {w[0][1]}..{w[-1][1]} from the observer"); print(int(round(s)))
PY
  then :; else measure_pace 0 "$out" || { log "pace: not measurable ($(head -1 "$out"))"; return 1; }; fi
  tail -1 "$out" > "$RUN/pace.s"; log "$(head -1 "$out")"
}

# ---- C2 --------------------------------------------------------------------------------------
LANE_ALARM_RE='\[palw-lane-watch\] (still: )?no PALW work block in the newest'
cmd_alarm() {
  local E; E=$(ev C2-alarm); local m0; m0=$(mark 0)
  # no producer has run yet: the selected chain is heartbeats (and lifecycle carriers riding them). The
  # alarm needs 30 selected-chain blocks with no work — at most two beats a 120 s slot, so ≈ 15 DAA, 30–55 min
  # from genesis. It may already have been logged during `up`; it repeats every 10 min as "still: …".
  wait_until "since 0 $m0 | grep -qE '$LANE_ALARM_RE' || grep -qE '$LANE_ALARM_RE' $(logf 0)" "the lane alarm on n0" "$(daa_wall_s 40)"
  $CHAINSTATE status --port "$(json 0)" > "$E/status-alarmed.json"
  $CHAINSTATE gate --port "$(json 0)" --window-daa 600 --bonds "$(roster_bonds)" --floor "$C_FLOOR" --model "$C_8K" \
    --two-m "$C_2M_PREFIX" --alarm-ports "$(json 0)" --expect-heartbeat-only > "$E/gate-heartbeat-only.json" \
    || { fail C2-alarm "the gate did not refuse a heartbeat-only window ($E/gate-heartbeat-only.json)"; return 1; }
  # producers on: floor on n1 and n3 (canonical free-prompt claims too when the floor is FP-certified)
  local fp=""; [ "$(fp_certified "$C_FLOOR")" = True ] && fp="--palw-canonical-claims --palw-canonical-interval-daa=20"
  echo "floor fpCertified: ${fp:+yes}" > "$E/floor-fp.txt"
  set_extra 1 "--palw-produce $fp"; set_extra 3 "--palw-produce"
  local m1 m3; m1=$(mark 1); m3=$(mark 3); m0=$(mark 0)
  restart_node 1; restart_node 3
  wait_until "since 1 $m1 | grep -q '\[palw-producer\] produced block #' || since 3 $m3 | grep -q '\[palw-producer\] produced block #'" "a floor attempt block" "$(daa_wall_s 15)"
  wait_until "since 0 $m0 | grep -q '\[palw-lane-watch\] PALW work is back on the chain'" "the lane alarm to clear" 3600
  $CHAINSTATE status --port "$(json 0)" > "$E/status-cleared.json"
  python3 -c "import json,sys; s=json.load(open('$E/status-cleared.json')); sys.exit(0 if not s.get('laneAlarm') and s.get('laneWorkBlocks',0)>0 else 1)" \
    || { fail C2-alarm "getPalwNodeStatus still alarms after work returned"; return 1; }
  invariants C2 && pass C2-alarm "heartbeat-only window alarmed (log + RPC + gate refused); first floor attempt cleared it"
}

# ---- C3 / C4 setup, F0 ------------------------------------------------------------------------
# fund <from-key> <to-addr> <MSK> <evidence-file> — a drill wallet only (assert_drill_key: keygen-made, or the
# drill main wallet), and only through a node that proves the drill chain (assert_drill_chain). Prints the
# txid the node accepted; appends the CLI's answer to the evidence file.
fund() {
  assert_drill_key "$1"
  assert_drill_chain 0
  local out rc
  out=$(cli 0 --output json wallet send --key-file "$1" --to "$2" --amount "$3" --yes 2>>"$4"); rc=$?
  printf '%s send %s MSK from %s to %s: rc=%s %s\n' "$(date -u +%FT%TZ)" "$3" "${1##*/}" "$2" "$rc" "$out" >> "$4"
  [ $rc = 0 ] || return 1
  printf '%s\n' "$out" | python3 -c 'import json,sys
for l in reversed(sys.stdin.read().strip().splitlines()):
    try: print(json.loads(l).get("txid") or ""); break
    except ValueError: pass' 2>/dev/null
}

# ---- F0 ----------------------------------------------------------------------------------------
cmd_fund() {
  # The drill chain's main wallet is the public TESTNET_MAIN_SEED key (lib.sh). It may sign here ONLY because
  # this chain's premine txid is salted, so the order is: the CHAIN (n0 booted genesis ${DRILL_GENESIS:0:8}…
  # at DAA 0, bonds on premine ${PREMINE:0:8}…, nothing public), then the KEY (imported, 0600, address =
  # TESTNET_MAIN_ADDRESS), then the MONEY (the wallet's outputs are the drill premine's :40 or drill-chain
  # change — none on a public, sentinel or first-drill premine txid). Sends go one at a time, each confirmed
  # before the next: a second send would reach for the output the first one is spending.
  local E; E=$(ev F0-fund); local x amt need tx sent="" skipped="" snap
  [ -s "$MAIN_KEY" ] || die "no drill main wallet $MAIN_KEY — run \`drill.sh mainkey\` (PLAN A3)"
  for x in x1 x2 x3; do [ -s "$FRESH/$x.key" ] || die "no $FRESH/$x.key (drill.sh keygen)"; done
  assert_drill_chain 0
  cp "$SPEND_GUARD_FILE" "$E/spend-guard-n0-$(date -u +%H%M%S).json"
  assert_drill_key "$MAIN_KEY"
  snap="$E/main-utxos-$(date -u +%H%M%S).json"
  utxo_json "$MAIN_KEY" 50 > "$snap"
  python3 - "$snap" "$TESTNET_MAIN_ADDRESS" "$PREMINE" "$MAIN_PREMINE_INDEX" "$(csv $FORBIDDEN_PREMINES)" \
      "$([ -s "$E/sends.tsv" ] && echo later || echo first)" <<'PY' >> "$E/main-wallet.txt" 2>&1 \
    || die "F0 refused: the drill main wallet's outputs are not (only) the drill premine's ($E/main-wallet.txt)"
import json,sys
w=json.load(open(sys.argv[1])); addr,drill,idx,forb,phase=sys.argv[2],sys.argv[3],sys.argv[4],sys.argv[5].split(","),sys.argv[6]
rows=w.get("recent") or []
on_forbidden=[u["outpoint"] for u in rows if u["outpoint"].rsplit(":",1)[0] in forb]
main=[u for u in rows if u["outpoint"]==f"{drill}:{idx}"]
held=f"held, {int(main[0]['sompi'])/1e8} MSK" if main else "not held"
print(f"F0 ({phase} pass): {w.get('address')} — {w.get('total')} outputs, mature {w.get('mature')}, bonded {w.get('bonded')}; "
      f"{drill[:16]}…:{idx} {held}; outputs on a forbidden premine txid: {on_forbidden or 'none'}")
sys.exit(1 if w.get("address")!=addr or on_forbidden or (phase=="first" and not main) else 0)
PY
  log "$(tail -1 "$E/main-wallet.txt")"
  for x in x1 x2 x3; do
    case $x in x1) amt=$F0_X1_MSK;; x2) amt=$F0_X2_MSK;; *) amt=$F0_X3_MSK;; esac
    need=$(python3 -c "print(round($amt - 0.6, 8))")
    if [ "$x" = x2 ] && [ "${CONTROL:-1}" != 1 ]; then skipped="$skipped x2(CONTROL=0)"; continue; fi
    if [ -s "$FRESH/$x.bond" ]; then skipped="$skipped $x(already bonded)"; continue; fi
    if [ -n "$(utxo_ge "$FRESH/$x.key" "$need")" ]; then skipped="$skipped $x(holds>=$need)"; continue; fi
    # C3a must see x1 UNFUNDED (a floor-exact registration stops at "no confirmed UTXO to spend"), so x1 is
    # funded only once C3a has passed — `drill.sh b1` runs F0 right after its refusals
    if [ "$x" = x1 ] && ! grep -q '^PASS' "$EV/C3-b1/VERDICT" 2>/dev/null; then
      skipped="$skipped x1(after-C3a)"; log "F0: x1 stays unfunded until C3a has run (drill.sh b1 funds it right after its refusals)"; continue
    fi
    tx=$(fund "$MAIN_KEY" "$(wallet_addr "$x")" "$amt" "$E/sends.log") && [ -n "$tx" ] \
      || die "F0: the node did not accept the send of $amt MSK to $x ($E/sends.log)"
    printf '%s\t%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$x" "$amt" "$tx" >> "$E/sends.tsv"
    wait_until "[ -n \"\$(utxo_ge $FRESH/$x.key $need)\" ]" "F0: $x's $amt MSK (tx ${tx:0:16}…) to confirm" "$(daa_wall_s 5)"
    sent="$sent $x=$amt"
    log "F0: $x funded with $amt MSK from the drill main wallet (tx ${tx:0:16}…)"
  done
  utxo_json "$MAIN_KEY" 50 > "$E/main-utxos-after.json"
  for x in x1 x2 x3; do utxo_json "$FRESH/$x.key" 20 > "$E/$x-after.json"; done
  pass F0-fund "from ${PREMINE:0:16}…:$MAIN_PREMINE_INDEX (TESTNET_MAIN_ADDRESS) on the proven drill chain ${DRILL_GENESIS:0:16}…: sent${sent:- nothing}; skipped${skipped:- nothing}"
}

# reg_expect <name> <collateral-sompi> <ERE the refusal must match> <evidence dir> <tag> — run the registrar
# with NO funding outpoint and wait for the panel's own refusal line; a BondRegistered must not follow
reg_expect() {
  local n=$1 coll=$2 re=$3 E=$4 tag=$5 m8 ok=""
  assert_drill_chain 0
  : >> "$(logf 8)"; m8=$(mark 8)
  REG_KEY="$FRESH/$n.key" REG_COLL="$coll" REG_FUND="" start_node 8
  for _ in $(seq 1 36); do
    sleep 10
    if since 8 "$m8" | grep -qE "\[palw-panel\] (still )?cannot register a bond( yet)?.*$re"; then ok=1; break; fi
    since 8 "$m8" | grep -qE "\[palw-panel\] registered bond " && break
  done
  since 8 "$m8" | grep -E "palw-panel\]" | cut -c1-400 > "$E/$n-$tag.log"
  stop_node 8
  [ -n "$ok" ] && ! grep -q "registered bond " "$E/$n-$tag.log"
}

register_fresh() { # register_fresh <name> <collateral-sompi> <evidence dir> — spends the key's own (F0) money
  local n=$1 coll=$2 E=$3 m0 m8
  assert_drill_key "$FRESH/$n.key"
  assert_drill_chain 0
  m0=$(mark 0); : >> "$(logf 8)"; m8=$(mark 8)
  REG_KEY="$FRESH/$n.key" REG_COLL="$coll" REG_FUND="$(utxo_ge "$FRESH/$n.key" "$(python3 -c "print($coll/1e8 + 0.4)")")" start_node 8
  local ok=""
  for _ in $(seq 1 180); do
    sleep 10
    if since 8 "$m8" | grep -qE "\[palw-panel\] registered bond "; then ok=1; break; fi
    if since 0 "$m0" | grep -qE "lifecycle object was dropped.*(registration|BondRegistered|signature|Collateral)"; then break; fi
  done
  since 8 "$m8" | grep -E "palw-panel\]" | cut -c1-400 > "$E/$n-registrar.log"
  since 0 "$m0" | grep -E "PALW lifecycle carried .*BondRegistered|lifecycle object was dropped" | cut -c1-400 > "$E/$n-chain.log"
  stop_node 8
  [ -n "$ok" ] || return 1
  grep -m1 -oE "registered bond [0-9a-f]+:[0-9]+" "$E/$n-registrar.log" | awk '{print $3}' > "$FRESH/$n.bond"
  [ -s "$FRESH/$n.bond" ]
}

cmd_b1() {
  local E; E=$(ev C3-b1); local E4; E4=$(ev C4-cheap-bond)
  for n in x1 x2 x3; do [ -s "$FRESH/$n.key" ] || die "no $FRESH/$n.key (drill.sh keygen)"; done
  # C3a — the floor is 13,000 MSK (a3c5db22) and kaspad applies the fold's own registration floor
  # (palw_bond_registration_floor_v1) BEFORE it looks for money: a cheap bond is refused at registration,
  # so the old C4 premise ("a 0.1 MSK bond exists and must not be drawn") cannot arise any more.
  local floor_re="is below this chain's floor of $PRODUCER_FLOOR_SOMPI sompi"
  reg_expect x1 "$CHEAP_COLLATERAL_SOMPI" "--palw-bond-collateral $CHEAP_COLLATERAL_SOMPI $floor_re" "$E" cheap \
    || { fail C3-b1 "a 0.1 MSK --palw-bond-collateral was not refused against the 13,000 MSK floor ($E/x1-cheap.log)"; return 1; }
  reg_expect x1 "$FLOOR_MINUS_ONE_SOMPI" "--palw-bond-collateral $FLOOR_MINUS_ONE_SOMPI $floor_re" "$E" floor-minus-1 \
    || { fail C3-b1 "floor − 1 sompi was not refused ($E/x1-floor-minus-1.log)"; return 1; }
  if [ ! -s "$FRESH/x1.bond" ] && ! ge_msk "$(mature_msk "$FRESH/x1.key")" 0.00000001; then
    # exactly the floor passes the floor check and stops at the money: the only thing missing is funds
    # (only while x1 holds nothing — a funded x1 would register here for real; F0 funds x1 only after this)
    reg_expect x1 "$PRODUCER_FLOOR_SOMPI" "no confirmed UTXO to spend" "$E" at-floor-unfunded \
      || { fail C3-b1 "a floor-exact bond was refused for something other than missing funds ($E/x1-at-floor-unfunded.log)"; return 1; }
  fi
  # key mode (no --bond) answers bonds_registered_to_this_key / bonds_unanswered (misaka-cli/src/bond.rs ~311),
  # not `registered`: none registered AND every outpoint answered
  cli 0 --output json bond status --key-file "$FRESH/x1.key" > "$E/x1-bond-status-after-refusals.json" 2>&1
  if [ ! -s "$FRESH/x1.bond" ]; then   # (a re-run after C3b registered x1 skips this half)
    python3 -c "import json,sys; v=json.load(open('$E/x1-bond-status-after-refusals.json')); sys.exit(0 if v.get('ok') and v.get('bonds_registered_to_this_key')==[] and v.get('bonds_unanswered')==0 else 1)" 2>/dev/null \
      || { fail C3-b1 "x1 has a registered bond after refusals only, or the registry could not answer for every outpoint ($E/x1-bond-status-after-refusals.json)"; return 1; }
  fi
  pass C3-b1 "C3a: 0.1 MSK and floor−1 sompi refused before any money moved ('$floor_re'); the floor itself passes the check; no bond for x1"
  # C3b and C4 need real money on this chain. F0 moves it from the drill main wallet (${PREMINE:0:16}…:40, the
  # public TESTNET_MAIN_SEED key — it signs only on the proven drill chain) to x1..x3, now that C3a has seen x1
  # unfunded. A card key never sends here.
  local E3b; E3b=$(ev C3b-fresh-bond)
  [ -s "$FRESH/x1.bond" ] && { log "C3b already ran: x1 is bond $(cat "$FRESH/x1.bond") — nothing re-funded"; return 0; }
  if [ -z "$(utxo_ge "$FRESH/x1.key" 13000.4)" ]; then
    if [ -s "$MAIN_KEY" ]; then cmd_fund; else log "no drill main wallet $MAIN_KEY (drill.sh mainkey): F0 cannot run"; fi
  fi
  if [ -z "$(utxo_ge "$FRESH/x1.key" 13000.4)" ]; then
    notreached C3b-fresh-bond "x1 holds no ≥ 13,000.4 MSK output: F0 (drill.sh fund, from the drill main wallet ${PREMINE:0:16}…:$MAIN_PREMINE_INDEX) did not fund it"
    notreached C4-cheap-bond "needs C3b's floor-priced bond"
    return 0
  fi
  register_fresh x1 "$PRODUCER_FLOOR_SOMPI" "$E3b" || { fail C3b-fresh-bond "a fresh key's floor-exact --palw-register-bond did not land ($E3b/x1-*.log)"; return 1; }
  cli 0 --output json bond status --key-file "$FRESH/x1.key" > "$E3b/x1-bond-status.json" 2>&1
  grep -q "$(cut -d: -f1 "$FRESH/x1.bond")" "$E3b/x1-bond-status.json" || { fail C3b-fresh-bond "bond status does not list x1's bond"; return 1; }
  grep -q "dropped" "$E3b/x1-chain.log" && { fail C3b-fresh-bond "the chain dropped a lifecycle object during x1's registration ($E3b/x1-chain.log)"; return 1; }
  pass C3b-fresh-bond "fresh key x1 registered bond $(cat "$FRESH/x1.bond") at exactly 13,000 MSK with the drill build's builder, from F0 money; carried, not dropped"
  # x2 (the control): a bond above the 130,000 MSK seat floor registers too, from F0's 135,001 MSK. It does NOT
  # declare the floor by default (review 2): it runs no node, so a drawn x2 would be one more silent seat on
  # floor panels. CONTROL_DECLARE=1 declares it anyway (then C13's quorum-based verdict still holds, but floor voids rise).
  if [ "${CONTROL:-1}" = 1 ] && [ -n "$(utxo_ge "$FRESH/x2.key" "$(python3 -c "print($CONTROL_COLLATERAL_SOMPI/1e8 + 0.4)")")" ]; then
    register_fresh x2 "$CONTROL_COLLATERAL_SOMPI" "$E4" || log "bond x2 did not register"
  else
    log "x2 skipped: x2 holds less than $((CONTROL_COLLATERAL_SOMPI / 100000000)).4 MSK (F0 did not fund it) or CONTROL=0"
  fi
  local decl="x1"; [ "${CONTROL_DECLARE:-0}" = 1 ] && decl="x1 x2"
  assert_drill_chain 0
  for n in $decl; do
    [ -s "$FRESH/$n.bond" ] || continue
    cli 0 bond capability --key-file "$FRESH/$n.key" --bond "$(cat "$FRESH/$n.bond")" --class-id "$C_FLOOR" --declare "$C_FLOOR" --yes >> "$E4/declare-$n.txt" 2>&1
  done
  wait_until "$CHAINSTATE claims --port $(json 0) --bond $(cat "$FRESH/x1.bond") --role seat | grep -q '$C_FLOOR'" "x1's floor declaration on chain" "$(daa_wall_s 5)"
  $CHAINSTATE claims --port "$(json 0)" --bond "$(cat "$FRESH/x1.bond")" --role seat > "$E4/x1-declared.json"
  daa_of 0 > "$FRESH/x1.declared_daa"   # cheapcheck judges only floor claims bound after this DAA
  echo "x1 $(cat "$FRESH/x1.bond") collateral $PRODUCER_FLOOR_SOMPI (< seat floor $PANEL_FLOOR_SOMPI), floor declared by DAA $(cat "$FRESH/x1.declared_daa"); x2 $(cat "$FRESH/x2.bond" 2>/dev/null) collateral $CONTROL_COLLATERAL_SOMPI, declared: $([ "${CONTROL_DECLARE:-0}" = 1 ] && echo yes || echo no)" > "$E4/bonds.txt"
  log "C4 armed: the floor-priced bond declared the floor; cmd cheapcheck judges it over ≥ ${C4_MIN_DRAWS:-10} floor claims bound after DAA $(cat "$FRESH/x1.declared_daa")"
}

# ---- C7 / C8 ---------------------------------------------------------------------------------
cmd_hold7() {
  local E; E=$(ev C7-hold); local E8; E8=$(ev C8-readiness)
  local g; g=$(grace)
  wait_until "[ \"\$(daa_of 0)\" -gt $g ]" "DAA past the registry's grace ($g)" "$(daa_wall_s $((g + 5)))"
  $CHAINSTATE registry --port "$(json 0)" --class "$C_8K" > "$E/8k-before.json"
  local st; st=$(reg8k_state)
  case "$st" in Probation*|ActiveLimited*|Active) fail C7-hold "the 8k row is already $(reg8k state) with n6 held back — the hold cannot be shown"; return 1;; esac
  local fp=""; [ "$(fp_certified "$C_8K")" = True ] && fp="--palw-canonical-claims --palw-canonical-class=$C_8K --palw-canonical-interval-daa=20"
  echo "8k fpCertified: ${fp:+yes}" > "$E/8k-fp.txt"
  set_extra 2 "--palw-produce --palw-producer-class=$C_8K $fp"
  local m2; m2=$(mark 2); restart_node 2
  # the producer asks the registry first (`registry_holds_class`): a Prefetching row logs
  # "holding: the model registry holds class <id>: Prefetching since span N"; the fold's gate sentence
  # ("…admits no new claim of this class now") is what facts/CLI carry, and the log's form when the row admits
  local hold_re="holding: (the model registry admits no new claim of this class now|the model registry holds class ${C_8K:0:16}[0-9a-f]*: (Prefetching|Candidate|Registered|Held))"
  wait_until "since 2 $m2 | grep -qE '$hold_re'" "n2's E-MODEL-NOT-ADMITTING hold" 3600
  since 2 "$m2" | grep -m3 -E "holding:" | cut -c1-400 > "$E/n2-holding.log"
  $CHAINSTATE facts --port "$(json 2)" --class "$C_8K" --bond "$(bond_of 2)" > "$E/facts.json"
  cli 2 --output json mining status --appdir "$RUN/n2" --key-file "$(key_of 2)" --bond "$(bond_of 2)" --class "$C_8K" > "$E/mining-status.json" 2>&1 || true
  grep -q "E-MODEL-NOT-ADMITTING" "$E/mining-status.json" || { fail C7-hold "misaka mining status does not say E-MODEL-NOT-ADMITTING ($E/mining-status.json)"; return 1; }
  grep -q '"notReadyReason": "the model registry admits no new claim of this class now' "$E/facts.json" || { fail C7-hold "producer facts carry another reason"; return 1; }
  since 2 "$m2" | grep -q "\[palw-producer\] produced block #" && { fail C7-hold "n2 produced while the class was not admitting"; return 1; }
  pass C7-hold "a producer for the $st 8k class holds: log, getPalwProducerFacts and CLI E-MODEL-NOT-ADMITTING"
  # the seventh seat
  m2=$(mark 2); start_node 6
  wait_until "[ \"\$(reg8k readySeatsNow)\" -ge 7 ]" "7 ready 8k seats" "$(daa_wall_s 20)"
  # the RPC state is the Debug rendering: "Probation { probes_passed: 0 }" — match the variant
  wait_until "case \"\$(reg8k_state)\" in Probation*|ActiveLimited*|Active) true;; *) false;; esac" "the 8k row to admit (Probation)" "$(daa_wall_s 10)"
  wait_until "since 2 $m2 | grep -q '\[palw-producer\] produced block #'" "n2's first 8k attempt block" "$(daa_wall_s 20)"
  $CHAINSTATE registry --port "$(json 0)" --class "$C_8K" > "$E/8k-after.json"
  pass C7-hold "hold released by the 7th seat: $(reg8k state), n2 produced an 8k block"
  # C8: every seat proved readiness at 3.5 GiB and its LATEST 8k readiness event is a proof. readiness_note
  # logs once per change (kaspad/src/palw_panel.rs:742), so a transient "no proof" at start (artifact
  # loading, the replay budget briefly held) is not a refusal once a proof follows it. A bond-standing
  # refusal ("no proofs — this bond …", class 0…0) after the last proof is one.
  local bad=0
  $CHAINSTATE registry --port "$(json 0)" > "$E8/registry-readiness.json"
  for i in 0 1 2 3 4 5 6; do
    python3 - "$(logf "$i")" "$C_8K" > "$E8/n$i-proofs.txt" <<'PY'; local rc=$?
import re,sys
path,cls=sys.argv[1],sys.argv[2]
events=[]
for line in open(path,errors="replace"):
    if f"submitted a readiness proof for class {cls}" in line: events.append(("proof",line.rstrip()[:300]))
    elif f"readiness for class {cls}: no proof" in line: events.append(("refused",line.rstrip()[:300]))
    elif re.search(r"readiness for class 0{128}: no proofs — this bond", line): events.append(("standing",line.rstrip()[:300]))
proofs=[e for e in events if e[0]=="proof"]
last_proof=max([k for k,e in enumerate(events) if e[0]=="proof"], default=-1)
after=[e for e in events[last_proof+1:] if e[0] in ("refused","standing")]
before=[e for e in events[:last_proof] if e[0]=="refused"]
print(f"proofs {len(proofs)}; transient no-proof notes before the last proof {len(before)}; refusals after the last proof {len(after)}")
for e in after[-3:]: print("  AFTER:", e[1])
for e in before[-2:]: print("  before (transient):", e[1])
sys.exit(0 if proofs and not after else 1)
PY
    [ $rc = 0 ] || bad=1
    $CHAINSTATE status --port "$(json "$i")" > "$E8/n$i-status.json"
    python3 -c "import json,sys; s=json.load(open('$E8/n$i-status.json')); sys.exit(0 if int(s.get('memoryShareBytes',0))==$SHARE_3_5_GIB and int(s.get('memoryReservedBytes',0))<=$SHARE_3_5_GIB else 1)" || bad=1
  done
  [ $bad = 0 ] && pass C8-readiness "7/7 seats proved 8k readiness at a 3.5 GiB share; each seat's latest 8k readiness event is a proof (transient notes before it counted, not failed)" \
    || fail C8-readiness "a seat never proved, was refused after its last proof, or reports another share ($E8)"
}

# ---- C5 --------------------------------------------------------------------------------------
cmd_restart() {
  local E; E=$(ev C5-restart)
  local pick
  wait_until "has_claims --port $(json 2) --bond $(bond_of 2) --phase panel_bound --class $C_8K" "a bound 8k claim of bond 2" "$(daa_wall_s 30)"
  pick=$($CHAINSTATE claims --port "$(json 2)" --bond "$(bond_of 2)" --phase panel_bound --class "$C_8K" | python3 -c '
import json,sys
rows=sorted(json.load(sys.stdin)["claims"], key=lambda r: r.get("acceptedDaa",0))
print(rows[-1]["claimId"], " ".join(rows[-1].get("seats") or []))')
  local claim=${pick%% *} seats=${pick#* } seat=""
  for s in $seats; do i=${s##*:}; [ "${s%:*}" = "$PREMINE" ] && [ "$i" != 2 ] && [ "$i" -le 6 ] && alive "$i" && { seat=$i; break; }; done
  [ -n "$seat" ] || { fail C5-restart "claim $claim draws no running genesis seat"; return 1; }
  echo "claim $claim seats $seats restarted-seat n$seat" > "$E/pick.txt"
  declare -A M; for i in 0 1 2 3 4 5 6; do M[$i]=$(mark "$i"); done
  stop_node 2; stop_node "$seat"; start_node "$seat"; start_node 2
  wait_until "case \"\$(claim_phase 0 $(bond_of 2) $claim)\" in receipt_licensed|final) true;; *) false;; esac" "claim ${claim:0:16} to license after the restarts" "$(daa_wall_s 40)"
  # n2 is back: it produces again — or, the 8k row being HELD at c8652a97 (lib.sh C_8K_MAX_INFLIGHT: at most 5
  # in flight, each until Final), it holds at that cap with the producer's own room sentence. Either line,
  # logged after the restart, is the producer loop alive and asking the fold's gate.
  wait_until "since 2 ${M[2]} | grep -qE \"\$C5_BACK_RE\"" "n2 to produce again (or to hold at the held 8k row's in-flight cap)" "$(daa_wall_s 40)"
  since 2 "${M[2]}" | grep -m1 -E "$C5_BACK_RE" | cut -c1-400 > "$E/n2-back.log"
  local back="n2 produces again"
  grep -q "produced block #" "$E/n2-back.log" || back="n2 is back and holds at the held 8k row's cap ($(reg8k inflightNow) of $C_8K_MAX_INFLIGHT in flight until Final)"
  local defaulted=0; for i in 0 1 2 3 4 5 6; do since "$i" "${M[$i]}" | grep -q "ProducerDefaulted.*${claim:0:16}" && defaulted=1; done
  since "$seat" "${M[$seat]}" | grep -E "for claim ${claim:0:16}|$claim" | cut -c1-300 > "$E/seat-after-restart.log"
  $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of 2)" --terminal > "$E/claims-bond2.json"
  [ $defaulted = 0 ] || { fail C5-restart "a ProducerDefaulted names the restarted producer's claim"; return 1; }
  invariants C5 && pass C5-restart "8k producer n2 and drawn seat n$seat restarted mid-claim; ${claim:0:16} licensed, $back, no default"
}
# C5: what "n2 is back" looks like after its restart (quoted through a variable: the sentences carry apostrophes)
C5_BACK_RE="\[palw-producer\] produced block #|holding: .*(has no panel room left in the network's verification budget|claims in flight against the registry's cap of|the panel has no room for a claim of class)"

# ---- C13: floor licence liveness (item 10a) ----------------------------------------------------
cmd_licence() {
  # Item 10a is about floor claims whose seats all filed Valid. The VERDICT is therefore taken over the
  # bound floor claims whose running seats filed ≥ LIC_QUORUM (3) Valid receipts (counted per claim, one
  # per node, from the seats' own "filed a \"Valid\" [V3 ]receipt for claim <id>" lines): of those, ≥ 95 %
  # must license. A claim with fewer Valid receipts is structural (card 7 runs no node; C10's silenced
  # seats; a drawn x2) and is reported, not judged. The all-bound ratio is reported beside it.
  # This build carries the licence-stall fix (a4dfe903: a late full seat still licenses, coverage forms, a
  # re-bound panel is re-judged; d94d3a1b: a rider that cannot post its lock is passed over, the V3 pool is
  # capped at RECEIPTS_V3_MAX_CLAIMS): C13 is EXPECTED TO PASS. A FAIL here is a finding against c8652a97.
  local E; E=$(ev "C13-licence-$(date -u +%H%M)")
  local W=${LIC_WINDOW_DAA:-120} G=${LIC_GRACE_DAA:-20}
  $CHAINSTATE licence --port "$(json 0)" --bonds "$(roster_bonds)" --class "$C_FLOOR" --window-daa "$W" --grace-daa "$G" \
    --min-ratio 0 --min-bound 0 > "$E/licence.json" || true
  python3 - "$E" "$PREMINE" "$RUN" "$LIC_QUORUM" "${LIC_MIN_RATIO:-0.95}" "${LIC_MIN_BOUND:-10}" \
      "$(cat "$FRESH/x1.bond" 2>/dev/null)" "$(cat "$FRESH/x2.bond" 2>/dev/null)" <<'PY' > "$E/stuck-report.txt"; local rc=$?
import json,os,re,sys
E,premine,run=sys.argv[1:4]; Q=int(sys.argv[4]); min_ratio=float(sys.argv[5]); min_bound=int(sys.argv[6])
named={b:n for b,n in ((sys.argv[7],"x1"),(sys.argv[8],"x2")) if b}
v=json.load(open(f"{E}/licence.json"))
bound=v.get("boundClaims") or []
ids={r["claimId"] for r in bound}
short={cid[:16]:cid for cid in ids}
valid={cid:set() for cid in ids}       # nodes that filed a Valid receipt for the claim
hits={cid:{} for cid in ids}           # every log line naming the claim, per node
lic_sub={cid:[] for cid in ids}        # licence objects submitted for the claim
rx_valid=re.compile(r'filed a "?Valid"? (?:V3 )?receipt for claim ([0-9a-f]{16,})')
rx_claim=re.compile(r'claim ([0-9a-f]{16,})')
for i in range(0,9):
    p=f"{run}/n{i}.log"
    if not ids or not os.path.exists(p): continue
    with open(p,errors="replace") as f:
        for line in f:
            if "claim " not in line: continue
            m=rx_claim.search(line)
            if not m: continue
            cid=m.group(1); cid=cid if cid in ids else short.get(cid[:16])
            if not cid: continue
            hits[cid].setdefault(i,[]).append(line.rstrip()[:300])
            if rx_valid.search(line): valid[cid].add(i)
            if "submitted" in line and "Licensed" in line: lic_sub[cid].append(line.rstrip()[:300])
LIC=("receipt_licensed","final")
eligible=[r for r in bound if len(valid[r["claimId"]])>=Q]
elig_lic=[r for r in eligible if r.get("phase") in LIC]
all_lic=[r for r in bound if r.get("phase") in LIC]
ratio=(len(elig_lic)/len(eligible)) if eligible else None
all_ratio=(len(all_lic)/len(bound)) if bound else None
def seat_tag(s):
    if s in named: return f"{named[s]} (drill bond, runs no node)"
    tx,idx=s.rsplit(":",1)
    if tx!=premine: return f"{s[:12]}…:{idx} (unknown bond)"
    n=int(idx)
    if n>6: return f"card {n} (runs no node)"
    try: up=os.path.exists(f"{run}/n{n}.pid") and os.kill(int(open(f"{run}/n{n}.pid").read()),0) is None
    except Exception: up=False
    return f"card {n} = n{n} ({'running' if up else 'NOT running'})"
print(f"tip {v['tipDaa']}  bound in DAA {v['boundWindow']}: {len(bound)}  licensed {len(all_lic)}  all-bound ratio {all_ratio}")
print(f"VERDICT SET: bound claims with >= {Q} Valid receipts filed: {len(eligible)}  licensed {len(elig_lic)}  ratio {ratio}  (need >= {min_ratio} over >= {min_bound})")
print(f"observed receipt window (deadline-bound): {v['observedReceiptWindowDaa']}  challenge window (deadline-licence): {v['observedChallengeWindowDaa']}")
stuck_q=[]
for r in sorted(bound,key=lambda r:r.get("boundDaa") or 0):
    cid=r["claimId"]
    if r.get("phase") in LIC: continue
    q=len(valid[cid])>=Q
    if q: stuck_q.append(cid[:16])
    print(f"\n{'STUCK-WITH-QUORUM' if q else 'short of quorum'} {cid[:16]}… phase {r.get('phase')} executor {r.get('executorBond','')[-4:]} accepted {r.get('acceptedDaa')} bound {r.get('boundDaa')} deadline {r.get('deadlineDaa')} fp={r.get('isFreePrompt')} Valid filed by nodes {sorted(valid[cid])}")
    for s in r.get("seats") or []:
        tx,idx=s.rsplit(":",1); n=int(idx) if tx==premine else None
        print(f"  seat {seat_tag(s)}: Valid filed={n in valid[cid] if n is not None else False}")
        for l in (hits[cid].get(n,[]) if n is not None else [])[-3:]: print(f"      {l}")
    print(f"  licence object submitted by any node: {bool(lic_sub[cid])}")
    for l in lic_sub[cid][-2:]: print(f"      {l}")
print(f"\nclaims with >= {Q} Valid receipts filed and no licence: {stuck_q or 'none'}")
json.dump({"quorum":Q,"eligible":len(eligible),"eligibleLicensed":len(elig_lic),"ratio":ratio,"bound":len(bound),
           "boundLicensed":len(all_lic),"allBoundRatio":all_ratio,"stuckWithQuorum":stuck_q},open(f"{E}/verdict.json","w"),indent=1)
if len(eligible)<min_bound: sys.exit(2)
sys.exit(0 if ratio is not None and ratio>=min_ratio else 1)
PY
  head -3 "$E/stuck-report.txt" | while read -r l; do log "C13: $l"; done
  local summ; summ=$(python3 -c "import json; v=json.load(open('$E/verdict.json')); print(f\"{v['eligibleLicensed']}/{v['eligible']} claims with >= {v['quorum']} Valid licensed (ratio {v['ratio']}); all bound {v['boundLicensed']}/{v['bound']} (ratio {v['allBoundRatio']})\")" 2>/dev/null)
  case $rc in
    0) pass "${E##*/}" "$summ ≥ ${LIC_MIN_RATIO:-0.95} over DAA $(python3 -c "import json; print(json.load(open('$E/licence.json'))['boundWindow'])")" ;;
    2) notreached "${E##*/}" "fewer than ${LIC_MIN_BOUND:-10} bound floor claims reached $LIC_QUORUM Valid receipts in the window ($summ); run it again later" ;;
    *) fail "${E##*/}" "$summ — below ${LIC_MIN_RATIO:-0.95}; $E/stuck-report.txt names every STUCK-WITH-QUORUM claim and its seats" ;;
  esac
}

# ---- C14: the panel room rule (item 10b) -------------------------------------------------------
cmd_room() {
  local E; E=$(ev C14-panel-room); local n=${ROOM_SAMPLES:-20} i
  for i in $(seq 1 "$n"); do
    $CHAINSTATE room --port "$(json 0)" --class "$C_8K" --bond "$(bond_of 2)" >> "$E/samples.jsonl" 2>>"$E/errors.txt"
    sleep "${ROOM_EVERY_S:-60}"
  done
  # the observer's samples over the whole run count too
  [ -s "$RUN/room.jsonl" ] && cp "$RUN/room.jsonl" "$E/observer-room.jsonl"
  grep -h "held: overloaded (" "$RUN"/n*.log | cut -c1-300 > "$E/overloaded-log-lines.txt" 2>/dev/null
  # THE consequence of a disagreement: an attempt that passed the producer's room pre-check and that the
  # fold refuses — PanelRoomExhausted, or for the HELD 8k row ClassInflightCapped (c8652a97 holds a held
  # class to its static cap past the fence; op 186's panelRoom folds that cap in, so the producer's pre-check
  # sees it). A chain block's own refused attempt logs "disqualified from virtual chain (PALW state): …"
  # (processor.rs:2172); a merged one "PALW: merged blue … carried work this chain point refused …: …"
  # (processor.rs:2154). Both must be 0, for either sentence.
  grep -hE "(disqualified from virtual chain \(PALW [a-z ]+\):|carried work this chain point refused).*(the panel has no room for a claim of class|claims in flight against the registry's cap of)" \
    "$RUN"/n*.log | cut -c1-400 > "$E/room-refused-attempts.txt" 2>/dev/null
  python3 - "$E" <<'PY' || { fail C14-panel-room "see $E/verdict.txt"; return 1; }
import collections,json,sys
E=sys.argv[1]
rows=[]
for p in [f"{E}/observer-room.jsonl", f"{E}/samples.jsonl"]:
    try: rows+=[json.loads(l) for l in open(p) if l.strip().startswith("{")]
    except FileNotFoundError: pass
rows.sort(key=lambda r: r.get("utc",""))
app=[r for r in rows if r.get("applicable")]
# facts are read at the candidate's DAA, the registry at the tip's: judge agreement only on samples whose
# offset (factsDaa − registryTipDaa) is the run's usual one, so a one-span skew near room 0 is not a finding
offs=collections.Counter(r.get("factsMinusTipDaa") for r in app)
mode=offs.most_common(1)[0][0] if offs else None
judged=[r for r in app if r.get("factsMinusTipDaa")==mode]
dis=[r for r in judged if r.get("roomAgreesWithGate") is False]
persistent=[b for a,b in zip(judged,judged[1:]) if a.get("roomAgreesWithGate") is False and b.get("roomAgreesWithGate") is False]
rep=[r for r in rows if r.get("inflightReplayAgrees") is not None]
rep_bad=[r for r in rep if r.get("inflightReplayAgrees") is False]
over=[o for r in rows for o in r.get("overloadedReasons") or []]
horizon=sorted({r.get("panelHorizonSpans") for r in rows if r.get("workTargetShadow")})
zero=[r for r in judged if r.get("panelRoom")==0]
by_kind=collections.Counter(r.get("gateRefusal") or "admits" for r in judged)
nover=sum(1 for _ in open(E+'/overloaded-log-lines.txt'))
nref=sum(1 for _ in open(E+'/room-refused-attempts.txt'))
out=[f"samples {len(rows)}, applicable (8k admitting, sink stable, gate unmasked) {len(app)}, offsets factsDaa-tipDaa {dict(offs)} -> judged at offset {mode}: {len(judged)}, room==0 in {len(zero)}",
     f"room disagrees with the gate: {len(dis)} (persistent: {len(persistent)}); the gate's word on judged samples: {dict(by_kind)} (cap = the held row at its in-flight cap)",
     f"gate's inflight replay vs op 186 panelInflightReplay (a span's demand) x the gate's window, ceil, at equal DAA: {len(rep)} compared, {len(rep_bad)} differ",
     f"attempts the fold refused for panel room or the held cap (PALW state / merged-blue refusals): {nref}",
     f"Held reasons saying 'overloaded (… utilization …)': {len(over)} {over[:3]}",
     f"panelHorizonSpans seen (rate rule ⇒ 1): {horizon}",
     f"overloaded lines in node logs: {nover}"]
for r in dis[:5]: out.append(f"  disagree @tip {r['registryTipDaa']} facts {r['factsDaa']}: room {r['panelRoom']} / {r['notReadyReason'][:200]}")
for r in rep_bad[:3]: out.append(f"  replay differs @tip {r['registryTipDaa']}: gate {r['gateInflightReplay']} over {r.get('gateWindowSpans')} spans vs op186 {r['panelInflightReplay']} a span")
open(f"{E}/verdict.txt","w").write("\n".join(out)+"\n"); print("\n".join(out))
ok = len(judged)>0 and not persistent and not rep_bad and nref==0 and not over and horizon in ([], [1]) and nover==0
sys.exit(0 if ok else 1)
PY
  pass C14-panel-room "$(tr '\n' ';' < "$E/verdict.txt")"
}

# ---- C15: a second class passing audit does not hold the first (item 10c) ----------------------
cmd_second() {
  local E; E=$(ev C15-second-class)
  [ -s "$RUN/classes.tsv" ] || { notreached C15-second-class "no observer class log ($RUN/classes.tsv)"; return 0; }
  python3 - "$RUN/classes.tsv" "${C_8K:0:8}" "${C_FLOOR:0:8}" > "$E/verdict.txt" <<'PY'; local rc=$?
import sys
path,k8,floor=sys.argv[1:4]
rows=[]
for l in open(path):
    p=l.rstrip("\n").split("\t")
    if len(p)<3 or not p[1].isdigit(): continue
    cells={}
    for c in p[2].split():
        # chainstate classes_line writes the lifecycle VARIANT only ("Probation", never "Probation { … }");
        # a cell that does not parse (an older observer row) is skipped, not fatal
        if "=" not in c: continue
        k,v=c.split("=",1); f=v.split("/")
        if len(f)<3: continue
        cells[k]={"state":f[0].split("{")[0],"inflight":f[1],"room":f[2]}
    rows.append((int(p[1]),cells,p[3] if len(p)>3 else ""))
events=[]
for (d0,a,_),(d1,b,_) in zip(rows,rows[1:]):
    for k,v in b.items():
        if k in (k8,floor): continue
        if v["state"]=="Probation" and a.get(k,{}).get("state") not in ("Probation","ActiveLimited","Active"):
            events.append((d1,k,(b.get(k8) or a.get(k8) or {}).get("inflight")))   # the first class's inflight at the event
if not events:
    print("NOT-REACHED: no second model class entered Probation in this run (the 2M row cannot be seated; no Candidate was registered)"); sys.exit(2)
bad=[]
for d,k,infl in events:
    after=[c.get(k8,{}).get("state") for dd,c,_ in rows if d<=dd<=d+10]
    print(f"DAA {d}: class {k} entered Probation with 8k inflight {infl}; 8k states over the next 10 DAA: {sorted(set(after))}")
    if "Held" in after and infl not in (None,"0"): bad.append((d,k))
print("the first class was held by the second's audit:", bad or "never"); sys.exit(1 if bad else 0)
PY
  case $rc in
    0) pass C15-second-class "$(tail -1 "$E/verdict.txt")" ;;
    2) notreached C15-second-class "$(head -1 "$E/verdict.txt")" ;;
    *) fail C15-second-class "see $E/verdict.txt" ;;
  esac
}

# ---- C16: coinbase maturity is 600 DAA by DAA alone (item 8) ----------------------------------
cmd_maturity() {
  # The verdict is the NODE's: a --coinbase-only send of a drill-only coinbase ≥ 600 DAA old is accepted
  # and carried by a block (its output appears in the utxoindex). The wallet's `mature` flag is the CLI's
  # own arithmetic (misaka-cli/src/wallet.rs:333) and is recorded as evidence only. The negative half — the
  # node refusing a coinbase younger than 600 DAA — cannot be built with this CLI (it never selects an
  # immature output and has no raw-input send), so it is NOT a PASS item here.
  local E; E=$(ev "C16-coinbase-maturity-$(date -u +%H%M)")
  local v; v=$(daa_of 0)
  for n in pay-1 pay-3 pay-2 hb-0 hb-3; do utxo_json "$FRESH/$n.key" "${MAT_RECENT:-3000}" > "$E/$n.json"; done
  python3 - "$E" "$v" "$COINBASE_MATURITY_DAA" <<'PY' > "$E/cli-arithmetic.txt"
import glob,json,sys
E,v,m=sys.argv[1],int(sys.argv[2]),int(sys.argv[3])
best=None; notes=[]
for p in sorted(glob.glob(f"{E}/*.json")):
    name=p.rsplit('/',1)[1][:-5]
    try: w=json.load(open(p))
    except Exception: continue
    cb=[u for u in w.get("recent") or [] if u.get("coinbase")]
    ages=[v-int(u.get("blockDaaScore",0)) for u in cb]
    old=[u for u in cb if v-int(u.get("blockDaaScore",0))>=m+2 and u.get("mature") and not u.get("bonded") and not u.get("reserved")]
    old_sompi=sum(int(u.get("sompi",0)) for u in old)
    early=[(u["outpoint"][:20],v-int(u["blockDaaScore"])) for u in cb if u.get("mature") and v-int(u["blockDaaScore"])<m]
    late=[(u["outpoint"][:20],v-int(u["blockDaaScore"])) for u in cb if not u.get("mature") and v-int(u["blockDaaScore"])>=m+2]
    print(f"{name}: coinbase outputs listed {len(cb)} (of {w.get('total')}), oldest age {max(ages) if ages else None}, "
          f"≥{m+2}-DAA mature {len(old)} = {old_sompi/1e8} MSK; CLI says mature <{m}: {early[:3] or 'none'}; immature ≥{m+2}: {late[:3] or 'none'}")
    if early or late: notes.append(name)
    if old_sompi >= 110_000_000 and (best is None or max(ages) > best[1]): best=(name, max(ages))
print("CLI-arithmetic anomalies (informational):", notes or "none")
print(f"{best[0]} {best[1]}" if best else "none")
PY
  local pick; pick=$(tail -1 "$E/cli-arithmetic.txt")
  if [ "$pick" = none ]; then
    notreached "${E##*/}" "no drill-only wallet holds ≥ 1.1 MSK of coinbase ≥ $((COINBASE_MATURITY_DAA + 2)) DAA old yet (virtual DAA $v; first drill coinbase + 600 ≈ soak S4)"; return 0
  fi
  local w=${pick%% *} age=${pick#* } key tx
  key=$(wallet_key "$w"); assert_drill_key "$key"; assert_drill_chain 0
  tx=$(cli 0 --output json wallet send --key-file "$key" --coinbase-only --to "$(wallet_addr x3)" --amount 1.00000016 --yes 2>>"$E/send.txt" \
    | tee -a "$E/send.txt" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("txid",""))' 2>/dev/null)
  echo "send from $w (oldest coinbase age $age) at DAA $(daa_of 0): txid=$tx" >> "$E/send.txt"
  [ -n "$tx" ] || { fail "${E##*/}" "the node did not accept a --coinbase-only send of coinbase the CLI calls ≥ 600 DAA old ($E/send.txt)"; return 1; }
  wait_until "utxo_json $FRESH/x3.key 200 | python3 -c 'import json,sys; sys.exit(0 if any(u[\"outpoint\"].startswith(\"$tx:\") and int(u.get(\"blockDaaScore\",0))>0 for u in json.load(sys.stdin).get(\"recent\") or []) else 1)'" \
    "the coinbase-only send ${tx:0:16} to be carried by a block" "$(daa_wall_s 5)"
  utxo_json "$FRESH/x3.key" 200 > "$E/x3-after.json"
  pass "${E##*/}" "node accepted and a block carried a --coinbase-only send from $w (coinbase ≥ $COINBASE_MATURITY_DAA DAA old, oldest $age); txid ${tx:0:16}…; the negative half (refusing a younger coinbase) is CLI arithmetic only"
}

# ---- C10 setup -------------------------------------------------------------------------------
cmd_silence() {
  local E; E=$(ev C10-silence)
  # the held 8k row admits at most C_8K_MAX_INFLIGHT (5) claims, each until Final: once five are in flight a
  # FRESH bound 8k claim exists only after an 8k Final frees a slot (≈ the first 8k licence + 120 DAA) — so
  # this runs in the soak, after `c2watch` (PLAN S2)
  log "C10: waiting for a bound 8k claim (8k in flight now: $(reg8k inflightNow) of $C_8K_MAX_INFLIGHT, state $(reg8k state))"
  wait_until "has_claims --port $(json 2) --bond $(bond_of 2) --phase panel_bound --class $C_8K" "a fresh bound 8k claim" "$(daa_wall_s 30)"
  local pick; pick=$($CHAINSTATE claims --port "$(json 2)" --bond "$(bond_of 2)" --phase panel_bound --class "$C_8K" | python3 -c '
import json,sys
rows=sorted(json.load(sys.stdin)["claims"], key=lambda r: r.get("acceptedDaa",0))
print(rows[-1]["claimId"], " ".join(rows[-1].get("seats") or []))')
  local claim=${pick%% *} seats=${pick#* } chosen=()
  for s in $seats; do i=${s##*:}; [ "${s%:*}" = "$PREMINE" ] && [ "$i" != 2 ] && [ "$i" != 0 ] && [ "$i" != 3 ] && [ "$i" -le 6 ] && chosen+=("$i"); done
  [ ${#chosen[@]} -ge 3 ] || { log "claim ${claim:0:16}'s panel has < 3 non-hub genesis seats; retry on the next claim"; return 2; }
  chosen=("${chosen[@]:0:3}")
  echo "$claim" > "$E/claim"; echo "${chosen[*]}" > "$E/silenced"
  for i in "${chosen[@]}"; do touch "$RUN/n$i.noart"; restart_node "$i"; done
  $CHAINSTATE slashed --port "$(json 0)" --bonds "$(roster_bonds)" > "$E/slashed-before.json"
  $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of 2)" --terminal > "$E/claims-before.json"
  log "C10: claim ${claim:0:16} can reach at most 2 receipts; seats ${chosen[*]} lost the 8k artifact (floor duties kept). Next: reorg timeout (twice, ≈ bind + 600 and + 1,200 DAA)"
}

cmd_unsilence() {
  local E; E=$(ev C10-silence)
  for i in $(cat "$E/silenced" 2>/dev/null); do rm -f "$RUN/n$i.noart"; restart_node "$i"; done
}

# ---- C9 reorg --------------------------------------------------------------------------------
# trigger_daa MODE → the DAA the partition must straddle (empty = straddle nothing in particular)
trigger_daa() {
  case "$1" in
    span|exec) echo "" ;;
    final) # a claim that licensed and is within LEAD DAA of its Final (the 120-DAA short challenge window)
      $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of "${FINAL_BOND:-1}")" --phase receipt_licensed | python3 -c "
import json,sys
v=json.load(sys.stdin); t=int(v['tipDaa'] or 0)
c=[r for r in v['claims'] if r.get('deadlineDaa') and 1 <= r['deadlineDaa']-t <= ${LEAD:-3}]
print(min(c, key=lambda r: r['deadlineDaa'])['deadlineDaa'] if c else '')" ;;
    timeout) # the silenced claim's current window end
      local claim; claim=$(cat "$EV/C10-silence/claim")
      $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of 2)" --terminal | python3 -c "
import json,sys
v=json.load(sys.stdin); t=int(v['tipDaa'] or 0)
r=[r for r in v['claims'] if r['claimId']=='$claim']
d=r[0].get('deadlineDaa') if r else None
print(d if d and 1 <= d-t <= ${LEAD:-3} else '')" ;;
  esac
}

cmd_reorg() {
  local mode=${1:?span|final|timeout|exec} tag; tag="C9-reorg-$mode-$(date -u +%H%M)"
  local E; E=$(ev "$tag"); local PART_DAA=${PART_DAA:-4}
  if [ "$mode" = exec ]; then
    grep -qh "\[palw-round-lane\] chain block .* [1-9][0-9]* permit(s) granted" "$RUN"/n0.log || die "reorg exec needs a live execution lane (run route first)"
  fi
  local at=""
  if [ "$mode" = final ] || [ "$mode" = timeout ]; then
    wait_until "[ -n \"\$(trigger_daa $mode)\" ]" "a $mode deadline within ${LEAD:-3} DAA" 864000
    at=$(trigger_daa "$mode")
  fi
  local bonds; bonds=$(roster_bonds)
  # item 8: t12's DNS overlay (6 × 20M MSK validators) is not formed on this chain, so it stays in
  # Bootstrap and its reorg gate is NOT enforced — the reorg below is bounded by PALW finality alone
  $CHAINSTATE dns --port "$(json 0)" > "$E/dns-pre.json" 2>&1
  $CHAINSTATE slashed --port "$(json 0)" --bonds "$bonds" > "$E/slashed-pre-split.json"
  declare -A M; for i in 0 1 2 3 4 5 6; do M[$i]=$(mark "$i"); done
  local split; split=$(daa_of 0)
  echo "split at DAA $split${at:+ (straddling DAA $at)}" > "$E/fork.txt"
  log "$tag: partition (n0 restarts without its --addpeer to n3) at DAA $split${at:+, straddling DAA $at}"
  restart_node 0 partition
  wait_until '[ -n "$(daa_of 0)" ]' "n0 to answer after the split" 900
  local da db; da=$(daa_of 0); db=$(daa_of 3)
  local target_a=$((da + PART_DAA)) target_b=$((db + PART_DAA))
  if [ -n "$at" ]; then target_a=$(( at + 2 > target_a ? at + 2 : target_a )); target_b=$(( at + 2 > target_b ? at + 2 : target_b )); fi
  wait_until "[ \"\$(daa_of 0)\" -ge $target_a ] && [ \"\$(daa_of 3)\" -ge $target_b ]" "both sides past DAA $target_a/$target_b" "$(daa_wall_s $((PART_DAA + ${LEAD:-3} + 4)))"
  local sa sb; sa=$(sink_of 0); sb=$(sink_of 3)
  [ "$sa" != "$sb" ] || { fail "$tag" "the two sides share a sink — no fork"; restart_node 0; return 1; }
  $CHAINSTATE slashed --port "$(json 0)" --bonds "$bonds" > "$E/slashed-A.json"
  $CHAINSTATE slashed --port "$(json 3)" --bonds "$bonds" > "$E/slashed-B.json"
  $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of "${FINAL_BOND:-1}")" --terminal > "$E/claims-A.json"
  $CHAINSTATE claims --port "$(json 3)" --bond "$(bond_of "${FINAL_BOND:-1}")" --terminal > "$E/claims-B.json"
  $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of 2)" --terminal > "$E/claims2-A.json"
  $CHAINSTATE claims --port "$(json 3)" --bond "$(bond_of 2)" --terminal > "$E/claims2-B.json"
  echo "A $sa DAA $(daa_of 0)  B $sb DAA $(daa_of 3)" >> "$E/fork.txt"
  log "$tag: fork A ${sa:0:16}… B ${sb:0:16}…; rejoining"
  restart_node 0
  wait_until '[ -n "$(daa_of 0)" ]' "n0 to answer after the join" 900
  wait_until "sinks_agree 1 2 3 4 5 6" "all 7 nodes on one sink" 7200
  local ra rb
  ra=$(cli 0 --output json node dag-info --chain-from "$sa" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("removed_chain_blocks") or 0)')
  rb=$(cli 0 --output json node dag-info --chain-from "$sb" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("removed_chain_blocks") or 0)')
  echo "removed since A's sink: $ra; since B's sink: $rb" >> "$E/fork.txt"
  local winner
  if [ "${ra:-0}" -gt 0 ] && [ "${rb:-0}" = 0 ]; then winner=B; elif [ "${rb:-0}" -gt 0 ] && [ "${ra:-0}" = 0 ]; then winner=A
  else fail "$tag" "no side's selected chain was replaced (A $ra, B $rb): the join merged, it did not reorganise"; return 1; fi
  sleep 60
  $CHAINSTATE slashed --port "$(json 0)" --bonds "$bonds" > "$E/slashed-post.json"
  $CHAINSTATE dns --port "$(json 0)" > "$E/dns-post.json" 2>&1
  # no double slash / forfeit: every bond's slashed and collateral after the join are the WINNER's, never A+B
  python3 - "$E" "$winner" <<'PY' || { fail "$tag" "a bond's slashed/collateral after the join is not the winning side's ($E)"; return 1; }
import json,sys
E,w=sys.argv[1],sys.argv[2]
win=json.load(open(f"{E}/slashed-{w}.json"))["bonds"]; post=json.load(open(f"{E}/slashed-post.json"))["bonds"]
pre=json.load(open(f"{E}/slashed-pre-split.json"))["bonds"]
bad=[b for b in win if (post[b]["slashed"], post[b]["collateral"]) != (win[b]["slashed"], win[b]["collateral"])]
moved=[b for b in win if (win[b]["slashed"], win[b]["collateral"]) != (pre[b]["slashed"], pre[b]["collateral"])]
print("penalised on the winning side during the split:", moved or "none")
print("differs from the winner after the join:", bad or "none")
sys.exit(1 if bad else 0)
PY
  $CHAINSTATE compare --ports "$(all_json_ports)" --bonds "$bonds" --out "$E/compare" > "$E/compare.json"; local rc=$?
  local bad=0; for i in 0 1 2 3 4 5 6; do since "$i" "${M[$i]}" | grep -qE "disqualified from virtual chain \(PALW state root\)|panicked at" && bad=1; done
  [ $bad = 0 ] || { fail "$tag" "a node disqualified a chain block on the PALW state root, or panicked, after the split"; return 1; }
  [ $rc = 1 ] && { fail "$tag" "nodes at one sink disagree on PALW state ($E/compare.json)"; return 1; }
  if [ "$mode" = final ] || [ "$mode" = timeout ]; then
    local fb sfx
    if [ "$mode" = final ]; then fb=$(bond_of "${FINAL_BOND:-1}"); sfx=""; else fb=$(bond_of 2); sfx="2"; fi
    $CHAINSTATE claims --port "$(json 0)" --bond "$fb" --terminal > "$E/claims${sfx}-post.json"
    python3 "$DRILL_ROOT/reorgcheck.py" "$E" "$winner" "$sfx" "$split" \
      || { fail "$tag" "a claim that ended during the split ended differently after the join ($E)"; return 1; }
  fi
  [ "$mode" = exec ] && { wait_until "since 0 ${M[0]} | grep -qE '\[palw-round-lane\] chain block .* [1-9][0-9]* permit\(s\) granted'" "permits granted again after the join" 7200; }
  local stage; stage=$(python3 -c "import json; print(json.load(open('$E/dns-post.json')).get('stageName','?'))" 2>/dev/null)
  pass "$tag" "winner $winner, $([ $winner = A ] && echo "$rb" || echo "$ra") chain blocks replaced; slashed/collateral = winner's; state compare rc=$rc (0 agree, 2 inconclusive); no state-root disqualification; DNS overlay $stage (reorg gate not enforced)"
}

# ---- C6 --------------------------------------------------------------------------------------
cmd_ibd() {
  local E; E=$(ev "C6-ibd-$(date -u +%H%M)")
  rm -rf "$RUN/n7"; : > "$(logf 7)"
  local t0=$SECONDS; start_node 7
  wait_until "[ \"\$(sink_of 7)\" = \"\$(sink_of 0)\" ] && [ -n \"\$(sink_of 0)\" ]" "the IBD node to reach the fleet's sink" 86400
  echo "IBD to the fleet's sink in $((SECONDS - t0)) s at DAA $(daa_of 0)" > "$E/ibd.txt"
  grep -cE "disqualified from virtual chain \(PALW state root\)|panicked at" "$(logf 7)" > "$E/bad-lines.txt"
  identity_ok 7 6; local idrc=$?; cp "$RUN/identity-n7.json" "$E/identity-n7.json"
  $CHAINSTATE compare --ports "$(json 0),$(json 7),$(json 3)" --bonds "$(roster_bonds)" --out "$E/compare" > "$E/compare.json"; local rc=$?
  ps -o rss= -p "$(pid_of 7)" > "$E/rss-kb.txt" 2>/dev/null
  stop_node 7
  [ "$(cat "$E/bad-lines.txt")" = 0 ] || { fail "${E##*/}" "the IBD node disqualified a chain block or panicked"; return 1; }
  [ $idrc = 0 ] || { fail "${E##*/}" "the IBD node is not on the drill genesis/premine ($E/identity-n7.json)"; return 1; }
  [ $rc = 1 ] && { fail "${E##*/}" "the IBD node disagrees with the fleet at one sink ($E/compare.json)"; return 1; }
  pass "${E##*/}" "$(cat "$E/ibd.txt"); drill genesis/premine; compare rc=$rc (0 agree, 2 inconclusive — the tips kept moving)"
}

# ---- C11 execution route (item 10d) ------------------------------------------------------------
# a drill source of payments: x3 (F0 gave it 20 MSK for exactly this), else FUND_KEY (the drill main wallet,
# signing only on the proven drill chain), else the drill-only miner wallet with the most MATURE money
pay_source() { # pay_source <MSK needed> → "<key-file> <coinbase|plain>" or nothing
  local need=$1 best="" bm=0 n m
  if [ -s "$FRESH/x3.key" ] && ge_msk "$(mature_msk "$FRESH/x3.key")" "$need"; then echo "$FRESH/x3.key plain"; return; fi
  if [ -e "$FUND_KEY" ] && ge_msk "$(mature_msk "$FUND_KEY")" "$need"; then echo "$FUND_KEY plain"; return; fi
  for n in pay-1 pay-3 pay-2 hb-0 hb-3 pay-0 pay-4 pay-5 pay-6; do
    m=$(mature_msk "$FRESH/$n.key"); ge_msk "$m" "$bm" && { bm=$m; best=$n; }
  done
  [ -n "$best" ] && ge_msk "$bm" "$need" && echo "$FRESH/$best.key coinbase"
}

cmd_route() {
  local E; E=$(ev C11-exec-route)
  # 1. the first Final of a floor claim (licence + the 120-DAA short challenge window)
  wait_until "$CHAINSTATE claims --port $(json 0) --bond $(bond_of 1) --terminal --phase final | python3 -c 'import json,sys; sys.exit(0 if json.load(sys.stdin)[\"claims\"] else 1)'" "the first floor Final (≈ licence + 120 DAA)" 864000
  $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of 1)" --terminal --phase final > "$E/1-first-finals.json"
  # 2. its tickets: a Final's execution quanta mature window_challenge = 1,200 DAA after it (not the short window)
  wait_until "$CHAINSTATE claims --port $(json 0) --bond $(bond_of 1) --terminal | python3 -c 'import json,sys; sys.exit(0 if any(r.get(\"execStage\")==\"scheduled\" and r.get(\"execTickets\",0)>0 for r in json.load(sys.stdin)[\"claims\"]) else 1)'" "a Final's tickets scheduled (≈ Final + 1,200 DAA)" 1728000
  $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of 1)" --terminal > "$E/2-scheduled.json"
  # 3. a permit, and a round block produced for it
  wait_until "grep -qh '\[palw-round-producer\] [0-9]* round blocks produced' $RUN/n[0-6].log" "a round block produced (algo 10)" 172800
  cli 0 --output json palw round-lane > "$E/3-round-lane.json" 2>&1
  # 4. merged by a chain block, permits granted
  wait_until "grep -qhE '\[palw-round-lane\] chain block [0-9a-f]+ merged [0-9]+ round block\(s\): [1-9][0-9]* permit\(s\) granted' $RUN/n0.log" "a chain block granting permits" 7200
  pass C11-exec-route "Final → scheduled tickets → round block → permit granted"
  # 5. payments after this point from a DRILL-ONLY wallet; a granted round block must name one of THEM
  local E2; E2=$(ev C11-fee)
  local src; src=$(pay_source 6)
  [ -n "$src" ] || { notreached C11-fee "no drill wallet (x3, FUND_KEY, pay-*, hb-*) holds 6 MSK of mature money"; return 0; }
  local key=${src% *} kind=${src#* } flag=""
  [ "$kind" = coinbase ] && flag="--coinbase-only"
  # item 8: the coinbase it spends is mature by DAA alone — 600 DAA old or more at the send. The ages below
  # are the CLI's arithmetic (evidence); the node's word is that it accepts the send and a block carries it.
  utxo_json "$key" "${MAT_RECENT:-3000}" > "$E2/4-source-utxos.json"; local v; v=$(daa_of 0)
  python3 - "$E2/4-source-utxos.json" "$v" "$COINBASE_MATURITY_DAA" "$kind" <<'PY' > "$E2/4-maturity.txt" || log "C11-fee note: the CLI calls a coinbase younger than 600 DAA mature ($E2/4-maturity.txt) — CLI arithmetic, the node decides below"
import json,sys
w=json.load(open(sys.argv[1])); v,m,kind=int(sys.argv[2]),int(sys.argv[3]),sys.argv[4]
cb=[u for u in w.get("recent") or [] if u.get("coinbase")]
early=[u["outpoint"] for u in cb if u.get("mature") and v-int(u["blockDaaScore"])<m]
print(f"source kind {kind}; virtual DAA {v}; coinbase outputs {len(cb)}; mature {sum(1 for u in cb if u.get('mature'))}; youngest mature age {min([v-int(u['blockDaaScore']) for u in cb if u.get('mature')] or [None]) if cb else None}")
print("mature before 600 DAA:", early or "none"); sys.exit(1 if early else 0)
PY
  local m0; m0=$(mark 0); local ids=() k tx
  # x3's payments go back to the drill main wallet; any other source pays x3
  local rcpt; rcpt=$(wallet_addr x3)
  [ "$(readlink -f "$key")" = "$(readlink -f "$FRESH/x3.key")" ] && rcpt="$TESTNET_MAIN_ADDRESS"
  assert_drill_key "$key"; assert_drill_chain 0
  for k in 1 2 3 4 5; do
    tx=$(cli 0 --output json wallet send --key-file "$key" $flag --to "$rcpt" --amount "1.0000000$k" --yes 2>>"$E2/5-sends.txt" \
      | python3 -c 'import json,sys; print(json.load(sys.stdin).get("txid",""))' 2>/dev/null)
    echo "send $k from ${key##*/} ($kind) at DAA $(daa_of 0) txid=$tx" >> "$E2/5-sends.txt"; [ -n "$tx" ] && ids+=("$tx"); sleep 20
  done
  [ ${#ids[@]} -gt 0 ] || { fail C11-fee "the node accepted none of the payments ($E2/5-sends.txt)"; return 1; }
  local pat; pat=$(IFS='|'; echo "${ids[*]}")
  wait_until "since 0 $m0 | grep -qE '\[palw-round-lane\] chain block .*native: .*($pat)'" "a granted round block carrying a sent payment" 14400
  local line; line=$(since 0 "$m0" | grep -m1 -E "\[palw-round-lane\] chain block .*native: .*($pat)")
  echo "$line" > "$E2/6-carried.log"
  local chain; chain=$(echo "$line" | grep -oE "chain block [0-9a-f]+" | awk '{print $3}')
  # 6. the fee: the merging chain block's coinbase pays the round block's payout (ADR-0125), not the merger
  python3 - "$(json 0)" "$chain" "$E2" "$DRILL_ROOT" <<'PY' || { fail C11-fee "the merging chain block pays no round-block payout ($E2/7-fee.json)"; return 1; }
import json,sys
sys.path.insert(0, sys.argv[4])
from chainstate import WsRpc
port, chain, E = int(sys.argv[1]), sys.argv[2], sys.argv[3]
c = WsRpc(port)
b = c.call("getBlock", {"hash": chain, "includeTransactions": True}); b = b.get("block", b)
cb = (b.get("transactions") or [{}])[0]
vd = b.get("verboseData") or {}
rounds = []
for h in vd.get("mergeSetRedsHashes") or []:
    r = c.call("getBlock", {"hash": h, "includeTransactions": True}); r = r.get("block", r)
    if r.get("header", {}).get("powAlgoId") == 10:
        rcb = (r.get("transactions") or [{}])[0]
        rounds.append({"hash": h, "coinbasePayload": rcb.get("payload", "")})
outs = cb.get("outputs") or []
def script_hex(o):
    s = o.get("scriptPublicKey")
    if isinstance(s, dict):
        return (s.get("script") or s.get("scriptPublicKey") or "").lower()
    s = str(s or "").lower()
    return s[4:] if len(s) > 4 else s   # "<2-byte version><script>" spelling
paid = [o for o in outs if len(script_hex(o)) >= 16 and any(script_hex(o) in r["coinbasePayload"].lower() for r in rounds)]
json.dump({"chainBlock": chain, "coinbaseOutputs": outs, "roundBlocks": rounds, "paidToRoundPayout": paid}, open(f"{E}/7-fee.json", "w"), indent=1)
sys.exit(0 if paid else 1)
PY
  pass C11-fee "drill-only payment ($kind) accepted by the node and carried in ${chain:0:16}…; its coinbase pays the round payout (source coinbase ages: $E2/4-maturity.txt)"
}

cmd_c2watch() {
  local E; E=$(ev C11b-model-final-memory)
  wait_until "has_claims --port $(json 0) --bond $(bond_of 2) --terminal --phase final --class $C_8K" "the first 8k Final (≈ its licence + 120 DAA)" 864000
  $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of 2)" --terminal --phase final --class "$C_8K" > "$E/first-8k-finals.json"
  tail -40 "$RUN/observer.tsv" > "$E/observer-around-final.tsv"
  sleep 1800   # half an hour of the state that Final left behind
  tail -30 "$RUN/observer.tsv" >> "$E/observer-around-final.tsv"
  $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of 2)" --terminal --class "$C_8K" > "$E/8k-claims-after.json"
  python3 - "$E" <<'PYC' || { fail C11b-model-final-memory "anon growth or tickets past the window ($E)"; return 1; }
import json,sys,re
E=sys.argv[1]
rows=[l.rstrip("\n").split("\t") for l in open(f"{E}/observer-around-final.tsv") if l[:1].isdigit()]
def anon(r): return {k:int(v) for k,v in re.findall(r"(n\d)=(\d+)", r[-1])} if r else {}
first,last=anon(rows[0]),anon(rows[-1])
growth={k:last.get(k,0)-first.get(k,0) for k in first}
fin=json.load(open(f"{E}/8k-claims-after.json"))["claims"]
tick=max([int(r.get("execTickets") or 0) for r in fin] or [0])
print("anon growth MiB across the first 8k Final:", growth); print("largest execTickets on one 8k claim:", tick)
sys.exit(0 if max(growth.values() or [0]) < 512 and tick <= 120 else 1)
PYC
  pass C11b-model-final-memory "first 8k Final: node anon growth < 512 MiB, execution tickets within one span's 120 rounds"
}

# ---- C4 --------------------------------------------------------------------------------------
cmd_cheapcheck() {
  # two halves: every floor claim binds (always judged), and — when C3b made one — a floor-priced
  # (13,000 MSK < 130,000 MSK seat floor) bond that declared the floor is never drawn as a seat
  local E; E=$(ev C4-bind); local E4; E4=$(ev C4-cheap-bond)
  local x1; x1=$(cat "$FRESH/x1.bond" 2>/dev/null || true); local x2; x2=$(cat "$FRESH/x2.bond" 2>/dev/null || true)
  [ -n "$x1" ] && $CHAINSTATE claims --port "$(json 0)" --bond "$x1" --role seat --terminal > "$E4/x1-as-seat.json"
  [ -n "$x2" ] && $CHAINSTATE claims --port "$(json 0)" --bond "$x2" --role seat --terminal > "$E4/x2-as-seat.json"
  local fl=""; for i in 1 3; do $CHAINSTATE claims --port "$(json 0)" --bond "$(bond_of "$i")" --terminal > "$E/floor-claims-$i.json"; fl="$fl $E/floor-claims-$i.json"; done
  # a claim binds at its anchor, anchor_delay = 20 DAA after acceptance (PALW_RC_WINDOWS_V1): judge only
  # claims older than that plus a margin
  local decl; decl=$(cat "$FRESH/x1.declared_daa" 2>/dev/null || echo 0)
  python3 - "$E" "$E4" "$x1" "${x2:-}" "${BIND_GRACE_DAA:-30}" "$decl" "${C4_MIN_DRAWS:-10}" $fl <<'PY'; local rc=$?
import json,sys
E,E4,x1,x2=sys.argv[1:5]; g=int(sys.argv[5]); decl=int(sys.argv[6] or 0); min_draws=int(sys.argv[7])
rows=[r for f in sys.argv[8:] for r in json.load(open(f))["claims"]]
tip=max([json.load(open(f))["tipDaa"] or 0 for f in sys.argv[8:]] or [0])
old=[r for r in rows if (r.get("acceptedDaa") or 0) <= tip - g]
unbound=[r["claimId"][:16] for r in old if r.get("boundDaa") is None and not (r.get("phase")=="voided" and "bind" not in (r.get("voidReason") or "").lower())]
bindvoid=[r["claimId"][:16] for r in rows if r.get("phase")=="voided" and "bind" in (r.get("voidReason") or "").lower()]
out=[f"floor claims {len(rows)} (older than {g} DAA: {len(old)})", f"unbound claims older than {g} DAA: {unbound or 'none'}", f"bind-timeout voids: {bindvoid or 'none'}"]
open(f"{E}/verdict.txt","w").write("\n".join(out)+"\n")
bind_ok = not unbound and not bindvoid and len(old) > 0
cheap = None
if x1:
    # the draws that could have picked x1: floor claims bound after its declaration landed. With 8 genesis
    # seats a 5-seat panel over the 7 non-executor cards plus an eligible x1 would include x1 in about 5/8 of draws, so ≥ C4_MIN_DRAWS (10)
    # draws without it is a verdict even without a declared control; fewer is NOT-REACHED.
    after=[r for r in rows if r.get("boundDaa") is not None and int(r["boundDaa"]) >= decl]
    drawn_x1=sorted({r["claimId"][:16] for r in rows if x1 in (r.get("seats") or [])} | {r["claimId"][:16] for r in json.load(open(f"{E4}/x1-as-seat.json"))["claims"]})
    drawn_x2=[r["claimId"][:16] for r in rows if x2 and x2 in (r.get("seats") or [])]
    o=[f"x1 (13,000 MSK, below the 130,000 MSK seat floor) declared the floor by DAA {decl}; floor claims bound since: {len(after)} (need {min_draws})",
       f"x1 drawn into: {drawn_x1 or 'none'}",
       f"x2 (above the seat floor; declares the floor only with CONTROL_DECLARE=1) drawn into: {drawn_x2 or ('none' if x2 else 'no x2 bond')}"]
    open(f"{E4}/verdict.txt","w").write("\n".join(o)+"\n")
    cheap = False if drawn_x1 else (True if len(after) >= min_draws else "short")
print(json.dumps({"bind": bind_ok, "cheap": cheap}))
sys.exit((0 if bind_ok else 1) | (2 if cheap is False else 0) | (4 if cheap == "short" else 0))
PY
  [ $((rc & 1)) = 0 ] && pass C4-bind "$(tr '\n' ';' < "$E/verdict.txt")" || fail C4-bind "see $E/verdict.txt"
  if [ -z "$x1" ]; then notreached C4-cheap-bond "no floor-priced bond was registered (C3b needs F0: drill.sh fund from the drill main wallet)"
  elif [ $((rc & 2)) != 0 ]; then fail C4-cheap-bond "x1 was drawn as a seat — see $E4/verdict.txt"
  elif [ $((rc & 4)) != 0 ]; then notreached C4-cheap-bond "too few floor draws since x1's declaration: $(tr '\n' ';' < "$E4/verdict.txt")"
  else pass C4-cheap-bond "$(tr '\n' ';' < "$E4/verdict.txt")"; fi
}

# ---- C12 -------------------------------------------------------------------------------------
cmd_gate() {
  local E; E=$(ev "C12-gate${GATE_SHORT:+-short}")
  local fp=""; for c in "$C_FLOOR" "$C_8K"; do [ "$(fp_certified "$c")" = True ] && fp="$fp,${c:0:16}"; done
  $CHAINSTATE gate --port "$(json 0)" --window-daa "${GATE_WINDOW:-600}" --bonds "$(roster_bonds)" --floor "$C_FLOOR" --model "$C_8K" \
    --two-m "$C_2M_PREFIX" --alarm-ports "$(all_json_ports)" ${fp:+--want-fp "${fp#,}"} ${GATE_SHORT:+--short} > "$E/gate.json"
  local rc=$?
  $CHAINSTATE lanes --port "$(json 0)" --window-daa "${GATE_WINDOW:-600}" --bonds "$(roster_bonds)" > "$E/lanes.json"
  local what="algo 6 and 10 per class"; [ -n "${GATE_SHORT:-}" ] && what="algo 6 per class (10 not judged: tickets mature 1,200 DAA after a Final)"
  [ $rc = 0 ] && pass "${E##*/}" "$what, 9 absent, no alarm: $E/gate.json" \
    || fail "${E##*/}" "$(python3 -c "import json; print('; '.join(json.load(open('$E/gate.json'))['problems'])[:600])")"
}

# ---- observer --------------------------------------------------------------------------------
cmd_observe() {
  local out="$RUN/observer.tsv" op
  op=$(cat "$RUN/observer.pid" 2>/dev/null)
  [ -n "$op" ] && [ "$op" != $$ ] && observer_pid_ok "$op" && kill -0 "$op" 2>/dev/null && die "an observer is already running (pid $op)"
  echo $$ > "$RUN/observer.pid"
  [ -f "$out" ] || printf 'utc\tdaa0\tsinks_agree\tavail_gib\t8k_state\t8k_ready_now\tlane_alarm\tanon_mib_by_node\n' > "$out"
  local tick=0 low=0
  while :; do
    local anon="" i p
    for i in 0 1 2 3 4 5 6 7 8; do p=$(pid_of "$i"); drill_pid_ok "$i" "$p" && [ -r "/proc/$p/smaps_rollup" ] \
      && anon="$anon n$i=$(awk '/^Anonymous:/{print int($2/1024)}' "/proc/$p/smaps_rollup")"; done
    local agree=no; sinks_agree 1 2 3 4 5 6 2>/dev/null && agree=yes
    local alarm; alarm=$($CHAINSTATE status --port "$(json 0)" 2>/dev/null | python3 -c 'import json,sys; print("ALARM" if json.load(sys.stdin).get("laneAlarm") else "-")' 2>/dev/null)
    local av; av=$(avail_gib)
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$(daa_of 0)" "$agree" "$av" "$(reg8k_state 2>/dev/null)" \
      "$(reg8k readySeatsNow 2>/dev/null)" "$alarm" "$anon" >> "$out"
    # every class's state/inflight/room/ready (C15) and, every 5th minute, a room-vs-gate sample (C14)
    printf '%s\t%s\n' "$(date -u +%FT%TZ)" "$($CHAINSTATE classes --port "$(json 0)" 2>/dev/null)" >> "$RUN/classes.tsv"
    [ $((tick % 5)) = 0 ] && $CHAINSTATE room --port "$(json 0)" --class "$C_8K" --bond "$(bond_of 2)" >> "$RUN/room.jsonl" 2>/dev/null
    tick=$((tick + 1))
    # the brake: three samples in a row under EMERGENCY_GIB stop every DRILL node (never anything else) —
    # the OOM killer does not know which process is public (09-23, ibm)
    if awk -v a="$av" -v e="${EMERGENCY_GIB:-2.5}" 'BEGIN{exit !(a < e)}'; then low=$((low + 1)); else low=0; fi
    if [ "$low" -ge 3 ]; then log "EMERGENCY: MemAvailable ${av} GiB for 3 min — stopping the drill's nodes to protect the live floor seats"; for j in 8 7 6 5 4 3 2 1 0; do stop_node "$j"; done; exit 3; fi
    sleep 60
  done
}

cmd_down() {
  # the observer is signalled only when its pid is provably this kit's `drill.sh observe` (a stale pid
  # file — the observer exits 3 after an EMERGENCY stop — may name a reused pid)
  local op; op=$(cat "$RUN/observer.pid" 2>/dev/null)
  if [ -n "$op" ] && observer_pid_ok "$op"; then kill "$op" 2>/dev/null; fi
  rm -f "$RUN/observer.pid"
  for i in 8 7 6 5 4 3 2 1 0; do stop_node "$i"; done
  local rec="$EV/C0-preflight/public-endpoints-before.txt"
  if [ -s "$rec" ]; then
    local now; now=$(endpoints_fingerprint)
    if [ "$now" = "$(cat "$rec")" ]; then log "down: $PUBLIC_ENDPOINTS unchanged since preflight ($now)"
    elif endpoints_untouched; then log "down: $PUBLIC_ENDPOINTS changed since preflight but names no drill port (a live t12 unit rewrote it)"
    else log "down: WARNING — $PUBLIC_ENDPOINTS names a drill port; a drill process wrote root's registry"; fi
  fi
}

case "${1:-}" in
  keygen|mainkey|preflight|up|pace|alarm|b1|fund|hold7|restart|licence|room|second|maturity|silence|unsilence|ibd|route|c2watch|cheapcheck|gate|observe|down)
    c=$1; shift; "cmd_$c" "$@" ;;
  stage-art) shift; cmd_stage_art "$@" ;;
  verify-art) shift; cmd_verify_art "$@" ;;
  reorg) shift; cmd_reorg "$@" ;;
  *) sed -n '2,44p' "$0"; exit 2 ;;
esac
