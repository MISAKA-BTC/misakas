#!/bin/zsh
# run-class.sh — one class, the whole matrix, one process per sample (a FRESH verifier each time), appended as JSON lines.
#   run-class.sh <label> <container.palwtir> <work-dir> <results.jsonl>
# Env: BIN (opv-meas), CLAIMS, REPS_HONEST (5), REPS_OTHER (3), HTTP_REPS (3), RSS_CAP_GB (14), MIN_FREE_GB (14), PROMPT (3), MAXPOS (8),
#      WEIGHTS (set empty to skip the streaming weights step)
# The host is shared: before every sample the load average and the free+inactive memory are recorded; a sample is skipped (logged)
# when free memory is below MIN_FREE_GB. Nothing here touches another worktree or a live host.
set -u
LABEL=$1; CONT=$2; WORK=$3; OUT=$4
BIN=${BIN:-target/release/opv-meas}
CLAIMS=${CLAIMS:-"honest late early gather elem decode"}
REPS_HONEST=${REPS_HONEST:-5}; REPS_OTHER=${REPS_OTHER:-3}; HTTP_REPS=${HTTP_REPS:-3}
RSS_CAP_GB=${RSS_CAP_GB:-14}; MIN_FREE_GB=${MIN_FREE_GB:-14}
PROMPT=${PROMPT:-3}; MAXPOS=${MAXPOS:-8}
mkdir -p "$WORK"
free_gb() { vm_stat | awk '/page size of/ {ps=$8} /Pages free/ {f=$3} /Pages inactive/ {i=$3} /Pages speculative/ {s=$3} END {printf "%.1f", (f+i+s)*ps/1073741824}'; }
swap_used_gb() { sysctl -n vm.swapusage | sed 's/.*used = \([0-9.]*\)M.*/\1/' | awk '{printf "%.1f", $1/1024}'; }
# One host-state record per step: the host is shared, so every sample is read against its free memory, swap and load.
note() { print -r -- "{\"note\":\"$1\",\"label\":\"$LABEL\",\"free_gb\":$(free_gb),\"swap_used_gb\":$(swap_used_gb),\"load\":\"$(uptime | sed 's/.*load averages*: //')\",\"at\":\"$(date +%H:%M:%S)\"}" >> "$OUT"; }

$BIN static --container "$CONT" --label "$LABEL" >> "$OUT" 2>>"$WORK/err.log"
# The streaming per-parameter work of a verifier (any host RAM): read, commit, Freivalds projection.
[[ -n "${WEIGHTS-1}" ]] && { note "weights start"; $BIN weights --container "$CONT" --label "$LABEL" --rss-cap-gb 7 >> "$OUT" 2>>"$WORK/err.log"; }
[[ -f "$WORK/world/world.json" ]] || { note "build-world start"; $BIN build-world --container "$CONT" --out "$WORK/world" --label "$LABEL" --prompt $PROMPT --max-positions $MAXPOS >> "$OUT" 2>>"$WORK/err.log"; }
[[ -f "$WORK/world/world.json" ]] || { note "build-world failed"; exit 1; }
$BIN micro --file "$CONT" >> "$OUT" 2>>"$WORK/err.log"

run_one() { # claim provider rep
  local claim=$1 prov=$2 rep=$3
  local fg=$(free_gb)
  if (( fg < MIN_FREE_GB )); then note "skip $claim $rep: free ${fg} GB < $MIN_FREE_GB"; return; fi
  note "sample $claim rep $rep"
  $BIN verify --world "$WORK/world" --claim "$claim" --provider "$prov" --cache "$WORK/cache" --rep $rep --rss-cap-gb $RSS_CAP_GB >> "$OUT" 2>>"$WORK/err.log" \
    || note "verify $claim $rep exited $?"
}

for claim in ${=CLAIMS}; do
  n=$REPS_OTHER; [[ $claim == honest ]] && n=$REPS_HONEST
  for ((r=1; r<=n; r++)); do run_one $claim "$WORK/world/provider" $r; done
done

# The same honest and lying-late claims through the reference HTTP provider on localhost.
$BIN serve --dir "$WORK/world/provider" > "$WORK/serve.out" 2>>"$WORK/err.log" &
SPID=$!
sleep 2
URL=$(sed -n 's/.*"url":"\([^"]*\)".*/\1/p' "$WORK/serve.out" | head -1)
if [[ -n "$URL" ]]; then
  note "http provider $URL pid $SPID cpu_before $(ps -o cputime= -p $SPID)"
  for claim in honest late; do
    for ((r=1; r<=HTTP_REPS; r++)); do run_one $claim "$URL" $r; done
  done
  note "http provider cpu_after $(ps -o cputime= -p $SPID)"
fi
kill $SPID 2>/dev/null
# A withheld position: demand, disclose, check again.
fg=$(free_gb)
(( fg >= MIN_FREE_GB )) && $BIN verify --world "$WORK/world" --claim honest --provider "$WORK/world/provider" --cache "$WORK/cache" --withhold-pos 0 --serve --rss-cap-gb $RSS_CAP_GB >> "$OUT" 2>>"$WORK/err.log"
note "class done"
