#!/bin/bash
# Run a command and kill it if its resident memory exceeds a limit (the incident rule of 2026-10-01: nothing of the
# corpus lane may grow past its cap; a runaway model build once took a Mac down).
#
#   guard_run.sh LIMIT_MB OUTFILE command [args...]
#
# stdout and stderr go to OUTFILE; the last line is `finished` (and `KILLED rss=...` before it when the cap was hit).
LIMIT=$1; OUT=$2; shift 2
"$@" > "$OUT" 2>&1 &
PID=$!
while kill -0 $PID 2>/dev/null; do
  RSS=$(ps -o rss= -p $PID 2>/dev/null | tr -d ' ')
  if [ -n "$RSS" ] && [ "$RSS" -gt $((LIMIT*1024)) ]; then kill -9 $PID; echo "KILLED rss=${RSS}KB" >> "$OUT"; break; fi
  sleep 0.5
done
wait $PID 2>/dev/null
echo "finished" >> "$OUT"
