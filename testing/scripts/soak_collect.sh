#!/usr/bin/env bash
# Collect one sample from every node in a public testnet soak.
#
# Read-only. It runs `regress-rpc` and reads logs; it never restarts anything, never writes to a
# node's data directory, and never sends a message on the network. Run it from a machine with ssh
# to the fleet, on a timer.
#
# Output is one TSV line per node per sample, appended. Deliberately not JSON: the point is that a
# fourteen-day run can be answered with awk at 3am, and that a truncated line is obviously
# truncated.
#
# Usage: soak_collect.sh <fleet-file> [outfile]
#
# The fleet file is one node per line:
#     role  ssh-target                 rpc            logfile
#     base  root@1.2.3.4               127.0.0.1:16110 /var/log/kaspad.log
#     cand  ubuntu@5.6.7.8             127.0.0.1:16110 /var/log/kaspad.log
#
# Lines starting with # are ignored.
set -uo pipefail

FLEET=${1:?usage: soak_collect.sh <fleet-file> [outfile]}
OUT=${2:-soak-samples.tsv}
RPC_BIN=${RPC_BIN:-/var/lib/misaka-regression/src/target/release/regress-rpc}
SSH="ssh -i ${SSH_KEY:-$HOME/.ssh/claude_key} -o BatchMode=yes -o ConnectTimeout=15"

# Header once, so the file explains itself to whoever opens it later.
if [ ! -s "$OUT" ]; then
  printf 'ts\trole\tnode\treachable\tpruning_point\tsink_blue_work\tdaa\tis_synced\tgate\tpanics\tquarantines\tpermits\tbinary\n' > "$OUT"
fi

ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)

while read -r role target rpc logfile; do
  case "$role" in ''|\#*) continue;; esac

  # One ssh per node per sample. Everything the sample needs is gathered in that single call —
  # separate calls would spread one sample across seconds of wall clock and make a partition look
  # like a disagreement.
  line=$($SSH "$target" "
    probe=\$($RPC_BIN $rpc 2>/dev/null || echo '')
    pp=\$(sed -n 's/.*pruning_point=\([0-9a-f]*\).*/\1/p' <<<\"\$probe\")
    bw=\$(sed -n 's/.*sink_blue_work=\([0-9]*\).*/\1/p' <<<\"\$probe\")
    daa=\$(sed -n 's/.*virtual_daa_score=\([0-9]*\).*/\1/p' <<<\"\$probe\")
    sy=\$(sed -n 's/.*is_synced=\([a-z]*\).*/\1/p' <<<\"\$probe\")

    # The gate, as the node itself last described it. There is no RPC for this yet; see
    # docs/testing/public-testnet-soak.md, 'Known gap'.
    gate=\$(grep -a 'Chain participation' '$logfile' 2>/dev/null | tail -1 | sed -n 's/.*state=\([a-z-]*\).*/\1/p')
    [ -z \"\$gate\" ] && grep -qa 'reviewing the chain just adopted' <(tail -200 '$logfile' 2>/dev/null) && gate=candidate-review
    [ -z \"\$gate\" ] && gate=ready

    # Counts, not occurrences: a soak asks whether these ever happened, and a rising count is the
    # answer. Cheap enough to run every minute even on a large log.
    panics=\$(grep -ac 'panicked at' '$logfile' 2>/dev/null || echo 0)
    quar=\$(grep -ac 'QUARANTINED' '$logfile' 2>/dev/null || echo 0)
    permits=\$(grep -ac 'RecoveryPermitGranted' '$logfile' 2>/dev/null || echo 0)
    # The pid comes from the RPC port's listener, not from pgrep: these hosts also run
    # regression fixtures named kaspad, and 'newest kaspad' is usually one of those.
    port='$rpc'; port=\${port##*:}
    pid=\$(ss -tlnp 2>/dev/null | grep \":\$port \" | sed -n 's/.*pid=\([0-9]*\).*/\1/p' | head -1)
    bin=\$(sha256sum \$(readlink -f /proc/\${pid:-0}/exe 2>/dev/null) 2>/dev/null | cut -c1-12)

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      \"\${pp:0:16}\" \"\${bw:-}\" \"\${daa:-}\" \"\${sy:-}\" \"\$gate\" \"\$panics\" \"\$quar\" \"\$permits\" \"\${bin:-unknown}\"
  " 2>/dev/null)

  if [ -z "$line" ]; then
    printf '%s\t%s\t%s\tno\t\t\t\t\t\t\t\t\t\n' "$ts" "$role" "$target" >> "$OUT"
  else
    printf '%s\t%s\t%s\tyes\t%s\n' "$ts" "$role" "$target" "$line" >> "$OUT"
  fi
done < "$FLEET"
