#!/usr/bin/env bash
# Reproduce (or refute) the derived-artifact binding gap, end to end, with the built binary.
#
# THE DEFECT: `palw-derive verify` recomputed dsl_hash/artifact_hash from the caller's ANSWER
# BYTES and output_root from the caller's TOKEN IDS, and joined them nowhere. An executor could
# take its own claim's root, bake it into an artifact derived from an unrelated DSL, and every
# shipped verifier called the result `consistent`.
#
# MEASURED ON THE PRE-FH TREE (f77fd145), by this script:
#     dsl_hash_matches: True   artifact_hash_matches: True
#     output_root_matches: True   verdict: consistent
#     binding_checked: ABSENT — the build did not claim to check it
# for a MIDI derived from corpus/music/01-single-note.json bound to
# [151645, 9707, 11, 1879, 0, 151643].
#
# AFTER FH this must print either a refusal, or binding_checked:false with a verdict word that is
# NOT "consistent". If it still prints `consistent` with binding_checked absent or true, the fix
# did not land on the path an operator uses.
#
#   usage: scripts/reproduce-unbound-artifact.sh [path-to-palw-derive]
set -uo pipefail
B="${1:-./target/debug/palw-derive}"
[ -x "$B" ] || { echo "no palw-derive at $B — cargo build -p misaka-palw-derive --bin palw-derive"; exit 2; }
CORPUS="misaka-palw-derive/corpus/music/01-single-note.json"
[ -f "$CORPUS" ] || { echo "run from the repo root: $CORPUS not found"; exit 2; }

W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
echo '[151645,9707,11,1879,0,151643]' > "$W/ids.json"
CTX=$(printf 'ab%.0s' $(seq 1 64))          # 128 hex chars, any value: it is the claim's context

echo "1. derive a MIDI from a corpus DSL that has nothing to do with those ids"
"$B" derive --transformer music/smf/v1 --answer "$CORPUS" --out "$W/o1" >/dev/null 2>&1 || exit 1

echo "2. ask what output_root those ids imply — the executor knows this for its OWN claim"
ROOT=$("$B" verify --object "$(ls "$W"/o1/*.borsh | head -1)" --answer "$CORPUS" \
        --output-token-ids "$W/ids.json" --job-context-hash "$CTX" --family qwen36 2>/dev/null \
      | python3 -c "import json,sys; print(json.load(sys.stdin).get('recomputed_output_root',''))")
[ -n "$ROOT" ] || { echo "   could not read recomputed_output_root — the CLI surface moved"; exit 1; }

echo "3. bake that root into an object derived from the UNRELATED DSL"
"$B" derive --transformer music/smf/v1 --answer "$CORPUS" --output-root "$ROOT" --out "$W/o2" >/dev/null 2>&1 || exit 1

echo "4. verify — and read the verdict WORD, not the exit code"
"$B" verify --object "$(ls "$W"/o2/*.borsh | head -1)" --answer "$CORPUS" \
     --output-token-ids "$W/ids.json" --job-context-hash "$CTX" --family qwen36 2>&1 \
| python3 -c "
import json,sys
raw=sys.stdin.read()
try: d=json.loads(raw)
except Exception: print('   (not json) '+raw[:300]); sys.exit(0)
for k in ('dsl_hash_matches','artifact_hash_matches','output_root_matches','binding_checked','verdict','exit_status'):
    if k in d: print(f'   {k}: {d[k]}')
v=str(d.get('verdict','')); bc=d.get('binding_checked')
print()
if v.strip()=='consistent' and bc is not True:
    print('   >>> DEFECT PRESENT: an unrelated artifact verified as consistent.')
else:
    print('   >>> the binding is checked or the verdict is qualified — the gap is closed on this path.')
"
