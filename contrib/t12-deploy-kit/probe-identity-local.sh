#!/usr/bin/env bash
# deploy-t12/probe-identity-local.sh — read a testnet-12 kaspad build's OWN identity on THIS machine:
# the consensus params fingerprint (= the params id, getPalwNodeStatus.consensusParamsId), the genesis
# hash, the premine txid its genesis bonds sit on, the fence schedule id and the rule manifest digest.
# Prints the fleet.env lines. LOCAL ONLY: it contacts no host.
#
#   ./probe-identity-local.sh [--build] [--kaspad <path>] [--drill-salt <64 hex>] [--layout]
#
#   --build            cargo build -p kaspad --bin kaspad first (PROFILE=release|dev, default release;
#                      CARGO_TARGET_DIR / CARGO_BUILD_JOBS as the caller sets them). The identity is a
#                      function of the params and the genesis, not of the profile or the architecture,
#                      so a dev build on the Mac reads the same values the fleet's x86_64 release announces.
#   --kaspad <path>    probe this binary (default: $CARGO_TARGET_DIR/<profile>/kaspad).
#   --drill-salt <hex> ALSO derive the drill genesis this salt gives (kaspad --palw-drill-write-keyring into
#                      a throwaway dir: it writes files and exits, dials nobody) and print its
#                      `DRILL_GENESES+=…` line. The node probe itself never carries a drill flag.
#   --layout           ALSO run this tree's pin of the kit's copies of chain facts
#                      (consensus/core/tests/t12_deploy_kit_constants.rs: card N = bond index N and fee
#                      float FEE_FLOAT_BASE+N paid to the card's payout key, CLASS_8K, ART_8K_BYTES (the
#                      committed sidecar's artifact_bytes),
#                      CLASS_2M_PREFIX, the explorer's class table and PANEL_BOND_TX) and print its
#                      layout table; and check the committed 8k sidecar against MANIFEST_8K_SHA256.
#                      Needs cargo (one test target of kaspa-consensus-core).
#
# Always: the probe node's genesis classes (id, artifact bytes, root) and genesis bond outpoints are
# read over RPC and compared with fleet.env CLASS_8K / PREMINE_TXID and lib.sh CLASS_2M_PREFIX — the facts
# `--check` and the fp/genesis gate do not see (exit 4 on a mismatch). The registry's `bytes=` is the work
# derivation's artifact figure, not the file's size, so ART_8K_BYTES is checked by --layout (sidecar), not here.
#
# The probe node is isolated: 127.0.0.1 listeners only (P2P, wRPC JSON), --nogrpc, --nodnsseed,
# --disable-upnp, --outpeers=0, no --addpeer/--connect, a fresh throwaway --appdir, HOME pointed at the
# throwaway dir (kaspad writes ~/.misaka/<net>/endpoints.json on start: the operator's registry is never
# touched), no PALW flag, and `env -i` so no KASPAD_* variable of the caller's shell reaches it. It is
# stopped with SIGINT and its throwaway dir removed; its log is kept (PROBE_OUT, default ./probe-out).
# PROBE_WAIT_S (default 180) bounds the wait for the identity over RPC; build-release-local.sh raises it
# when the release binary runs under emulation in a linux/amd64 container.
set -euo pipefail

KIT=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$KIT/../.." && pwd)
PROFILE=${PROFILE:-release}
BUILD=0; KASPAD=""; SALT=""; LAYOUT=0
while [ $# -gt 0 ]; do
    case "$1" in
        --build) BUILD=1 ;;
        --layout) LAYOUT=1 ;;
        --kaspad) KASPAD=${2:?--kaspad needs a path}; shift ;;
        --drill-salt) SALT=${2:?--drill-salt needs 64 hex}; shift ;;
        -h|--help) sed -n '2,36p' "$0"; exit 0 ;;
        *) echo "unknown argument $1" >&2; exit 2 ;;
    esac
    shift
done
say() { printf '[probe %s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }
die() { say "ABORT: $*"; exit 1; }

P2P=${PROBE_P2P:-26991}; JSON=${PROBE_JSON:-26994}
PROBE_OUT=${PROBE_OUT:-$PWD/probe-out}
TARGET=${CARGO_TARGET_DIR:-$REPO/target}
case "$PROFILE" in release) PDIR=release ;; dev) PDIR=debug ;; *) die "PROFILE must be release or dev" ;; esac
if [ "$BUILD" = 1 ]; then
    flag=(); [ "$PROFILE" = release ] && flag=(--release)
    say "building kaspad ($PROFILE) from $REPO at $(git -C "$REPO" rev-parse --short=12 HEAD)"
    (cd "$REPO" && cargo build --locked "${flag[@]}" -p kaspad --bin kaspad) >&2
fi
KASPAD=${KASPAD:-$TARGET/$PDIR/kaspad}
[ -x "$KASPAD" ] || die "no kaspad at $KASPAD (--build, or --kaspad <path>)"
if [ -n "$SALT" ]; then [[ "$SALT" =~ ^[0-9a-fA-F]{64}$ ]] || die "--drill-salt must be 64 hex"; fi

port_busy() { { command -v lsof >/dev/null 2>&1 && lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; } || { command -v ss >/dev/null 2>&1 && ss -ltn "sport = :$1" 2>/dev/null | grep -q LISTEN; }; }
for p in "$P2P" "$JSON"; do port_busy "$p" && die "probe port $p is in use (PROBE_P2P / PROBE_JSON)"; done

# fleet.env (or its template) for the forbidden lists
if [ -f "$KIT/fleet.env" ]; then . "$KIT/fleet.env"; else . "$KIT/fleet.env.example"; fi

mkdir -p "$PROBE_OUT"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/t12-probe.XXXXXX")
LOG="$PROBE_OUT/probe-$(date -u +%Y%m%dT%H%M%SZ).log"
PID=""
cleanup() {
    if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
        kill -INT "$PID" 2>/dev/null || true
        for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
        kill -9 "$PID" 2>/dev/null || true
    fi
    case "$WORK" in "${TMPDIR:-/tmp}"/t12-probe.*) rm -rf "$WORK" ;; esac
}
trap cleanup EXIT

KSHA=$( (command -v sha256sum >/dev/null && sha256sum "$KASPAD" || shasum -a 256 "$KASPAD") | cut -d' ' -f1)
say "probing $KASPAD (sha256 ${KSHA:0:16}…), isolated on 127.0.0.1:$P2P / json 127.0.0.1:$JSON, appdir $WORK/app"
env -i PATH="$PATH" HOME="$WORK/home" "$KASPAD" --testnet --netsuffix=12 --appdir="$WORK/app" --yes \
    --listen="127.0.0.1:$P2P" --rpclisten-json="127.0.0.1:$JSON" --nogrpc --nodnsseed --disable-upnp --outpeers=0 \
    > "$LOG" 2>&1 &
PID=$!

FP=""; GEN=""; out=""
WAIT_S=${PROBE_WAIT_S:-180}   # build-release-local.sh raises it for a container probe under emulation
for _ in $(seq 1 $(( (WAIT_S + 1) / 2 ))); do
    sleep 2
    kill -0 "$PID" 2>/dev/null || { tail -30 "$LOG" >&2; die "the probe node exited — see $LOG"; }
    out=$(python3 "$KIT/t12check.py" --port "$JSON" --probe 2>/dev/null || true)
    FP=$(sed -n 's/^FP=//p' <<<"$out"); GEN=$(sed -n 's/^GENESIS=//p' <<<"$out")
    [ -n "$FP" ] && [ -n "$GEN" ] && break
done
[ -n "$FP" ] && [ -n "$GEN" ] || { tail -30 "$LOG" >&2; die "no identity over RPC within $WAIT_S s (PROBE_WAIT_S) — see $LOG"; }
PREMINE=$(python3 "$KIT/t12check.py" --port "$JSON" --premine 2>/dev/null | sed -n 's/^PREMINE_TXID=//p' || true)
GLAYOUT=$(python3 "$KIT/t12check.py" --port "$JSON" --layout 2>/dev/null || true)
kill -INT "$PID" 2>/dev/null || true
for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
PID_EXITED=1; kill -0 "$PID" 2>/dev/null && PID_EXITED=0

LOGFP=$(grep -oE 'Consensus params fingerprint: [0-9a-f]{64}' "$LOG" | head -1 | awk '{print $4}')
NETLINE=$(grep -oE 'Consensus params fingerprint: [0-9a-f]{64} \(network [^)]*\)' "$LOG" | head -1 | sed -E 's/.*\(network (.*)\)/\1/')
SCHED=$(grep -oE 'Consensus fence schedule: .*\(schedule id [0-9a-f]+\)' "$LOG" | head -1 | sed -E 's/.*\(schedule id ([0-9a-f]+)\)/\1/')
MANI=$(grep -oE 'Consensus rule manifest: .*\(digest [0-9a-f]+\)' "$LOG" | head -1 | sed -E 's/.*\(digest ([0-9a-f]+)\)/\1/')
[ "$FP" = "$LOGFP" ] || die "the RPC fingerprint ($FP) and the log line ($LOGFP) disagree — see $LOG"
[ "$NETLINE" = testnet-12 ] || die "the probe announced network '$NETLINE', not testnet-12"
if grep -q 'PALW DRILL' "$LOG"; then die "the probe announced a PALW DRILL chain — this binary or its environment carries a salt"; fi
bad=""
for g in $FORBIDDEN_GENESIS ${DRILL_GENESES:-}; do [ "$GEN" = "$g" ] && bad=$g; done

DRILL_GEN=""
if [ -n "$SALT" ]; then
    KR="$WORK/keyring"
    env -i PATH="$PATH" HOME="$WORK/home" "$KASPAD" --testnet --netsuffix=12 --palw-drill-genesis-salt="$SALT" \
        --palw-drill-write-keyring="$KR" >> "$LOG" 2>&1 || die "the keyring export failed — see $LOG"
    DRILL_GEN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["genesis_hash"])' "$KR/manifest.json")
    PUB_GEN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["public_genesis_hash"])' "$KR/manifest.json")
    [ "$PUB_GEN" = "$GEN" ] || die "the keyring names public genesis ${PUB_GEN:0:16}…, the probe read ${GEN:0:16}…"
    [ "$DRILL_GEN" != "$GEN" ] || die "the salt did not move the genesis"
fi

# ---- the kit's copies of chain facts, against what this binary's genesis registers ----
CLASS_2M_PREFIX=$(sed -n 's/^CLASS_2M_PREFIX=//p' "$KIT/lib.sh" | head -1)
kitbad=""
row8k=$(grep -E "^CLASS ${CLASS_8K:-none} " <<<"$GLAYOUT" || true)
[ -n "$row8k" ] || kitbad+=" CLASS_8K ${CLASS_8K:0:16}… is not a genesis class of this binary;"
grep -qE "^CLASS ${CLASS_2M_PREFIX:-none}" <<<"$GLAYOUT" || kitbad+=" lib.sh CLASS_2M_PREFIX ${CLASS_2M_PREFIX:-?} names no genesis class;"
if [ -n "$PREMINE" ]; then
    for n in 0 1 2 3 4 5 6 7; do grep -qx "BOND $PREMINE:$n" <<<"$GLAYOUT" || kitbad+=" no genesis bond at PREMINE_TXID:$n;"; done
fi
LAYOUT_OUT=""
if [ "$LAYOUT" = 1 ]; then
    side="$REPO/consensus/core/src/config/class-manifests/qwen25-1.5b-a16-8k.palwmanifest"
    sside=$( (command -v sha256sum >/dev/null && sha256sum "$side" || shasum -a 256 "$side") | cut -d' ' -f1)
    [ "$sside" = "${MANIFEST_8K_SHA256:-none}" ] || kitbad+=" MANIFEST_8K_SHA256 ${MANIFEST_8K_SHA256:0:16}… != the committed 8k sidecar's ${sside:0:16}…;"
    say "running the kit-constants pin (cargo test -p kaspa-consensus-core --test t12_deploy_kit_constants)"
    if LAYOUT_OUT=$(cd "$REPO" && cargo test --locked -p kaspa-consensus-core --test t12_deploy_kit_constants -- --nocapture 2>&1); then
        LAYOUT_OUT=$(grep -E '^(LAYOUT|CLASSES|GENESIS) ' <<<"$LAYOUT_OUT")
    else
        tail -25 <<<"$LAYOUT_OUT" >&2; kitbad+=" the kit-constants pin FAILED (above);"; LAYOUT_OUT=""
    fi
fi

REVSRC=""; if git -C "$REPO" rev-parse --git-dir >/dev/null 2>&1; then REVSRC=$(git -C "$REPO" rev-parse --short=12 HEAD); [ -z "$(git -C "$REPO" status --porcelain --untracked-files=no)" ] || REVSRC="$REVSRC-dirty"; fi
cat <<EOF

# ---- probe-identity-local.sh $(date -u +%FT%TZ) ----
# binary   $KASPAD
# sha256   $KSHA  (the probed binary; the fleet's KASPAD_SHA256 is the release build's — build-release-local.sh, or build-release-5104.sh)
# source   ${REVSRC:-unknown} (the tree this script sits in; the binary is whatever --kaspad named)
# log      $LOG   (probe stopped: $([ "$PID_EXITED" = 1 ] && echo cleanly || echo 'SIGKILL'))
EXPECT_FP=$FP
EXPECT_GENESIS=$GEN
PREMINE_TXID=${PREMINE:-__FILL_ME__}
# informational (compare between builds; not read by the kit):
# EXPECT_SCHEDULE_ID=${SCHED:-?}
# RULE_MANIFEST_DIGEST=${MANI:-?}
EOF
[ -n "$DRILL_GEN" ] && echo "DRILL_GENESES+=\" $DRILL_GEN\"   # drill salt id: $(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("salt_id","?"))' "$KR/manifest.json")"
echo "# genesis classes (bytes = the registry's work figure, not the file) and bonds this binary registers (compared with CLASS_8K, CLASS_2M_PREFIX, PREMINE_TXID):"
sed 's/^/#   /' <<<"$GLAYOUT"
[ -n "$LAYOUT_OUT" ] && { echo "# premine layout (t12_deploy_kit_constants: card N = bond N, fee float FEE_FLOAT_BASE+N = ${FEE_FLOAT_BASE:-?}+N):"; sed 's/^/#   /' <<<"$LAYOUT_OUT"; }
[ -z "$kitbad" ] || say "KIT MISMATCH:$kitbad — fix fleet.env.example / lib.sh / app.js at the re-pin (checklist §5) before staging"
if [ -n "$bad" ]; then say "WARNING: this build's genesis ${bad:0:16}… is FORBIDDEN (fleet.env FORBIDDEN_GENESIS / DRILL_GENESES) — install-*.sh will refuse it"; exit 3; fi
[ -z "$kitbad" ] || exit 4
exit 0
