#!/usr/bin/env python3
"""Rebuild `genesis8_models.json` (the genesis model registry) from anonymous Hub metadata.

Metadata only: `GET /api/models/<repo>/revision/<sha>?blobs=true` (file names, sizes, LFS SHA-256)
for every source at its PINNED revision. No Authorization header, no token, no weight byte.

    python3 scripts/genesis/genesis_registry_build.py            # writes scripts/genesis/genesis8_models.json
    python3 scripts/genesis/genesis_registry_build.py --check    # exit 1 when the committed file differs

The numbering keeps the user's original 1..10 (7 = Gemma 4 26B-A4B and 9 = Qwen3-32B were dropped on
2026-10-15). Every source is the uncensored variant the user chose (huihui abliterated, or Dolphin where
no huihui variant exists).
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE / "genesis8_models.json"
API = "https://huggingface.co/api/models"
SMALL_KEEP = {
    "config.json", "generation_config.json", "tokenizer.json", "tokenizer_config.json", "vocab.json", "merges.txt",
    "special_tokens_map.json", "added_tokens.json", "chat_template.jinja", "preprocessor_config.json",
    "video_preprocessor_config.json", "processor_config.json", "model.safetensors.index.json", "LICENSE",
    "USE_POLICY.md", "README.md", "configuration.json",
}
APACHE_NOTE = "Apache License 2.0 (the full text ships in the artifact: scripts/genesis/licenses/Apache-2.0.txt when the source has no LICENSE file)"

# Conversion profiles. `calib`: documents of this repository tokenised by the model's own tokenizer.
RECURRENT_CALIB = {"docs": ["docs/rfc/0002-palw-tensor-ir.md"], "len": 8192, "chunks": 1}
DENSE_CALIB = {"docs": ["docs/archival.md", "docs/crescendo-guide.md", "CONTRIBUTING.md", "SECURITY.md"], "len": 512, "chunks": 1}

MODELS = [
    dict(id=1, slug="dolphin3-qwen25-1p5b", source_repo="dphn/Dolphin3.0-Qwen2.5-1.5B",
         source_revision="60c996e28ebee1ea3256fa9d40c06e0b20ceca67", source_format="safetensors",
         base_model="Qwen/Qwen2.5-1.5B", base_author="Qwen team (Alibaba Cloud)",
         derivation="fine-tune (Dolphin 3.0)",
         derivation_author="Eric Hartford, Ben Gitter, BlouseJury and Cognitive Computations",
         derivation_note="No huihui 1.5B abliterated variant exists; Dolphin 3.0 is the uncensored 1.5B (user decision 2026-10-15). "
                         "Converted 2026-10-15 with the legacy base0 qwen25-convert --a16 --n-ctx 8192 at release commit 0b1c11b87; "
                         "the unified container wraps that A16 payload without re-quantising.",
         architecture="Qwen2ForCausalLM", model_type="qwen2", family="qwen2-dense",
         license="apache-2.0", target_repo="Misakachain/Dolphin3.0-Qwen2.5-1.5B-PALW",
         stem="Dolphin3.0-Qwen2.5-1.5B-PALW", converter="a16-legacy-wrap",
         convert_options={"n_ctx": 8192},
         legacy_artifact={"repo": "Misakachain/Dolphin3.0-Qwen2.5-1.5B-PALW",
                          "revision": "816208c4f944a403665b94f8d55f4414aacd3ae5",
                          "path": "palw-runtime/dolphin3.0-qwen25-1.5b-a16-8k.palwart",
                          "bytes": 1799359436,
                          "sha256": "894e6914294e0b225a559365914a5e4509ffcd6ac884ca77f7e0f020c4595438",
                          "artifact_digest": "50e8acbe1767a9eb6b0c5b550c60c019a0d9a8824ccc64343bcd0661d55c66bc0b156a696ee200f8881f265e892820f5ae6cdebbc44c09ebcdf8fa80d800c217",
                          "tokenizer_digest": "c41926f508315c992ff1f1547a58e7dbcb4c68466f89da6d7f4891fe019637f96307ce36a2229457f3a477c3089cc04f58e53310bc0638a7629f36d1f07596df",
                          "converter": "qwen25-convert --a16 --n-ctx 8192 @ 0b1c11b87",
                          "faithful": {"top1": "46/57", "top5": "55/57", "rank_corr": 0.8581}},
         e2e_adjudicable=True, tentative=False, components=["decoder"]),
    dict(id=2, slug="huihui-qwen35-2b", source_repo="huihui-ai/Huihui-Qwen3.5-2B-abliterated",
         source_revision="b2e291a65f29a9b148981fa5299caea5d35bd4c8", source_format="safetensors",
         base_model="Qwen/Qwen3.5-2B", base_author="Qwen team (Alibaba Cloud)",
         derivation="abliteration", derivation_author="huihui-ai",
         architecture="Qwen3_5ForConditionalGeneration", model_type="qwen3_5", family="qwen35-gdn-hybrid",
         license="apache-2.0", target_repo="Misakachain/Huihui-Qwen3.5-2B-abliterated-PALW",
         stem="Huihui-Qwen3.5-2B-abliterated-PALW", converter="tir",
         convert_options={"context": 8192, "gdn_core_wide": True, "calib": RECURRENT_CALIB},
         e2e_adjudicable=True, tentative=False, components=["decoder"]),
    dict(id=3, slug="huihui-qwen35-9b", source_repo="huihui-ai/Huihui-Qwen3.5-9B-abliterated",
         source_revision="05b9e7c9b978ba29bdb8f50a49c30e4b91183339", source_format="safetensors",
         base_model="Qwen/Qwen3.5-9B", base_author="Qwen team (Alibaba Cloud)",
         derivation="abliteration", derivation_author="huihui-ai",
         derivation_note="Tentative: no official Qwen3.8 9B exists (user decision pending). The past refusal of this exact "
                         "revision (HF_FIT_OUT_OF_TOLERANCE, KL 0.268) is fixed by gdn_core_wide (KL 0.0039); the fixtures stay.",
         architecture="Qwen3_5ForConditionalGeneration", model_type="qwen3_5", family="qwen35-gdn-hybrid",
         license="apache-2.0", target_repo="Misakachain/Huihui-Qwen3.5-9B-abliterated-PALW",
         stem="Huihui-Qwen3.5-9B-abliterated-PALW", converter="tir",
         convert_options={"context": 8192, "gdn_core_wide": True, "calib": RECURRENT_CALIB},
         e2e_adjudicable=True, tentative=True, components=["decoder"]),
    dict(id=4, slug="huihui-gemma4-12b", source_repo="huihui-ai/Huihui-gemma-4-12B-it-abliterated",
         source_revision="060ea173c4d1e30b636819f5281e7791b6e9be2f", source_format="safetensors",
         base_model="google/gemma-4-12B-it", base_author="Google DeepMind",
         derivation="abliteration", derivation_author="huihui-ai",
         architecture="Gemma4UnifiedForConditionalGeneration", model_type="gemma4_unified", family="gemma4-unified-dense",
         license="apache-2.0", target_repo="Misakachain/Huihui-gemma-4-12B-it-abliterated-PALW",
         stem="Huihui-gemma-4-12B-it-abliterated-PALW", converter="tir",
         convert_options={"context": 8192, "calib": DENSE_CALIB},
         e2e_adjudicable=True, tentative=False, components=["decoder"]),
    dict(id=5, slug="huihui-gpt-oss-20b", source_repo="huihui-ai/Huihui-gpt-oss-20b-BF16-abliterated",
         source_revision="b73ec897fe872d5e3fea830d516520537decdde6", source_format="safetensors",
         base_model="openai/gpt-oss-20b", base_author="OpenAI",
         derivation="abliteration (BF16)", derivation_author="huihui-ai",
         derivation_note="The huihui official BF16 (user decision 2026-10-15); its GGUF/ directory is not used.",
         architecture="GptOssForCausalLM", model_type="gpt_oss", family="gpt-oss-moe",
         license="apache-2.0", target_repo="Misakachain/Huihui-gpt-oss-20b-BF16-abliterated-PALW",
         stem="Huihui-gpt-oss-20b-BF16-abliterated-PALW", converter="tir",
         convert_options={"context": 8192, "calib": DENSE_CALIB},
         e2e_adjudicable=True, tentative=False, components=["decoder"]),
    dict(id=6, slug="huihui-qwen38-27b-coder390", source_repo="huihui-ai/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-GGUF",
         source_revision="a0b17c71d9943529aefe88b83f494bf01c6b4d68", source_format="gguf",
         base_model="nerkyor/Qwen3.8-27B-Coder390-EfficientThink-Opus5.5-GPT6Astra-Grok4.7-DSV4Pro-K3-SFT-RLOO-MTP-DFlash2",
         base_author="nerkyor (community SFT of Qwen/Qwen3.8-27B by the Qwen team, Alibaba Cloud)",
         derivation="abliteration of a community SFT, GGUF", derivation_author="huihui-ai",
         derivation_note="GGUF only. User decision 2026-10-15: converted from the bf16 GGUF, streamed end to end (HTTPS range "
                         "reads at the pinned revision, whole-file SHA-256 over the streamed bytes). The IQ3_S files stay "
                         "registered as an alternative. The vision projector (mmproj-model-bf16.gguf) is pinned here but not "
                         "converted: the genesis class is the text decoder (the image path is a separate prefix stage).",
         aux_files={"mmproj-model-bf16.gguf": "vision-projector (pinned, not converted)"},
         architecture="Qwen3_5ForCausalLM (GGUF qwen35)", model_type="qwen35", family="qwen35-gdn-hybrid",
         license="apache-2.0", target_repo="Misakachain/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW",
         stem="Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW", converter="tir-gguf",
         convert_options={"context": 8192, "gdn_core_wide": True, "calib": RECURRENT_CALIB, "gguf_variant": "bf16"},
         gguf_variants={"bf16": "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-bf16.gguf",
                        "iq3_s1": "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-IQ3_S1.gguf",
                        "iq3_s2": "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-IQ3_S2.gguf",
                        "iq3_s3": "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-IQ3_S3.gguf"},
         tokenizer_source={"repo": "huihui-ai/Huihui-Qwen3.8-27B-abliterated", "revision": "739e3c5b89849f6c238ce1e5b70008612ae42cdd",
                           "files": ["tokenizer.json", "tokenizer_config.json"],
                           "why": "the GGUF repo has no tokenizer.json; calibration tokenises with the Qwen3.8 tokenizer, checked "
                                  "token by token against the GGUF's own tokenizer.ggml.tokens before use"},
         e2e_adjudicable=True, tentative=False, components=["decoder"]),
    dict(id=8, slug="huihui-qwen36-35b-a3b", source_repo="huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated",
         source_revision="8f0ee727aff5e771ea72466d64d13ecd851d2cc7", source_format="safetensors",
         base_model="Qwen/Qwen3.6-35B-A3B", base_author="Qwen team (Alibaba Cloud)",
         derivation="abliteration", derivation_author="huihui-ai",
         derivation_note="Needs a NEW PALWQ361 integer-runtime conversion from safetensors (the legacy qwen36-convert reads GGUF "
                         "only); the published Misakachain/Qwen3.6-35B-A3B-PALW-runtime was built from other weights.",
         architecture="Qwen3_5MoeForConditionalGeneration", model_type="qwen3_5_moe", family="qwen35-gdn-hybrid-moe",
         license="apache-2.0", target_repo="Misakachain/Huihui-Qwen3.6-35B-A3B-abliterated-PALW",
         stem="Huihui-Qwen3.6-35B-A3B-abliterated-PALW", converter="palwq36",
         convert_options={"context": 8192},
         e2e_adjudicable=True, tentative=False, components=["decoder"]),
    dict(id=10, slug="llama33-70b-abliterated", source_repo="huihui-ai/Llama-3.3-70B-Instruct-abliterated",
         source_revision="fa13334669544bab573e0e5313cad629a9c02e2c", source_format="safetensors",
         base_model="meta-llama/Llama-3.3-70B-Instruct", base_author="Meta Platforms, Inc.",
         derivation="abliteration", derivation_author="huihui-ai",
         derivation_note="Not required to be end-to-end adjudicable (user decision 2026-10-15). Llama 3.3 Community License: "
                         "the name begins with \"Llama\", \"Built with Llama\" is displayed, the license and notice ship with it.",
         architecture="LlamaForCausalLM", model_type="llama", family="llama-dense",
         license="llama3.3", target_repo="Misakachain/Llama-3.3-70B-Instruct-abliterated-PALW",
         stem="Llama-3.3-70B-Instruct-abliterated-PALW", converter="tir",
         convert_options={"context": 8192, "calib": DENSE_CALIB},
         e2e_adjudicable=False, tentative=False, components=["decoder"]),
]


def fetch(repo: str, rev: str) -> dict:
    req = urllib.request.Request(f"{API}/{repo}/revision/{rev}?blobs=true", headers={"User-Agent": "misaka-genesis8-registry/1 (metadata only)"})
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(req, timeout=60) as r:
        return json.load(r)


def build() -> dict:
    out = []
    for m in MODELS:
        meta = fetch(m["source_repo"], m["source_revision"])
        if meta.get("sha") != m["source_revision"]:
            raise SystemExit(f"{m['source_repo']}: the Hub answered revision {meta.get('sha')}")
        variants = set((m.get("gguf_variants") or {}).values())
        files = []
        for s in sorted(meta["siblings"], key=lambda s: s["rfilename"]):
            name = s["rfilename"]
            if m["source_format"] == "gguf":
                is_weight = name in variants
            else:
                is_weight = name.endswith(".safetensors") and "/" not in name
            is_aux = name in (m.get("aux_files") or {})
            if not is_weight and not is_aux and name not in SMALL_KEEP:
                continue
            lfs = s.get("lfs") or {}
            kind = "weight" if is_weight else (m["aux_files"][name] if is_aux else "metadata")
            files.append({"path": name, "bytes": s.get("size"), "kind": kind, "lfs_sha256": lfs.get("sha256"), "git_blob": s.get("blobId")})
        kept = {f["path"] for f in files}
        excluded = sorted(s["rfilename"] for s in meta["siblings"] if s["rfilename"] not in kept and s["rfilename"] != ".gitattributes")
        m2 = dict(m)
        m2["gated"] = bool(meta.get("gated"))
        m2["card_license"] = (meta.get("cardData") or {}).get("license")
        m2["card_base_model"] = (meta.get("cardData") or {}).get("base_model")
        m2["weight_bytes"] = sum(f["bytes"] for f in files if f["kind"] == "weight")
        m2["files"] = files
        m2["excluded_files"] = excluded
        out.append(m2)
    return {
        "schema": "misaka.genesis8.models.v2",
        "decision": "user-2026-10-15-genesis8-uncensored-variants (#5 BF16 b73ec897, #6 GGUF a0b17c71)",
        "recorded_from": "anonymous Hub API /api/models/<repo>/revision/<sha>?blobs=true (metadata only; no weights read)",
        "numbering": "ids keep the user's original 1..10; 7 (Gemma 4 26B-A4B) and 9 (Qwen3-32B) were dropped",
        "license_notes": {"apache-2.0": APACHE_NOTE,
                          "llama3.3": "Llama 3.3 Community License Agreement; Acceptable Use Policy at https://www.llama.com/llama3_3/use-policy"},
        "models": out,
    }


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args(argv)
    doc = build()
    text = json.dumps(doc, indent=1, ensure_ascii=False) + "\n"
    if a.check:
        return 0 if OUT.read_text() == text else 1
    OUT.write_text(text)
    for m in doc["models"]:
        print(m["id"], m["source_repo"], m["source_revision"][:12], "%.2f GB" % (m["weight_bytes"] / 1e9), len(m["files"]),
              "excluded:", m["excluded_files"])
    return 0


if __name__ == "__main__":
    sys.exit(main())
