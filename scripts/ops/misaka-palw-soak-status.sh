#!/usr/bin/env bash
# misaka-palw-soak-status.sh — one-line health of this host's soak node/miner (gate 4).
# Run over SSH from the orchestrator: prints chain height (mined+accepted), last bits,
# unit states, host pressure. Read-only.
set -u
APPDIR="${APPDIR:-$HOME/.palw-soak}"
mined=$(grep -c "mined block #" "$APPDIR/miner.log" 2>/dev/null || echo 0)
last=$(grep "mined block #" "$APPDIR/miner.log" 2>/dev/null | tail -1 | sed -n 's/.*daa_score=\([0-9]*\), bits=\(0x[0-9a-f]*\).*blue_score=\([0-9]*\).*/daa=\1 bits=\2 bs=\3/p')
accepted=$(awk '/Accepted block /{n+=1} /Accepted [0-9]+ blocks/{for(i=1;i<=NF;i++) if($i=="Accepted"){n+=$(i+1); break}} /Unorphaned [0-9]+ block/{for(i=1;i<=NF;i++) if($i=="Unorphaned"){n+=$(i+1); break}} END{print n+0}' "$APPDIR/kaspad.log" 2>/dev/null)
node_state=$( (systemctl is-active palw-soak-node 2>/dev/null) || (kill -0 "$(cat "$APPDIR/kaspad.pid" 2>/dev/null)" 2>/dev/null && echo active) || echo dead)
miner_state=$( (systemctl is-active palw-soak-miner 2>/dev/null) || (kill -0 "$(cat "$APPDIR/miner.pid" 2>/dev/null)" 2>/dev/null && echo active) || echo none)
panics=$(grep -c -i "panic" "$APPDIR/kaspad.log" 2>/dev/null || echo 0)

# PALW verification headroom (ADR-0041) — the margin that decides whether this network can admit
# new nodes at all. One implementation, in misaka-palw-headroom.sh, so this line and any other
# caller cannot drift apart.
palw=$(bash "$(dirname "$0")/misaka-palw-headroom.sh" "$APPDIR" 2>/dev/null || echo "headroom=?x")

echo "node=$node_state miner=$miner_state mined=$mined accepted=${accepted:-0} $last panics=$panics $palw load=$(cut -d' ' -f1 /proc/loadavg) avail=$(awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo)MB swap=$(awk '/SwapTotal/{t=$2}/SwapFree/{f=$2}END{print int((t-f)/1024)}' /proc/meminfo)MB"