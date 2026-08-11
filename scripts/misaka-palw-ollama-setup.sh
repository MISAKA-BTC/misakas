#!/usr/bin/env bash
# misaka-palw-ollama-setup.sh — provision an Ubuntu VPS as a PALW-Ollama (algo_id = 5) node host.
#
# Run ON the VPS (as a sudo-capable user):
#   MODEL=qwen3.5:2b bash misaka-palw-ollama-setup.sh
#
# What it does, idempotently:
#   1. installs Ollama (official installer → systemd service `ollama` on 127.0.0.1:11434)
#   2. pulls the pinned Qwen model and prints its DIGEST — the value the whole fleet must share
#   3. runs the determinism probe: the canonical PoW prompt twice, byte-compared — the same check
#      `scripts/misaka-palw-cpu-calibrate.sh` does for the compute class, adapted to Ollama
#   4. prints the calibration line to compare across fleet hosts OF THE SAME ARCHITECTURE
#
# After every fleet host prints an identical calibration line, export on each (kaspad + miner):
#   export MISAKA_PALW_OLLAMA_MODEL=<model>          # same ref everywhere
#   export MISAKA_PALW_OLLAMA_URL=http://127.0.0.1:11434   # (default; only set if changed)
set -euo pipefail

MODEL="${MODEL:-qwen3.5:2b}"
URL="${URL:-http://127.0.0.1:11434}"

if ! command -v ollama >/dev/null 2>&1; then
  echo "== installing Ollama =="
  curl -fsSL https://ollama.com/install.sh | sh
fi

# The installer registers a systemd unit on Ubuntu; make sure it is up either way.
if command -v systemctl >/dev/null 2>&1 && systemctl list-unit-files ollama.service >/dev/null 2>&1; then
  sudo systemctl enable --now ollama
else
  pgrep -x ollama >/dev/null || (nohup ollama serve >/tmp/ollama-serve.log 2>&1 &)
fi
for _ in $(seq 1 30); do curl -s "$URL/api/version" >/dev/null && break; sleep 1; done
echo "== ollama: $(curl -s "$URL/api/version")"

echo "== pulling $MODEL =="
ollama pull "$MODEL"
DIGEST=$(curl -s "$URL/api/show" -d "{\"model\":\"$MODEL\"}" | python3 -c "
import json,sys
d=json.load(sys.stdin)
print(d.get('details',{}).get('parameter_size',''), d.get('modified_at','')[:19])
" 2>/dev/null || true)
LIST_DIGEST=$(ollama list | awk -v m="$MODEL" '$1==m{print $2}')
echo "== model digest: ${LIST_DIGEST:-unknown}  ($DIGEST)"
echo "   Every fleet host MUST show this digest. A different digest = a different model blob = a"
echo "   node that refutes honest peers."

# Determinism probe — the EXACT consensus request shape (raw, temperature 0, num_predict 48,
# num_ctx 4096) over a fixed probe seed. Consensus constants live in
# consensus/core/src/pow_layer0.rs (POW_L1_PALW_OLLAMA_*); keep this block in sync with them.
probe() {
  curl -s "$URL/api/generate" -d "{
    \"model\": \"$MODEL\", \"raw\": true, \"stream\": false,
    \"prompt\": \"MISAKA PALW proof-of-work v1\nseed: 00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff\ncontinue:\",
    \"options\": {\"temperature\": 0.0, \"num_predict\": 48, \"num_ctx\": 4096, \"seed\": 0}
  }"
}
R1=$(probe); R2=$(probe)
T1=$(printf '%s' "$R1" | python3 -c "import json,sys; d=json.load(sys.stdin); print(json.dumps([d['response'], d.get('prompt_eval_count'), d.get('eval_count')]))")
T2=$(printf '%s' "$R2" | python3 -c "import json,sys; d=json.load(sys.stdin); print(json.dumps([d['response'], d.get('prompt_eval_count'), d.get('eval_count')]))")
if [ "$T1" != "$T2" ]; then
  echo "DETERMINISM PROBE FAILED: two identical requests produced different outputs." >&2
  echo "Do NOT put this host on the network. Check for GPU offload (must be CPU-only on the VPS" >&2
  echo "class), a background model update, or an Ollama version mismatch." >&2
  exit 1
fi
CAL=$(printf '%s' "$T1" | python3 -c "
import hashlib,sys
print(hashlib.sha256(sys.stdin.buffer.read()).hexdigest()[:32])")
echo "== determinism probe OK =="
echo "MISAKA-PALW-OLLAMA-CALIBRATION-v1 arch=$(uname -m) os=$(uname -s) ollama=$(curl -s "$URL/api/version" | python3 -c 'import json,sys;print(json.load(sys.stdin)["version"])') model=${LIST_DIGEST:-unknown} probe=$CAL"
echo "   Compare this LINE across fleet hosts of the same architecture: identical ⇒ the class is"
echo "   calibrated; different ⇒ STOP (version or blob skew — fix before scheduling anything)."
