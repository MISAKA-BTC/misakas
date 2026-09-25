# deploy-t12/lib.sh — shared by install-<host>.sh. Sourced, never executed.
#
# Rules this file enforces (each one is a past incident):
#  * a launch script is written NEXT TO the old one, names its binary by an absolute versioned path,
#    checks that binary's sha256, that the binary knows every flag it passes and that it runs every
#    duty BY CONSTRUCTION (the duty check below), and exits 78 (RestartPreventExitStatus) instead of
#    crash-looping — ibm's public node crash-looped 108x on a pre-edited script that named a flag its
#    installed binary lacked (09-23);
#  * (09-25) a node's protocol duties are not flags: the panel's seat duties, the execution lane's round
#    blocks and (testnet-12) the chain-registered-class arm run on every node that holds a bond, and the
#    kit no longer passes --palw-panel / --palw-round-lane / --palw-chain-classes. The launch script
#    refuses a binary whose --help does not mark each of them ALWAYS-ON DUTY — an older binary, where
#    each was "default off", would otherwise start with no panel and no lane;
#  * nothing under /etc/systemd changes before `switch`, and `switch` installs the drop-in / unit in
#    the same step that stops the old process and starts the new one;
#  * old chain data is MOVED ASIDE (never deleted by switch); key files are never read, moved or
#    written — only `test -f` / `ls -l`;
#  * scripts are replaced by write-temp + rename, never overwritten in place (bash reads a running
#    script lazily);
#  * (R-core+, ADR-0152 §8.2) a PUBLIC node never carries a drill flag: `--palw-drill-genesis-salt`
#    and every other `--palw-drill-*` flag (and their KASPAD_PALW_DRILL_* environment twins) make the
#    launch script exit 78, the unit drop-in resets the inherited environment so no KASPAD_* variable
#    can add a flag the script does not show, an app dir carrying the drill marker is refused, and a
#    node that announces a drill chain ("PALW DRILL") is stopped by `switch`.
set -euo pipefail

KIT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
[ -f "$KIT_DIR/fleet.env" ] || { echo "ABORT: $KIT_DIR/fleet.env missing — cp fleet.env.example fleet.env and fill it (PLAN.md §4)" >&2; exit 1; }
# shellcheck source=fleet.env.example
. "$KIT_DIR/fleet.env"

TS=$(date +%Y%m%dT%H%M%S)
say()  { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
warn() { printf '[%s] WARNING: %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }
die()  { printf '[%s] ABORT: %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; exit 1; }

REL=""   # set by require_release
# genesis_refusal <128-hex genesis> — prints why a public node must not run it, or nothing
genesis_refusal() {
    local g=$1 f
    for f in $FORBIDDEN_GENESIS; do
        [ "$g" = "$f" ] || continue
        case "$f" in
            f6cc9576*) echo "the PRIVATE t12's genesis f6cc9576… (same card keys, same premine: every private signature and premine spend replays) — PLAN.md §7 Q1, decided (a)";;
            1eaa6c0f*|32a665d6*) echo "a pre-drill's SALTED genesis ${f:0:8}… (a drill chain, never a public one)";;
            *) echo "a retired t12 genesis ${f:0:8}… (fb1074b0 = the live chain being retired, a8cabac4 = 09-23, d73dbf44 = before the 100M row)";;
        esac
        return 0
    done
    for f in ${DRILL_GENESES:-}; do
        [ "$g" = "$f" ] && { echo "a post-launch drill's salted genesis ${f:0:8}… (DRILL_GENESES)"; return 0; }
    done
    return 0
}

require_release() {
    local v bad=0
    for v in REV KASPAD_SHA256 MISAKA_SHA256 PALW_CLASS_SHA256 EXPECT_FP EXPECT_GENESIS PREMINE_TXID HB_ADDR; do
        if [ "${!v:-}" = "__FILL_ME__" ] || [ -z "${!v:-}" ]; then warn "fleet.env: $v is still a placeholder"; bad=1; fi
    done
    [ "$bad" = 0 ] || die "fill fleet.env from the SHIPPING commit's release build first (PLAN.md §4; docs/t12-rcore-launch-checklist.md §4)"
    [[ "$EXPECT_FP" =~ ^[0-9a-f]{64}$ ]] || die "fleet.env: EXPECT_FP must be 64 lowercase hex (the \"Consensus params fingerprint:\" line)"
    [[ "$EXPECT_GENESIS" =~ ^[0-9a-f]{128}$ ]] || die "fleet.env: EXPECT_GENESIS must be the full 128-hex genesis hash"
    [[ "$PREMINE_TXID" =~ ^[0-9a-f]{128}$ ]] || die "fleet.env: PREMINE_TXID must be the full 128-hex t12 premine txid"
    [[ "$HB_ADDR" =~ ^misakatest: ]] || die "fleet.env: HB_ADDR must be a misakatest: address"
    REL="$REL_ROOT/$REV"
    local why; why=$(genesis_refusal "$EXPECT_GENESIS")
    [ -z "$why" ] || die "EXPECT_GENESIS ${EXPECT_GENESIS:0:8}… is $why — this is not the R-core+ regenesis build"
}

sha_of() { sha256sum "$1" | cut -d' ' -f1; }
verify_sha() { # file expected
    local got; got=$(sha_of "$1")
    [ "$got" = "$2" ] || die "sha256 mismatch for $1: got $got, expected $2"
    say "  sha256 OK $(basename "$1") ${got:0:16}…"
}

# ---------------------------------------------------------------------------------------------
# Node table. Each host script defines NODES=( "spec" ... ) with fields
#   id|unit|mode|listen|borsh|json|grpc|evm|share_mib|memmax_gib|produce|heartbeat|seat8k|peers
#   mode    : dropin (the unit exists; ExecStart is replaced by a drop-in) | new (unit file created)
#   memmax  : the unit's MemoryMax in GiB, or '-' for none (MemoryMax=infinity). A crash guard, never a
#             division of the host: the node's memory ledger reads memory.max − memory.current as a second
#             live bound (kaspad palw_backends host_available_bytes_v1), so a MemoryMax that does not clear
#             share + base + artifact page cache + running working sets + 1 GiB refuses duties the share
#             admits (PLAN.md §2). check_memmax refuses the plainly impossible ones at stage.
#   grpc/evm: port or '-'
#   produce : none | floor | 8k
#   heartbeat, seat8k : 0|1
#   peers   : comma-separated host:port list for --addpeer
# ---------------------------------------------------------------------------------------------
parse_node() {
    IFS='|' read -r N_ID N_UNIT N_MODE N_LISTEN N_BORSH N_JSON N_GRPC N_EVM N_SHARE N_MEMMAX N_PRODUCE N_HB N_SEAT8K N_PEERS <<<"$1"
    N_APPDIR="${APPDIR_PREFIX}${N_ID}"
    N_KEY="$KEY_DIR/t12-bond-${N_ID}.key"
    N_LAUNCH="$REL/launch/b${N_ID}.sh"
}

# The protocol duties the binary must run BY CONSTRUCTION (09-25). The kit passes none of these flags;
# every launch script checks that the binary's --help marks each ALWAYS-ON DUTY (write_launch).
DUTY_FLAGS="--palw-panel --palw-round-lane --palw-chain-classes"

# The 2M genesis row (graph-v7@2097152, class 74c67e63…) is CLOSED at launch (ADR-0152 §8.3 item 7 as
# amended by IA-12/U-D1, O-11): no fleet node holds its artifact or produces for it. The kit refuses to
# stage a node that would.
CLASS_2M_PREFIX=74c67e63

build_args() { # fills ARGS from the parsed node
    ARGS=(--testnet --netsuffix=12 "--appdir=$N_APPDIR" --yes
          "--listen=$N_LISTEN" "--rpclisten-borsh=127.0.0.1:$N_BORSH" "--rpclisten-json=127.0.0.1:$N_JSON")
    if [ "$N_GRPC" != "-" ]; then ARGS+=("--rpclisten=127.0.0.1:$N_GRPC"); else ARGS+=(--nogrpc); fi
    if [ "$N_EVM" != "-" ]; then ARGS+=("--evm-rpc-listen=127.0.0.1:$N_EVM"); fi
    ARGS+=(--utxoindex --unsaferpc --disable-upnp --nodnsseed)
    local p
    IFS=',' read -r -a _peers <<<"$N_PEERS"
    for p in "${_peers[@]}"; do [ -n "$p" ] && ARGS+=("--addpeer=$p"); done
    # Every bond is a panel seat with the round lane — BY CONSTRUCTION (09-25): the panel's seat duties
    # (SEAT-R replays, readiness proofs, the DA answers of P2-7, the automatic filers of P2-8 and, with
    # the fee outpoint, the carriers), the execution lane's round blocks and testnet-12's
    # chain-registered-class arm run on every node that holds a bond key and a bond; no flag turns one
    # on or off, so none is passed (DUTY_FLAGS: the launch script's duty check instead). Each duty
    # reserves its figure on the one memory ledger this node's --palw-host-memory-share bounds
    # (PLAN.md §2). What stays here is identity and resources — and whether the node PRODUCES.
    ARGS+=("--palw-producer-key=$N_KEY"
           "--palw-producer-bond=$PREMINE_TXID:$N_ID"
           "--palw-fee-outpoint=$PREMINE_TXID:$((FEE_FLOAT_BASE + N_ID))"
           "--palw-host-memory-share=$((N_SHARE * 1048576))")
    if [ "$N_SEAT8K" = 1 ]; then ARGS+=("--palw-class-artifact=$ART_8K" --palw-verify-class-manifest); fi
    case "$N_PRODUCE" in
        none) ;;
        floor) ARGS+=(--palw-produce) ;;
        8k) [ "$N_SEAT8K" = 1 ] || die "b$N_ID: an 8k producer needs the 8k artifact (seat8k=1)"
            ARGS+=(--palw-produce "--palw-producer-class=$CLASS_8K") ;;
        *) die "b$N_ID: produce must be none|floor|8k (2M is closed at launch), got $N_PRODUCE" ;;
    esac
    if [ "$N_HB" = 1 ]; then ARGS+=("--palw-heartbeat-miner-address=$HB_ADDR"); fi
    local a
    for a in "${ARGS[@]}"; do
        case "$a" in
            --palw-drill*) die "b$N_ID: $a is a DRILL flag — a public testnet-12 node never carries one (ADR-0152 §8.2)" ;;
            --palw-panel|--palw-panel=*|--palw-round-lane|--palw-round-lane=*|--palw-chain-classes|--palw-chain-classes=*)
                die "b$N_ID: $a — a duty is not a flag (09-25): the binary runs it by construction, and the launch script's duty check proves it" ;;
            --palw-producer-class=${CLASS_2M_PREFIX}*|--palw-class-artifact=*2m*|--palw-class-artifact=*2M*)
                die "b$N_ID: $a — the 2M row is closed at launch (§8.3 item 7, O-11)" ;;
            --palw-producer-bond=*|--palw-fee-outpoint=*)
                [[ "${a#*=}" =~ ^[0-9a-f]{128}:[0-9]+$ ]] || die "b$N_ID: $a is not <128-hex premine txid>:<index>" ;;
        esac
    done
}

# check_memmax — refuse a MemoryMax under which the ledger's cgroup term (memory.max − memory.current −
# 1 GiB) could never reach the share: share + the 8k artifact's page cache (charged to the first node that
# faults it in) + the 1 GiB reserve is the floor; PLAN.md §2 derives the real figure (+ base, + the other
# running duties' working sets, + a cache allowance) and kaspad/tests/t12_role_memory_figures.rs prints it.
check_memmax() {
    [ "$N_MEMMAX" = - ] && return 0
    [[ "$N_MEMMAX" =~ ^[0-9]+$ ]] || die "b$N_ID: memmax must be GiB or '-', got $N_MEMMAX"
    local art_mib=0 floor
    [ "$N_SEAT8K" = 1 ] && art_mib=$(( (ART_8K_BYTES + 1048575) / 1048576 ))
    floor=$((N_SHARE + art_mib + 1024))
    [ $((N_MEMMAX * 1024)) -ge "$floor" ] || die "b$N_ID: MemoryMax ${N_MEMMAX}G (${N_MEMMAX}×1024 MiB) < share ${N_SHARE} + artifact ${art_mib} + 1024 = ${floor} MiB — the ledger's cgroup term would refuse duties the share admits (PLAN.md §2)"
}

atomic_write() { # path mode  (content on stdin)
    local tmp; tmp=$(mktemp "$1.tmp.XXXXXX")
    cat >"$tmp"; chmod "$2" "$tmp"; mv -f "$tmp" "$1"
}

write_launch() {
    build_args
    local q a
    q=""
    for a in "${ARGS[@]}"; do q+="  $(printf '%q' "$a") \\"$'\n'; done
    mkdir -p "$REL/launch"
    atomic_write "$N_LAUNCH" 0755 <<EOF
#!/bin/bash
# testnet-12 regenesis (R-core+) — bond $N_ID ($N_UNIT), release $REV. GENERATED by deploy-t12/lib.sh on $TS.
# Do not edit in place: regenerate with \`install-<host>.sh stage\`. \`$N_LAUNCH --check\` validates
# the binary, every flag and the drill refusal without starting anything.
set -u
BIN=$REL/bin/kaspad
EXPECT_SHA=$KASPAD_SHA256
got=\$(sha256sum "\$BIN" 2>/dev/null | cut -d' ' -f1)
if [ "\$got" != "\$EXPECT_SHA" ]; then echo "[launch b$N_ID] binary \$BIN sha256 \${got:-missing} != \$EXPECT_SHA — refusing (exit 78)"; exit 78; fi
[ -f "$N_KEY" ] || { echo "[launch b$N_ID] bond key $N_KEY missing — refusing (exit 78)"; exit 78; }
ARGS=(
$q)
# ADR-0152 §8.2: a public node never carries a drill flag, from the command line or the environment
# (the salt has no environment twin; the other drill knobs do). The unit's drop-in resets the
# inherited environment, so any KASPAD_* seen here was put back by something after it.
for a in "\$@" "\${ARGS[@]}"; do
  case "\$a" in --palw-drill*) echo "[launch b$N_ID] \$a is a DRILL flag — a public testnet-12 node never carries one — refusing (exit 78)"; exit 78 ;; esac
done
envbad=\$(env | grep -oE '^(KASPAD|MISAKA_PALW|PALW)_[A-Z0-9_]*' | tr '\\n' ' ')
if [ -n "\$envbad" ]; then echo "[launch b$N_ID] environment carries \$envbad— the command line above is this node's whole configuration — refusing (exit 78)"; exit 78; fi
if [ -e "$N_APPDIR/misaka-testnet-12/palw-drill-genesis" ]; then echo "[launch b$N_ID] $N_APPDIR holds a DRILL chain (palw-drill-genesis marker) — refusing (exit 78)"; exit 78; fi
HELP=\$("\$BIN" --help 2>&1)
for a in "\${ARGS[@]}"; do
  f=\${a%%=*}
  case "\$f" in --*) grep -qE -- "(^|[[:space:],])\${f}([[:space:]=,<\\[]|\$)" <<<"\$HELP" || { echo "[launch b$N_ID] \$BIN does not know \$f — refusing (exit 78)"; exit 78; } ;; esac
done
# The duty check (09-25): this script passes no duty flag, so the binary must run each duty by
# construction. Its --help marks each such flag ALWAYS-ON DUTY (on the flag's line or the next); a
# binary that does not is one where the duty is "default off", and it would start as a seat that
# answers nothing — refused.
for d in $DUTY_FLAGS; do
  grep -A1 -E -- "^[[:space:]]+\${d}([[:space:]=,<\\[]|\$)" <<<"\$HELP" | grep -q 'ALWAYS-ON DUTY' || { echo "[launch b$N_ID] \$BIN does not run \$d as an always-on duty (an older binary: the duty would be off) — refusing (exit 78)"; exit 78; }
done
if [ "\${1:-}" = "--check" ]; then echo "[launch b$N_ID] OK: sha \${got:0:16}…, \${#ARGS[@]} args, every flag known, every duty always on ($DUTY_FLAGS), no drill flag"; exit 0; fi
exec "\$BIN" "\${ARGS[@]}"
EOF
}

unit_body_service() { # the [Service] lines both modes share
    # `Environment=` / `EnvironmentFile=` with no value RESET what the base unit (or an earlier drop-in)
    # set: kaspad reads ~100 KASPAD_* variables, and one left on an old unit (a KASPAD_PALW_PRODUCE, a
    # KASPAD_PALW_DRILL_TAMPER_LEAF) would add a flag the launch script does not show.
    cat <<EOF
Environment=
EnvironmentFile=
WorkingDirectory=$REL
ExecStart=
ExecStart=$N_LAUNCH
Restart=on-failure
RestartSec=20
RestartPreventExitStatus=78
KillSignal=SIGINT
KillMode=mixed
TimeoutStopSec=180
StandardOutput=journal
StandardError=journal
MemoryHigh=infinity
MemoryMax=$( [ "$N_MEMMAX" = - ] && echo infinity || echo "${N_MEMMAX}G" )
EOF
}

write_unit() { # into $REL/units — nothing under /etc yet
    check_memmax
    mkdir -p "$REL/units"
    if [ "$N_MODE" = dropin ]; then
        atomic_write "$REL/units/$N_UNIT.zz-t12-regenesis.conf" 0644 <<EOF
# deploy-t12 drop-in, release $REV, bond $N_ID. Roll back = delete this file + daemon-reload.
[Unit]
StartLimitIntervalSec=900
StartLimitBurst=4
[Service]
$(unit_body_service)
EOF
    else
        atomic_write "$REL/units/$N_UNIT.service" 0644 <<EOF
# deploy-t12 unit, release $REV, bond $N_ID. Roll back = disable + delete this file + daemon-reload.
[Unit]
Description=MISAKA testnet-12 bond $N_ID (regenesis, release $REV)
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=900
StartLimitBurst=4
[Service]
Type=simple
$(unit_body_service)
[Install]
WantedBy=multi-user.target
EOF
    fi
}

unit_dropin_path() { echo "/etc/systemd/system/$N_UNIT.service.d/zz-t12-regenesis.conf"; }
unit_file_path()   { echo "/etc/systemd/system/$N_UNIT.service"; }

# drill_processes_here — every process on this host that runs a testnet-12 DRILL (the salt on its
# command line, or the rcore drill script). Printed one per line; empty when there is none.
drill_processes_here() {
    { pgrep -af -- '--palw-drill-genesis-salt|misaka-palw-t12-rcore-drill\.sh|t12-drill-kit/' 2>/dev/null || true; } | cut -c1-160
}

# ---------------------------------------------------------------------------------------------
# preflight (read-only)
# ---------------------------------------------------------------------------------------------
preflight_common() { # $1 = MiB this host keeps for everything that is not a t12 kaspad
    local reserve=$1 spec total_share=0 memtotal_kb ok=1 port holder
    say "host $(hostname) — $(nproc) cores, $(free -g | awk '/Mem:/{print $2" GiB RAM, "$7" GiB available"}')"
    df -h / | tail -1 | awk '{print "  disk /: "$4" free ("$5" used)"}'
    local free_gb; free_gb=$(df -BG --output=avail / | tail -1 | tr -dc 0-9)
    [ "$free_gb" -ge 8 ] || { warn "only ${free_gb} GB free on / — the new chain, the 8k artifact and the aside copies need room"; ok=0; }
    for spec in "${NODES[@]}"; do
        parse_node "$spec"
        total_share=$((total_share + N_SHARE))
        if [ -f "$N_KEY" ]; then say "  b$N_ID key present: $(ls -l "$N_KEY" | awk '{print $1, $5"B", $9}')"; else warn "b$N_ID key $N_KEY MISSING"; ok=0; fi
        [ -e "$N_APPDIR/misaka-testnet-12/palw-drill-genesis" ] && { warn "b$N_ID: $N_APPDIR carries a DRILL marker — switch moves it aside, never runs it"; }
        for port in "${N_LISTEN##*:}" "$N_BORSH" "$N_JSON" $( [ "$N_GRPC" != - ] && echo "$N_GRPC" ) $( [ "$N_EVM" != - ] && echo "$N_EVM" ); do
            holder=$(ss -ltnpH "sport = :$port" 2>/dev/null | grep -oE 'users:\(\("[^"]+",pid=[0-9]+' | head -1 || true)
            [ -n "$holder" ] && say "  port $port (b$N_ID) is held now by ${holder#users:((} — switch stops the old unit first; it must be free after that"
        done
    done
    local drills; drills=$(drill_processes_here)
    [ -z "$drills" ] || { warn "a DRILL node runs on this host (ADR-0152 §8.3: never beside a public t12 node) — switch will refuse:"; echo "$drills" >&2; ok=0; }
    memtotal_kb=$(awk '/MemTotal/{print $2}' /proc/meminfo)
    local memtotal_mib=$((memtotal_kb / 1024))
    say "  declared shares: ${total_share} MiB + ${reserve} MiB reserve vs MemTotal ${memtotal_mib} MiB"
    [ $((total_share + reserve)) -le "$memtotal_mib" ] || { warn "shares + reserve exceed this host's memory"; ok=0; }
    if [ -f "$ART_8K" ]; then
        local sz; sz=$(stat -c %s "$ART_8K")
        [ "$sz" = "$ART_8K_BYTES" ] && say "  8k artifact present ($sz B)" || { warn "8k artifact size $sz != $ART_8K_BYTES"; ok=0; }
        [ -f "$ART_8K.palwmanifest" ] || { warn "8k sidecar $ART_8K.palwmanifest missing"; ok=0; }
    elif [ -f "$ART_8K.incoming" ]; then say "  8k artifact staged as .incoming (stage promotes it after sha256)"
    else
        local need=0; for spec in "${NODES[@]}"; do parse_node "$spec"; [ "$N_SEAT8K" = 1 ] && need=1; done
        [ "$need" = 1 ] && { warn "8k artifact missing and a node here is an 8k seat — run distribute-from-mac.sh"; ok=0; }
    fi
    [ "$ok" = 1 ] && say "preflight: OK" || warn "preflight: NOT OK (see warnings)"
}

# ---------------------------------------------------------------------------------------------
# stage: binaries + artifact + launch scripts + unit files, all under $REL_ROOT. No service touched.
# ---------------------------------------------------------------------------------------------
# The build's own probe ($INCOMING/$REV/IDENTITY, written by build-release-local.sh / build-release-5104.sh
# and shipped by distribute-from-mac.sh) against fleet.env's pins: a stale EXPECT_FP / EXPECT_GENESIS /
# PREMINE_TXID is refused at stage, not found at switch after the public unit has stopped.
identity_check() { # <IDENTITY file>
    local f=$1 v got bad=""
    for v in EXPECT_FP EXPECT_GENESIS PREMINE_TXID; do
        got=$(sed -n "s/^$v=//p" "$f" | head -1)
        [ "$got" = "${!v}" ] || bad+=" $v: build ${got:0:16}…, fleet.env ${!v:0:16}…;"
    done
    [ -z "$bad" ] || die "$f (the build's own probe) and fleet.env disagree:$bad fleet.env is not this build's"
}

stage_binaries() { # list of binaries this host needs
    local b exp src
    mkdir -p "$REL/bin"
    if [ -f "$INCOMING/$REV/IDENTITY" ]; then
        identity_check "$INCOMING/$REV/IDENTITY"
        say "  IDENTITY of $REV = fleet.env's EXPECT_FP / EXPECT_GENESIS / PREMINE_TXID"
    else
        warn "no $INCOMING/$REV/IDENTITY — fleet.env's EXPECT_FP / EXPECT_GENESIS / PREMINE_TXID are first checked at switch"
    fi
    for b in "$@"; do
        case $b in
            kaspad) exp=$KASPAD_SHA256 ;; misaka) exp=$MISAKA_SHA256 ;;
            palw-class) exp=$PALW_CLASS_SHA256 ;;
            *) die "unknown binary $b" ;;
        esac
        if [ -f "$REL/bin/$b" ] && [ "$(sha_of "$REL/bin/$b")" = "$exp" ]; then say "  $b already staged"; continue; fi
        src="$INCOMING/$REV/$b"
        [ -f "$src" ] || die "$src missing — run distribute-from-mac.sh"
        verify_sha "$src" "$exp"
        install -m 0755 "$src" "$REL/bin/$b.tmp.$TS" && mv -f "$REL/bin/$b.tmp.$TS" "$REL/bin/$b"
    done
    (cd "$REL/bin" && sha256sum -- * > "$REL/SHA256SUMS")
    say "  staged → $REL/bin ($(tr '\n' ' ' < "$REL/SHA256SUMS" | cut -c1-120)…)"
}

stage_artifact() {
    local need=0 spec
    for spec in "${NODES[@]}"; do parse_node "$spec"; [ "$N_SEAT8K" = 1 ] && need=1; done
    [ "$need" = 1 ] || return 0
    if [ ! -f "$ART_8K" ]; then
        [ -f "$ART_8K.incoming" ] || die "no $ART_8K and no $ART_8K.incoming"
        say "  verifying $ART_8K.incoming (1.8 GB, ~10 s)"
        verify_sha "$ART_8K.incoming" "$ART_8K_SHA256"
        [ -f "$ART_8K.palwmanifest.incoming" ] || die "sidecar $ART_8K.palwmanifest.incoming missing"
        verify_sha "$ART_8K.palwmanifest.incoming" "$MANIFEST_8K_SHA256"
        mv "$ART_8K.palwmanifest.incoming" "$ART_8K.palwmanifest"
        mv "$ART_8K.incoming" "$ART_8K"
        say "  8k artifact promoted"
    else
        if [ "${SKIP_ART_SHA:-0}" = 1 ]; then say "  8k artifact present (sha check skipped)"; else
            say "  verifying $ART_8K (~10 s)"; verify_sha "$ART_8K" "$ART_8K_SHA256"; fi
        [ -f "$ART_8K.palwmanifest" ] || die "$ART_8K.palwmanifest missing (copy it with distribute-from-mac.sh)"
        verify_sha "$ART_8K.palwmanifest" "$MANIFEST_8K_SHA256"
    fi
}

stage_nodes() {
    local spec
    for spec in "${NODES[@]}"; do
        parse_node "$spec"
        write_launch; write_unit
        "$N_LAUNCH" --check || die "b$N_ID launch check failed — nothing was switched"
    done
    say "  launch scripts: $REL/launch/  units: $REL/units/  (nothing under /etc changed)"
}

# ---------------------------------------------------------------------------------------------
# switch helpers
# ---------------------------------------------------------------------------------------------
record_state() { mkdir -p "$STATE_DIR"; echo "$*" >> "$STATE_DIR/switch-$REV.log"; }

stop_unit() { # unit
    local u=$1
    if systemctl is-active --quiet "$u"; then
        say "  stopping $u (SIGINT, up to 180 s)"
        systemctl stop "$u"
    fi
    systemctl is-active --quiet "$u" && die "$u still active after stop"
    return 0
}

aside_ok() { # dir — read-only: may this kit move it? (checked for every dir BEFORE anything is stopped)
    local d=$1
    [ -e "$d" ] || return 0
    [ -L "$d" ] && die "$d is a symlink — refusing to move it"
    case "$d" in /root/.t12*) ;; *) die "refusing to move $d (only /root/.t12* appdirs)";; esac
    if find "$d" -maxdepth 5 \( -name '*.key' -o -name '*mnemonic*' -o -name '*.seed' \) -print -quit | grep -q .; then
        die "$d contains key-like files ($(find "$d" -maxdepth 5 \( -name '*.key' -o -name '*mnemonic*' -o -name '*.seed' \) -print -quit)) — refusing to move it; inspect by hand"
    fi
    return 0
}

aside_dir() { # dir — rename, never delete; refuse anything that could hold a key
    local d=$1
    [ -e "$d" ] || return 0
    case "$d" in *.old-genesis-*|*.pre-*|*.aug2026-*|*.bak*|*.rolledback-*) say "  $d is already an aside copy — left alone"; return 0;; esac
    aside_ok "$d"
    mv "$d" "$d.old-genesis-$TS"
    record_state "ASIDE $d $d.old-genesis-$TS"
    say "  moved aside $d → $d.old-genesis-$TS ($(du -sh "$d.old-genesis-$TS" | cut -f1))"
}

install_unit_for_node() {
    if [ "$N_MODE" = dropin ]; then
        mkdir -p "/etc/systemd/system/$N_UNIT.service.d"
        install -m 0644 "$REL/units/$N_UNIT.zz-t12-regenesis.conf" "$(unit_dropin_path)"
        record_state "DROPIN $N_UNIT $(unit_dropin_path)"
    else
        [ -f "$(unit_file_path)" ] && ! grep -q "deploy-t12 unit" "$(unit_file_path)" && die "$(unit_file_path) exists and is not ours"
        install -m 0644 "$REL/units/$N_UNIT.service" "$(unit_file_path)"
        record_state "NEWUNIT $N_UNIT $(unit_file_path)"
    fi
}

wait_fingerprint() { # unit since-epoch
    local u=$1 since=$2 got="" i
    for i in $(seq 1 60); do
        got=$(journalctl -u "$u" --since "@$since" --no-pager 2>/dev/null | grep -oE 'Consensus params fingerprint: [0-9a-f]{64}' | tail -1 | awk '{print $4}' || true)
        [ -n "$got" ] && break
        systemctl is-failed --quiet "$u" && break
        sleep 3
    done
    if journalctl -u "$u" --since "@$since" --no-pager 2>/dev/null | grep -q 'PALW DRILL'; then
        warn "$u announces a PALW DRILL chain — a public unit must never run one; stopping it"
        systemctl stop "$u" || true
        return 1
    fi
    if [ "$got" = "$EXPECT_FP" ]; then
        say "  $u fingerprint OK ${got:0:16}…"
        return 0
    fi
    warn "$u fingerprint ${got:-<none>} != EXPECT_FP ${EXPECT_FP:0:16}… — stopping it"
    journalctl -u "$u" --since "@$since" --no-pager | tail -25 >&2
    systemctl stop "$u" || true
    return 1
}

start_node() { # parsed node
    local since; since=$(date +%s)
    [ -e "$N_APPDIR" ] && aside_dir "$N_APPDIR"   # a previous attempt of this kit — never resume it
    local port holder
    for port in "${N_LISTEN##*:}" "$N_BORSH" "$N_JSON" $( [ "$N_GRPC" != - ] && echo "$N_GRPC" ) $( [ "$N_EVM" != - ] && echo "$N_EVM" ); do
        holder=$(ss -ltnpH "sport = :$port" 2>/dev/null | head -1 || true)
        [ -z "$holder" ] || die "b$N_ID: port $port is still held (${holder}) — not starting into a bind failure"
    done
    systemctl daemon-reload
    # the drop-in resets Environment=/EnvironmentFile=; anything still here came from a unit file or
    # drop-in that sorts after ours — refuse before the process can read it
    local envs; envs=$(systemctl show -p Environment,EnvironmentFiles --value "$N_UNIT" 2>/dev/null | tr '\n' ' ')
    case "$envs" in *[![:space:]]*) die "b$N_ID: $N_UNIT still carries an environment after the reset (${envs:0:160}) — a drop-in sorting after zz-t12-regenesis.conf sets it; remove it first" ;; esac
    [ "$N_MODE" = new ] && systemctl enable "$N_UNIT" >/dev/null 2>&1 && record_state "ENABLED $N_UNIT"
    say "  starting $N_UNIT (bond $N_ID, share ${N_SHARE} MiB, produce=$N_PRODUCE hb=$N_HB seat8k=$N_SEAT8K)"
    systemctl reset-failed "$N_UNIT" 2>/dev/null || true   # 5.104's seats sit in 'failed' since 09-23 00:30
    systemctl start "$N_UNIT"
    wait_fingerprint "$N_UNIT" "$since" || return 1
    wait_genesis "$N_UNIT" || return 1
    wait_duties "$N_UNIT" "$since"
}

# wait_duties <unit> <since-epoch> — 09-25: every fleet node holds a bond, so its duties must RUN. Read after
# wait_genesis: once the RPC answers, every service has been built, so the journal already holds both the
# node's PLAN ('PALW duties (on by construction; …)') and, where a key or bond failed to load, its
# correction ('PALW duties NOT as planned: …') — the LAST 'PALW duties' line is the one read. The panel's
# own word (getPalwNodeStatus.panelRunning, set when its worker starts) is asked too. Warns; never stops.
wait_duties() {
    local u=$1 since=$2 duties j
    duties=$(journalctl -u "$u" --since "@$since" --no-pager -o cat 2>/dev/null | grep -oE 'PALW duties .*' | tail -1 || true)
    case "$duties" in
        *"panel seat duties ON"*"receipts only"*) warn "$u: the panel runs RECEIPTS ONLY (no --palw-fee-outpoint reached it) — ${duties:0:300}" ;;
        *"panel seat duties ON"*"round blocks ON"*) say "  $u duties (plan): panel ON, execution lane ON" ;;
        "") warn "$u printed no 'PALW duties' line — is this the release binary? (the launch script's duty check should have refused an older one)" ;;
        *) warn "$u: a duty is NOT running — ${duties:0:300}" ;;
    esac
    t12check_expect
    for j in $(seq 1 10); do
        if python3 "$KIT_DIR/t12check.py" --port "$N_JSON" "${EXPECT_ARGS[@]}" --expect-panel >/dev/null 2>&1; then
            say "  $u duties (running): panel worker started (getPalwNodeStatus.panelRunning)"; return 0
        fi
        sleep 3
    done
    warn "$u: getPalwNodeStatus says the panel is NOT running although the node holds bond $N_ID — see 'panel service disabled' in its journal"
    python3 "$KIT_DIR/t12check.py" --port "$N_JSON" "${EXPECT_ARGS[@]}" --expect-panel >&2 || true
    return 0
}

# the checker's expectations: the release identity AND the kit's copies of chain facts a node never checks
# at start (the genesis bonds on PREMINE_TXID:0..7, the 8k class CLASS_8K registered, a class under the 2M
# row's prefix) — a merge that moved one of them fails the switch gate here instead of idling a producer later.
# (ART_8K_BYTES is the FILE's size and is pinned to the committed sidecar by t12_deploy_kit_constants; the
# registry's artifactBytes is the work derivation's figure — 2,620,391,424 for both dense rows — not the file.)
t12check_expect() { # sets EXPECT_ARGS
    EXPECT_ARGS=(--expect-fp "$EXPECT_FP" --expect-genesis "$EXPECT_GENESIS"
                 --expect-premine "$PREMINE_TXID" --expect-class "$CLASS_8K" --expect-class-prefix "$CLASS_2M_PREFIX")
}

wait_genesis() { # unit — the node's own RPC names EXPECT_GENESIS (a fresh node's pruning point IS its genesis)
    local u=$1 i
    t12check_expect
    for i in $(seq 1 20); do
        if python3 "$KIT_DIR/t12check.py" --port "$N_JSON" "${EXPECT_ARGS[@]}" --registry >/dev/null 2>&1; then
            say "  $u genesis OK ${EXPECT_GENESIS:0:16}…, bonds on ${PREMINE_TXID:0:16}…:0..7, class ${CLASS_8K:0:16}… registered (json 127.0.0.1:$N_JSON)"; return 0
        fi
        sleep 3
    done
    warn "$u does not answer with genesis ${EXPECT_GENESIS:0:16}… / the kit's bonds and classes on 127.0.0.1:$N_JSON — stopping it"
    python3 "$KIT_DIR/t12check.py" --port "$N_JSON" "${EXPECT_ARGS[@]}" --registry >&2 || true
    systemctl stop "$u" || true
    return 1
}

switch_node() { # parsed node: record prior state, stop, install unit, start
    local prior_active prior_enabled
    prior_active=$(systemctl show -p ActiveState --value "$N_UNIT" 2>/dev/null || echo unknown)
    prior_enabled=$(systemctl is-enabled "$N_UNIT" 2>/dev/null || true)
    record_state "PRIOR $N_UNIT active=$prior_active enabled=${prior_enabled:-none}"
    stop_unit "$N_UNIT"
    install_unit_for_node
}

# ---------------------------------------------------------------------------------------------
# post-start checks (read-only)
# ---------------------------------------------------------------------------------------------
# cgroup_memory <unit> — the unit's memory.max / memory.current (anon + file) and its main process's RSS:
# the figures the ledger's cgroup term reads (PLAN.md §2). Warns when that term is below the share.
cgroup_memory() {
    local u=$1 cg d max cur anon file pid rss_kb live
    cg=$(systemctl show -p ControlGroup --value "$u" 2>/dev/null); d="/sys/fs/cgroup${cg}"
    [ -n "$cg" ] && [ -r "$d/memory.current" ] || { say "  cgroup: (no cgroup v2 memory files for $u)"; return 0; }
    max=$(cat "$d/memory.max"); cur=$(cat "$d/memory.current")
    anon=$(awk '$1=="anon"{print $2}' "$d/memory.stat"); file=$(awk '$1=="file"{print $2}' "$d/memory.stat")
    pid=$(systemctl show -p MainPID --value "$u"); rss_kb=$(awk '/VmRSS/{print $2}' "/proc/$pid/status" 2>/dev/null || echo 0)
    say "  cgroup: memory.max $([ "$max" = max ] && echo none || echo "$((max / 1048576)) MiB"), current $((cur / 1048576)) MiB (anon $((anon / 1048576)), file/page cache $((file / 1048576))), process RSS $((${rss_kb:-0} / 1024)) MiB"
    if [ "$max" != max ]; then
        live=$(( (max - cur) / 1048576 - 1024 ))
        [ "$live" -ge "$N_SHARE" ] || warn "b$N_ID: memory.max − memory.current − 1 GiB = ${live} MiB < share ${N_SHARE} MiB — the ledger is bound by the cgroup, not the share (page cache ${file:+$((file / 1048576)) MiB}); PLAN.md §2: raise this node's memmax or set '-'"
    fi
}

check_nodes() {
    local spec rc=0
    t12check_expect
    for spec in "${NODES[@]}"; do
        parse_node "$spec"
        say "== b$N_ID $N_UNIT: $(systemctl show -p ActiveState,SubState,NRestarts --value "$N_UNIT" | tr '\n' ' ')"
        python3 "$KIT_DIR/t12check.py" --port "$N_JSON" "${EXPECT_ARGS[@]}" --expect-panel ${CHECK_REGISTRY:+--registry} || rc=1
        cgroup_memory "$N_UNIT"
        journalctl -u "$N_UNIT" --since "-15min" --no-pager -o cat 2>/dev/null \
            | grep -E 'WrongGenesis|WrongConsensusParams|panicked|ERROR|class manifest|does not know|refusing \(exit 78\)|PALW DRILL|memory ledger cannot cover|panel idle|execution lane idle|deprecated and does nothing|round lane is not produced|panel service disabled|NOT as planned|receipts only' \
            | sed -E 's/^[0-9-]+ [0-9:.+]+ //' | cut -c1-200 | sort | uniq -c | sort -rn | head -6 || true
    done
    return $rc
}

# ---------------------------------------------------------------------------------------------
# rollback (reads the state log switch wrote)
# ---------------------------------------------------------------------------------------------
rollback_host() {
    local log="$STATE_DIR/switch-$REV.log" spec line kind a b
    [ -f "$log" ] || die "no $log — nothing was switched with REV=$REV here"
    for spec in "${NODES[@]}"; do
        parse_node "$spec"
        stop_unit "$N_UNIT" || true
        if [ "$N_MODE" = dropin ]; then rm -f "$(unit_dropin_path)"; say "  removed $(unit_dropin_path)"
        else systemctl disable "$N_UNIT" >/dev/null 2>&1 || true
             grep -q "deploy-t12 unit" "$(unit_file_path)" 2>/dev/null && rm -f "$(unit_file_path)" && say "  removed $(unit_file_path)"
        fi
        [ -e "$N_APPDIR" ] && mv "$N_APPDIR" "$N_APPDIR.rolledback-$TS" && say "  new-chain data kept as $N_APPDIR.rolledback-$TS"
    done
    systemctl daemon-reload
    # restore the OLD chain's appdirs (only those; a previous attempt's .t12r-b* stays aside)
    local d
    for d in "${OLD_APPDIRS[@]}"; do
        b=$(grep "^ASIDE $d " "$log" | tail -1 | awk '{print $3}')
        if [ -n "$b" ] && [ -e "$b" ] && [ ! -e "$d" ]; then mv "$b" "$d"; say "  restored $d"; fi
    done
    # units switch disabled (ibm's misaka-t11-node0) are enabled again
    grep '^DISABLED ' "$log" | while read -r kind a; do systemctl enable "$a" >/dev/null 2>&1 && say "  re-enabled $a"; done
    # restart only what was running before the FIRST switch of this REV (a retried switch records the
    # new node as "active", and 5.104's old seat scripts crash-loop on their installed binary)
    grep '^PRIOR ' "$log" | awk '!seen[$2]++' | while read -r kind a b _; do
        if [ "$b" = "active=active" ]; then say "  starting $a (was active before switch)"; systemctl start "$a" || warn "$a did not start"; fi
    done
    say "rollback done — old units run their untouched old scripts/binaries"
}

purge_old() {
    local log="$STATE_DIR/switch-$REV.log" kind a b
    [ "${CONFIRM_PURGE:-}" = yes ] || die "purge deletes the old chain's data (rollback becomes impossible). Re-run with CONFIRM_PURGE=yes"
    [ -f "$log" ] || die "no $log"
    grep '^ASIDE ' "$log" | while read -r kind a b; do
        case "$b" in /root/.t12*.old-genesis-*) ;; *) warn "skipping unexpected path $b"; continue;; esac
        [ -d "$b" ] && { say "  deleting $b ($(du -sh "$b" | cut -f1))"; rm -rf --one-file-system "$b"; }
    done
}

# ---------------------------------------------------------------------------------------------
# the host-level switch: every old unit stopped and re-pointed in one pass, old data aside, then
# the new nodes started one by one, each gated on its own fingerprint line
# ---------------------------------------------------------------------------------------------
switch_host() { # $1 = seconds between node starts
    local gap=$1 spec d
    [ -d "$REL/launch" ] || die "$REL/launch missing — run \`stage\` first"
    for spec in "${NODES[@]}"; do
        parse_node "$spec"
        "$N_LAUNCH" --check || die "b$N_ID: launch check failed — nothing was switched"
        if [ "$N_MODE" = dropin ]; then [ -f "$REL/units/$N_UNIT.zz-t12-regenesis.conf" ] || die "staged drop-in for $N_UNIT missing"
        else [ -f "$REL/units/$N_UNIT.service" ] || die "staged unit $N_UNIT missing"; fi
    done
    for d in "${OLD_APPDIRS[@]}"; do aside_ok "$d"; done       # refuse BEFORE anything is stopped
    local drills; drills=$(drill_processes_here)
    [ -z "$drills" ] || { echo "$drills" >&2; die "a testnet-12 DRILL runs on this host — a public node never runs beside one (ADR-0152 §8.3 item 3). Stop the drill first."; }
    say "1/3 stop every old unit and install its new ExecStart (script + binary switch together)"
    for spec in "${NODES[@]}"; do parse_node "$spec"; switch_node; done
    systemctl daemon-reload
    say "2/3 move the old chain data aside (never deleted here; keys never touched)"
    for d in "${OLD_APPDIRS[@]}"; do aside_dir "$d"; done
    say "3/3 start the new nodes, ${gap}s apart, each gated on its fingerprint"
    for spec in "${NODES[@]}"; do
        parse_node "$spec"
        start_node || die "b$N_ID did not come up on EXPECT_FP — the nodes after it were NOT started. Fix, or \`rollback\`."
        sleep "$gap"
        systemctl is-active --quiet "$N_UNIT" || { journalctl -u "$N_UNIT" -n 30 --no-pager -o cat >&2; die "b$N_ID exited within ${gap}s of starting (manifest check? bind?) — the nodes after it were NOT started"; }
    done
    say "switch done on $(hostname); running checks"
    check_nodes || warn "a check failed — read the lines above"
}

# ---------------------------------------------------------------------------------------------
# DNS seeder (unit misaka-dnsseeder-t12 on all four hosts) — read-only status only. The running
# 1174b965 build has no genesis dependency, so the regenesis needs no seeder change (SEEDERS.md §1).
# ---------------------------------------------------------------------------------------------
SEEDER_UNIT=misaka-dnsseeder-t12
seeder_status() {
    local pid
    say "$SEEDER_UNIT: $(systemctl show -p ActiveState,SubState,NRestarts --value $SEEDER_UNIT | tr '\n' ' ')"
    pid=$(systemctl show -p MainPID --value $SEEDER_UNIT)
    [ "${pid:-0}" != 0 ] && say "  running binary sha256 $(sha_of /proc/"$pid"/exe | cut -c1-16)… ($(readlink /proc/"$pid"/exe))"
    ss -lunpH 'sport = :53' 2>/dev/null | grep -F dnsseed | awk '{print "  udp listen "$4}' || true
    journalctl -u $SEEDER_UNIT -n 4 --no-pager | cut -c1-200 | sed 's/^/  /'
}
# Swapping or rolling back a seeder is NOT done here: seeders/30-swap.sh and 50-rollback.sh (see
# ../SEEDERS.md) own that, one host at a time, from the Mac. One mechanism, not two.

usage_common() {
    cat <<EOF2
usage: $0 <command>
  preflight        read-only: resources, keys (ls -l), ports, artifact, conflicting units
  stage            copy + verify binaries/artifact, write launch scripts and units under $REL_ROOT (no service touched)
  switch           stop old units, install the new ExecStart, move old chain data aside, start new nodes, check
  check            read-only post-start checks (fingerprint, genesis, peers, lane mix, memory); CHECK_REGISTRY=1 adds classes
  rollback         undo switch for this REV (old scripts/binaries were never modified)
  purge-old        delete the old chain data switch moved aside (CONFIRM_PURGE=yes; rollback impossible after)
  seeder-status    read-only (swap/rollback of a seeder: seeders/30-swap.sh, 50-rollback.sh from the Mac)
EOF2
}

# ---------------------------------------------------------------------------------------------
# dispatcher: host scripts define NODES, OLD_APPDIRS, RESERVE_MIB, BINARIES,
# HOST_NAME_EXPECTED, START_GAP and the hooks host_preflight / host_guard_switch / host_cmd
# ---------------------------------------------------------------------------------------------
main_dispatch() {
    [ "$(id -u)" = 0 ] || die "run as root"
    [ "$(hostname)" = "$HOST_NAME_EXPECTED" ] || die "this is $(hostname), not $HOST_NAME_EXPECTED — wrong host for $0"
    local cmd=${1:-help}
    case "$cmd" in
        preflight) REL="$REL_ROOT/${REV}"; preflight_common "$RESERVE_MIB"; host_preflight ;;
        stage)     require_release; host_preflight
                   stage_binaries "${BINARIES[@]}"; stage_artifact; stage_nodes
                   say "stage done — nothing running was touched" ;;
        switch)    require_release; host_guard_switch; switch_host "$START_GAP" ;;
        check)     require_release; check_nodes; seeder_status ;;
        rollback)  require_release; rollback_host ;;
        purge-old) require_release; purge_old ;;
        seeder-status)   seeder_status ;;
        help|-h|--help)  usage_common; host_usage ;;
        *) require_release; host_cmd "$@" ;;
    esac
}
