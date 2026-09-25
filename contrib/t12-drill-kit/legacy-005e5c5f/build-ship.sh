#!/usr/bin/env bash
# drill-t12/build-ship.sh — build the DRILL tree on the drill host, into its own dirs.
#
#   ./build-ship.sh          (defaults: SHIP_REV=005e5c5f…, SRC_FROM=/root/drill-t12/ship-pre2.bundle)
#
# The drill is evidence about ONE binary. Here that binary is 005e5c5f: c8652a97 (the t12 launch
# candidate — the panel-room review f8c91f19/92a9658d/0533e1de, the licence-stall fix a4dfe903/d94d3a1b,
# and the operator's 100M community row 5bf72b46, public genesis a27f8f44…) plus ONE drill-only commit
# that salts the chain's identity (premine txid 269a354e…, genesis 32a665d6…) and makes the drill chain's
# main wallet TESTNET_MAIN_ADDRESS (the key of the public test seed tests::TESTNET_MAIN_SEED). It is NEVER
# shipped — the fleet ships c8652a97 without the salt — so this script refuses to build anything but
# SHIP_REV, and after the build it greps the source and the binaries for the markers of every fix the
# checklist exercises AND for the drill salt: a drill on a stale build proves nothing (09-19: a drill ran
# 40 commits behind the ship tip), and a drill on an UNSALTED build signs spends that are valid on public
# testnet-12.
#
# It never touches /root/t12-private, /root/perm-drill or any other session's tree or target; it
# runs at nice 10 with -j4 and CARGO_INCREMENTAL=0 so the live floor seats on this host keep their CPU,
# and it refuses to start while another cargo/rustc runs on the host.
# $DRILL_ROOT/src is a --no-checkout clone of a bundle whose branch is a REMOTE-TRACKING ref
# (refs/remotes/origin/drill/t12-pre-…) and whose HEAD names an unborn `master`: the script fetches the
# bundle's heads explicitly (into refs/remotes/bundle/*) and checks out the SHA detached.
set -u
DRILL_ROOT="${DRILL_ROOT:-/root/drill-t12}"
SHIP_REV="${SHIP_REV:-005e5c5f2d42131dcdeaaa633312aafa34f84bdd}"
SRC_FROM="${SRC_FROM:-$DRILL_ROOT/ship-pre2.bundle}"
[ ${#SHIP_REV} = 40 ] || { echo "SHIP_REV must be the 40-hex commit"; exit 1; }
SRC="$DRILL_ROOT/src"; TGT="$DRILL_ROOT/target"; BIN="$DRILL_ROOT/bin"; L="$DRILL_ROOT/build.log"
mkdir -p "$DRILL_ROOT"
say() { echo "[$(date -u +%FT%TZ)] $*" | tee -a "$L"; }

free_gb=$(df -BG --output=avail "$DRILL_ROOT" | tail -1 | tr -dc 0-9)
[ "${free_gb:-0}" -ge 40 ] || { say "REFUSED: ${free_gb} GB free under $DRILL_ROOT, a release build needs ~40"; exit 1; }
avail=$(awk '/MemAvailable/{print int($2/1048576)}' /proc/meminfo)
[ "$avail" -ge 8 ] || { say "REFUSED: MemAvailable ${avail} GiB; a -j4 release link needs ~8 and the live floor seats share this host"; exit 1; }
# one build at a time on this host: another cargo/rustc (any session's) would double the memory peak
others=$(pgrep -a -x cargo; pgrep -a -x rustc) || true
[ -z "$others" ] || { say "REFUSED: another cargo/rustc is running on this host:"; echo "$others" | tee -a "$L"; exit 1; }
say "build-ship start: SHIP_REV=$SHIP_REV SRC_FROM=$SRC_FROM ($(sha256sum "$SRC_FROM" 2>/dev/null | awk '{print $1}')), ${free_gb} GB free, MemAvailable ${avail} GiB, pid $$"

source ~/.cargo/env 2>/dev/null || true
if [ ! -d "$SRC/.git" ]; then
  say "cloning $SRC_FROM"
  git clone -q --no-hardlinks --no-checkout "$SRC_FROM" "$SRC" >> "$L" 2>&1 || { say "BUILD-FAILED (clone)"; exit 1; }
fi
if [ -f "$SRC_FROM" ]; then
  git -C "$SRC" bundle verify "$SRC_FROM" >> "$L" 2>&1 || { say "BUILD-FAILED: $SRC_FROM is not a complete bundle"; exit 1; }
fi
# every head the bundle carries, into a namespace of its own (a bare `git fetch <bundle>` asks for the
# bundle's HEAD, which this bundle does not have, and fails)
git -C "$SRC" fetch -q "$SRC_FROM" "+refs/heads/*:refs/remotes/bundle/*" >> "$L" 2>&1 || { say "BUILD-FAILED: fetch from $SRC_FROM"; exit 1; }
git -C "$SRC" cat-file -e "$SHIP_REV^{commit}" 2>> "$L" || { say "BUILD-FAILED: $SHIP_REV is not in $SRC_FROM"; exit 1; }
git -C "$SRC" -c advice.detachedHead=false checkout -q --detach "$SHIP_REV" >> "$L" 2>&1 || { say "BUILD-FAILED: checkout $SHIP_REV"; exit 1; }
[ "$(git -C "$SRC" rev-parse HEAD)" = "$SHIP_REV" ] || { say "BUILD-FAILED: HEAD is not $SHIP_REV"; exit 1; }
[ -z "$(git -C "$SRC" status --porcelain --untracked-files=no)" ] || { say "BUILD-FAILED: the checkout is dirty"; exit 1; }

# the fixes this drill exercises, by their source markers (ancestry is not enough: grep the fix itself).
# Each was verified present at 005e5c5f (Mac worktree drill/t12-pre-c8652a97) before this list was written.
miss=0
for m in "kaspad/src/palw_lane_watch.rs:no PALW work block in the newest" \
         "kaspad/src/palw_panel.rs:palw_operator_possession_message_v1" \
         "consensus/core/src/palw_producer_v2.rs:PALW_NOT_READY_CLASS_NOT_ADMITTING_V2" \
         "consensus/core/src/palw_execution_quanta_v1.rs:palw_execution_mint_quanta_windowed_v1" \
         "consensus/core/src/palw_panel_v2.rs:valid_lock" \
         "misaka-cli/src/operator/catalog.rs:E-MODEL-NOT-ADMITTING" \
         "consensus/core/src/palw_state_v2.rs:fn panel_room_by_rate_v1(" \
         "consensus/core/src/palw_work_target_v1.rs:pub fn palw_panel_capacity_by_rate_v1(" \
         "consensus/core/src/palw_state_v2.rs:fn palw_panel_owed_v1(" \
         "consensus/core/src/palw_state_v2.rs:pub fn palw_panel_room_read_v1(" \
         "consensus/core/src/palw_state_v2.rs:if audited && !exempt && self.state.class_is_held_v1(class_id) {" \
         "consensus/core/src/palw_state_v2.rs:class {class} has {inflight} claims in flight against the registry's cap of {cap}" \
         "consensus/core/src/palw_panel_v2.rs:pub fn palw_select_optimistic_licence_v2" \
         "kaspad/src/palw_panel.rs:const RECEIPTS_V3_MAX_CLAIMS: usize = 256;" \
         "kaspad/src/palw_panel.rs:fn seat_duty_panel_key_v1(" \
         "kaspad/src/palw_producer.rs:has no panel room left in the network's verification budget" \
         "consensus/core/src/palw_model_registry_v1.rs:palw_lifecycle_reason_v2" \
         "consensus/core/src/config/params.rs:pub palw_clock_floor" \
         "consensus/core/src/palw_heartbeat_v1.rs:heartbeat_chain_capacity_v1" \
         "kaspad/src/palw_heartbeat_miner.rs:holds the open slot, not granted yet: the block" \
         "consensus/core/src/config/params.rs:pub const PALW_T12_DNS_PARAMS" \
         "consensus/core/src/config/premine.rs:pub fn premine_txid_for" \
         "consensus/core/src/config/premine.rs:misaka-palw-t12/premine/v2/2026-09-24/DRILL-PRE-c8652a97" \
         "consensus/core/src/config/genesis.rs:0x32, 0xa6, 0x65, 0xd6, 0x22, 0xba, 0xba, 0x4c" \
         "consensus/core/src/config/genesis.rs:0xd5, 0x0a, 0x03, 0xca, 0x1f, 0x6c, 0x25, 0xb8" \
         "consensus/core/src/config/genesis.rs:timestamp: 1788220860000," \
         "consensus/core/src/config/premine.rs:        return TESTNET_MAIN_ADDRESS;" \
         "consensus/core/src/config/premine.rs:misakatest:qtpflz03z576h02mtpn2vtwg5npj8fhlau3fgmsjl2a2uw0venj3573l07uahcs4gnsl8eqc7nlq5phakthxy606q2jyuxh2a08weduxa2yqlxuz" \
         "consensus/core/src/config/premine.rs:pub(super) const TESTNET_MAIN_SEED: &[u8] = b\"misaka-testnet-premine-9b-claude-managed\";" \
         "consensus/core/src/config/premine.rs:misakatest:qffaadrfjpt9gy3705xhr2n6085767w290lgf0xd55nrj8px2lk8cj8w34scu4y7l5avauhul3lu9apzc6vugkeu3jhkltgrvfk4m6emz4hjtsfy" \
         "misaka-cli/src/main.rs:fn key_import(ctx: &node::Ctx, out: &str, hex_stdin: bool, hex_file: Option<&str>) -> CliResult {" \
         "consensus/core/src/palw_state_v2.rs:pub fn palw_bond_registration_floor_v1" \
         "consensus/core/src/palw_fp_devnet_v3.rs:PALW_MAINNET_MIN_COLLATERAL_SOMPI: u64 = 13_000" \
         "kaspad/src/daemon.rs:let inbound_limit = if connect_peers.is_empty() { args.inbound_limit } else { 0 };" \
         "protocol/flows/src/service.rs:let p2p_adaptor = if self.inbound_limit == 0 {" \
         "components/addressmanager/src/lib.rs:if address.ip.is_loopback() || address.ip.is_unspecified() {" \
         "kaspad/src/daemon.rs:if !args.connect_peers.is_empty() && !args.add_peers.is_empty() {" \
         "misaka-endpoints/src/lib.rs:dirs::home_dir().map(|h| h.join(\".misaka\").join(network_id).join(\"endpoints.json\"))" \
         "consensus/core/src/config/premine.rs:misakatest:qf7hzj76mg0wrch9mm89ag8s8apgrz7qgkk77j5z0ypykngrl2ayd2rnvleafk0fxhaxl70kr29x6fakav79jax9ul6jghrcs42nmlqx0tawqn8x" \
         "rpc/service/src/service.rs:state: row.map(|r| format!(\"{:?}\", r.state))" \
         "rpc/service/src/service.rs:consensus_params_id: params.consensus_params_id().to_string()," \
         "rpc/service/src/service.rs:format!(\"{why} [{detail}]\")" \
         "consensus/core/src/palw_state_v2.rs:the panel has no room for a claim of class {class}: {inflight_replay} of replay in flight against a budget of {budget} over {horizon_spans} spans" \
         "consensus/src/pipeline/virtual_processor/processor.rs:is disqualified from virtual chain (PALW state): {}" \
         "kaspad/src/palw_panel.rs:filed a {:?} receipt for claim {}" \
         "kaspad/src/palw_panel.rs:submitted a readiness proof for class {class_id} (span {span}) in tx {txid}" \
         "kaspad/src/palw_heartbeat_miner.rs:and advanced the clock to DAA {daa}" \
         "misaka-cli/src/bond.rs:\"bonds_registered_to_this_key\": owned"; do
  f=${m%%:*}; s=${m#*:}
  grep -qF -- "$s" "$SRC/$f" 2>/dev/null || { say "MARKER MISSING in $f: $s"; miss=1; }
done
# and every public/retired/first-drill genesis must be GONE from the source that builds the binary:
# a27f8f44 (public, c8652a97), d73dbf44, f6cc9576, fb1074b0, a8cabac4, 1eaa6c0f (the first pre-drill)
for g in "0xa2, 0x7f, 0x8f, 0x44" "0xd7, 0x3d, 0xbf, 0x44" "0xf6, 0xcc, 0x95, 0x76" "0xfb, 0x10, 0x74, 0xb0" "0xa8, 0xca, 0xba, 0xc4" "0x1e, 0xaa, 0x6c, 0x0f"; do
  if grep -qF "$g" "$SRC/consensus/core/src/config/genesis.rs"; then say "MARKER: forbidden genesis bytes ($g…) are still in genesis.rs"; miss=1; fi
done
[ $miss = 0 ] || { say "BUILD-FAILED: SHIP_REV lacks a fix the checklist exercises, or is not the salted drill build"; exit 1; }

say "building $SHIP_REV (CARGO_TARGET_DIR=$TGT, CARGO_INCREMENTAL=0, nice 10, -j4)"
( cd "$SRC" && CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$TGT" nice -n 10 cargo build --release -j 4 --bin kaspad --bin misaka --bin palw-class ) >> "$L" 2>&1
rc=$?
say "cargo rc=$rc"
[ $rc = 0 ] || { say "BUILD-FAILED"; exit 1; }
mkdir -p "$BIN"
for b in kaspad misaka palw-class; do cp -f "$TGT/release/$b" "$BIN/$b"; done
echo "$SHIP_REV" > "$BIN/REV"
( cd "$BIN" && sha256sum kaspad misaka palw-class > SHA256SUMS )
# the same markers, in the binaries this time (literal pieces of format strings, each on one source line)
for pair in "kaspad:palw-lane-watch" \
            "kaspad:no PALW work block in the newest " \
            "kaspad:the model registry admits no new claim of this class now" \
            "kaspad:has no panel room left in the network's verification budget" \
            "kaspad:held: seats and window are back; it re-enters probation at the next boundary" \
            "kaspad: spans) does not fit the receipt deadline" \
            "kaspad: are required to re-enter probation (compare readySeatsNow)" \
            "kaspad:holds the open slot, not granted yet: the block" \
            "kaspad: is below this chain's floor of " \
            "kaspad:misaka-palw-t12/premine/v2/2026-09-24/DRILL-PRE-c8652a97" \
            "kaspad:the panel has no room for a claim of class " \
            "kaspad: claims in flight against the registry's cap of " \
            "kaspad: is disqualified from virtual chain (PALW state): " \
            "kaspad:advanced the clock to DAA " \
            "kaspad: receipt for claim " \
            "misaka:E-MODEL-NOT-ADMITTING" \
            "misaka:Not mining: the chain admits no new claim of this class now" \
            "misaka:name a source for the seed: --hex-stdin"; do
  b=${pair%%:*}; s=${pair#*:}
  grep -aqF -- "$s" "$BIN/$b" || { say "BINARY MARKER MISSING in $b: $s"; exit 1; }
done
"$BIN/kaspad" --help 2>/dev/null | grep -q -- "--palw-host-memory-share" || { say "kaspad lacks --palw-host-memory-share"; exit 1; }
say "BUILD-DONE $(cat "$BIN/SHA256SUMS" | tr '\n' ' ')"
