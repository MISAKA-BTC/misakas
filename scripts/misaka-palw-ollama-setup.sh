#!/usr/bin/env bash
# misaka-palw-ollama-setup.sh — provision an Ubuntu VPS as a PALW-Ollama (algo_id = 5) node host.
#
# Run ON the VPS (as a sudo-capable user):
#   GGUF=/tmp/qwen35-2b-f16.gguf bash misaka-palw-ollama-setup.sh
# (MODEL defaults to the pinned class model `misaka-palw-2b-f16`, created from that GGUF.)
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

MODEL="${MODEL:-misaka-palw-2b-f16}"
URL="${URL:-http://127.0.0.1:11434}"
# The v1 class model is CREATED from the canonical F16 GGUF, not pulled from the registry (the
# registry Q8_0 blob was measured non-portable across ISAs). Place the file at $GGUF first —
# distribution is out-of-band (rsync from the release host) — and this script verifies its sha
# and creates the model. `MODEL=qwen3.5:2b` style registry pulls remain supported for probes.
GGUF="${GGUF:-/tmp/qwen35-2b-f16.gguf}"
GGUF_SHA_PIN="575eddc35774ca9ea250541bb7ba4c639e2502941ea6826b52208483b0a42788"

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

if [ "$MODEL" = "misaka-palw-2b-f16" ]; then
  echo "== creating $MODEL from $GGUF =="
  [ -f "$GGUF" ] || { echo "no GGUF at $GGUF — rsync the canonical F16 file here first" >&2; exit 1; }
  SHA=$(sha256sum "$GGUF" | cut -d" " -f1)
  [ "$SHA" = "$GGUF_SHA_PIN" ] || { echo "GGUF sha $SHA != pinned $GGUF_SHA_PIN — refusing" >&2; exit 1; }
  printf 'FROM %s
' "$GGUF" > /tmp/Modelfile.palw-f16
  ollama create "$MODEL" -f /tmp/Modelfile.palw-f16
else
  echo "== pulling $MODEL =="
  ollama pull "$MODEL"
fi
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
# num_ctx 4096, num_gpu 0 = CPU backend, num_predict 16) over a fixed probe seed. Consensus constants live in
# consensus/core/src/pow_layer0.rs (POW_L1_PALW_OLLAMA_*); keep this block in sync with them.
probe() {
  curl -s "$URL/api/generate" -d "{
    \"model\": \"$MODEL\", \"raw\": true, \"stream\": false,
    \"prompt\": \"MISAKA PALW proof-of-work v1\nseed: 00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff\ncontinue:\",
    \"options\": {\"temperature\": 0.0, \"num_predict\": 16, \"num_ctx\": 4096, \"seed\": 0, \"num_gpu\": 0}
  }"
}
R1=$(probe); R2=$(probe)
T1=$(printf '%s' "$R1" | python3 -c "import json,sys; d=json.load(sys.stdin); print(json.dumps([d['response'], d.get('prompt_eval_count'), d.get('eval_count')]))")
T2=$(printf '%s' "$R2" | python3 -c "import json,sys; d=json.load(sys.stdin); print(json.dumps([d['response'], d.get('prompt_eval_count'), d.get('eval_count')]))")
if [ "$T1" != "$T2" ]; then
  echo "DETERMINISM PROBE FAILED: two identical requests produced different outputs." >&2
  echo "Do NOT put this host on the network. Check for a background model update or an Ollama" >&2
  echo "version mismatch (GPU offload is already excluded — the request pins num_gpu = 0)." >&2
  exit 1
fi
# The calibration value is the CANONICAL Layer-1 TAG for the probe seed — byte-for-byte what
# consensus computes (`palw_ollama_l1_tag_from_response`), so an operator's printed line and the
# pinned constant POW_L1_PALW_OLLAMA_CALIBRATION_V1 are the same object rather than two hashes
# that merely correlate. kaspad enforces this at startup; printing it here is for diagnosis.
CAL=$(printf '%s' "$R1" | python3 -c "
import json,sys,hashlib,struct
d=json.load(sys.stdin)
r=d['response'].encode()
h=hashlib.blake2b(key=b'misaka-l1-palw-ollama-v1', digest_size=64)
h.update(b'output'); h.update(struct.pack('<Q', len(r))); h.update(r)
tag=h.digest()+struct.pack('<I', d.get('prompt_eval_count',0))+struct.pack('<I', d.get('eval_count',0))
print(tag.hex())")
echo "== determinism probe OK =="
echo "MISAKA-PALW-OLLAMA-CALIBRATION-v1 arch=$(uname -m) os=$(uname -s) ollama=$(curl -s "$URL/api/version" | python3 -c 'import json,sys;print(json.load(sys.stdin)["version"])') model=${LIST_DIGEST:-unknown} probe=$CAL"
echo "   This probe= value must equal POW_L1_PALW_OLLAMA_CALIBRATION_V1 in"
echo "   calibrated; different ⇒ STOP (version or blob skew — fix before scheduling anything)."
