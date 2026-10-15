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
# Text generation only (user policy 2026-10-15): the image/video/audio processor configs are not fetched or carried.
SMALL_KEEP = {
    "config.json", "generation_config.json", "tokenizer.json", "tokenizer_config.json", "vocab.json", "merges.txt",
    "special_tokens_map.json", "added_tokens.json", "chat_template.jinja", "model.safetensors.index.json", "LICENSE",
    "USE_POLICY.md", "README.md", "configuration.json", "NOTICE",
}
SCOPE = {
    "kind": "text-generation",
    "inputs": "token ids",
    "outputs": "next-token logits",
    "out_of_scope": "vision towers and projectors (mmproj), image/audio/video embedders and processors, multi-token-prediction (MTP) "
                    "heads: not computed, not covered, not admitted as claims, not rewarded; refused by name below the text scope "
                    "(misaka_palw_tir_artifact::palwart_v3::check_text_scope_v3)",
}
APACHE_NOTE = "Apache License 2.0 (the full text ships in the artifact: scripts/genesis/licenses/Apache-2.0.txt when the source has no LICENSE file)"

# Conversion profiles. `calib`: documents of this repository tokenised by the model's own tokenizer (no chat template, no BOS).
#
# Calibration only measures each site's activation range (absmax, per-position and per-channel maxima, sum of squares) on the
# float reference; the integer program, and every adjudication over it, is exact and deterministic for ANY pinned calibration set.
# What the length buys is fit to the float model. A recurrent program (Gated DeltaNet) carries a state whose magnitude grows with
# the position, so it is calibrated on one sequence at least as long as the context it is served at (freeze-v1 §5.2, enforced by
# the converter): `len` IS the served context of a recurrent class. A dense program has no carried state: four 512-token
# documents (2,048 positions).
#
# User decision 2026-10-16: EVERY genesis model is served at a 32K context (32,768 tokens). The GDN classes (#2 #3 #6) are
# therefore calibrated on one 32,768-token sequence (the first 32K tokens of RFC-0002); the dense TIR classes declare 32,768;
# #1 is re-converted at --n-ctx 32768; #8's published runtime serves 512 (its header) and is blocked until re-converted.
GENESIS_CONTEXT = 32768
CALIB_POSITIONS_DEFAULT = GENESIS_CONTEXT
RECURRENT_CALIB = {"kind": "recurrent", "docs": ["docs/rfc/0002-palw-tensor-ir.md"], "len": CALIB_POSITIONS_DEFAULT, "chunks": 1}
DENSE_CALIB = {"kind": "dense", "docs": ["docs/archival.md", "docs/crescendo-guide.md", "CONTRIBUTING.md", "SECURITY.md"], "len": 512,
               "chunks": 1}
# The texts, pinned by git blob at one commit of this repository: a document edited later does not move a conversion's tokens.
# The pipeline extracts each blob (`git cat-file blob`), checks the id, and tokenises the extracted copy. `evaluation` is the
# held-out text of the fit guard (`palw-tir-fidelity --exec`: mean KL <= 0.1 and top-1 >= 0.8 against the float reference);
# `long_evaluation` is its long-context check: the NEXT 32K tokens of RFC-0002 (held out from the calibration's first 32K), every
# position compared, the mean and worst KL of 32 windows along it (the drift curve).
CALIB_TEXTS = {
    "git_commit": "d52d6c56ff33991f1caf67db5cd426dd736d2d29",
    "blobs": {
        "docs/rfc/0002-palw-tensor-ir.md": "7bb6e09f61b27775a9fa467e83f019aee371948a",
        "docs/archival.md": "2bb5730da32226843407b3036c40ed001b26804b",
        "docs/crescendo-guide.md": "9cad9e256809f5e893ce2ec69bde1efae4ae74be",
        "CONTRIBUTING.md": "9b7e5e85eb2e049679cd7ed1bcc46e9b18849047",
        "SECURITY.md": "d75fe2b3c7c52f9a1019a7999a472d64c4ce00d2",
        "README.md": "a3166dc2f915782ccea239f919ddd5aed5c5e7dd",
        "docs/PRINCIPLES.md": "1ab69ff7f2b0e72bec02d453afb9390dfff3607d",
    },
    "evaluation": {"docs": ["README.md", "docs/PRINCIPLES.md"], "len": 64, "chunks": 1},
    "long_evaluation": {"docs": ["docs/rfc/0002-palw-tensor-ir.md"], "len": GENESIS_CONTEXT, "chunks": 2, "skip": 1, "drift_windows": 32},
    "fit_tolerance": {"kl_mean_max": 0.1, "top1_min": 0.8, "window_kl_mean_max": 0.1, "drift_ratio_max": 1.5, "drift_floor_kl": 0.01,
                      "drift": "every window's mean KL <= window_kl_mean_max; late/early mean KL <= drift_ratio_max unless the late "
                              "window's mean KL <= drift_floor_kl (corpus-v1 §9's 1.5x criterion, with a floor so noise at tiny KL "
                              "is not a failure)",
                      "source": "the L2 default tolerance (docs/evidence/hf-onboarding/20261010-huihui-qwen35-9b-fit-diagnosis), "
                                "measured against the lowering's own float reference"},
}

MODELS = [
    dict(id=1, slug="dolphin3-qwen25-1p5b", source_repo="dphn/Dolphin3.0-Qwen2.5-1.5B",
         source_revision="60c996e28ebee1ea3256fa9d40c06e0b20ceca67", source_format="safetensors",
         base_model="Qwen/Qwen2.5-1.5B", base_author="Qwen team (Alibaba Cloud)",
         derivation="fine-tune (Dolphin 3.0)",
         derivation_author="Eric Hartford, Ben Gitter, BlouseJury and Cognitive Computations",
         derivation_note="No huihui 1.5B abliterated variant exists; Dolphin 3.0 is the uncensored 1.5B (user decision 2026-10-15). "
                         "Served at 32K (user decision 2026-10-16): re-converted with the legacy base0 qwen25-convert --a16 "
                         "--n-ctx 32768 (the A16 width is part of the artifact digest) and carried in the unified container. The "
                         "8k A16 file published 2026-10-15 (history_artifact) stays as history.",
         architecture="Qwen2ForCausalLM", model_type="qwen2", family="qwen2-dense",
         license="apache-2.0", target_repo="Misakachain/Dolphin3.0-Qwen2.5-1.5B-PALW", upload_prefix="palwart/",
         stem="Dolphin3.0-Qwen2.5-1.5B-PALW", converter="a16",
         convert_options={"n_ctx": GENESIS_CONTEXT, "a16_file": "dolphin3.0-qwen25-1.5b-a16-32k.palwart"},
         history_artifact={"repo": "Misakachain/Dolphin3.0-Qwen2.5-1.5B-PALW",
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
         convert_options={"context": GENESIS_CONTEXT, "gdn_core_wide": True, "calib": RECURRENT_CALIB},
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
         convert_options={"context": GENESIS_CONTEXT, "gdn_core_wide": True, "calib": RECURRENT_CALIB},
         e2e_adjudicable=True, tentative=True, components=["decoder"]),
    dict(id=4, slug="huihui-gemma4-12b", source_repo="huihui-ai/Huihui-gemma-4-12B-it-abliterated",
         source_revision="060ea173c4d1e30b636819f5281e7791b6e9be2f", source_format="safetensors",
         base_model="google/gemma-4-12B-it", base_author="Google DeepMind",
         derivation="abliteration", derivation_author="huihui-ai",
         architecture="Gemma4UnifiedForConditionalGeneration", model_type="gemma4_unified", family="gemma4-unified-dense",
         license="apache-2.0", target_repo="Misakachain/Huihui-gemma-4-12B-it-abliterated-PALW",
         stem="Huihui-gemma-4-12B-it-abliterated-PALW", converter="tir",
         convert_options={"context": GENESIS_CONTEXT, "calib": DENSE_CALIB},
         e2e_adjudicable=True, tentative=False, components=["decoder"]),
    dict(id=5, slug="huihui-gpt-oss-20b", source_repo="huihui-ai/Huihui-gpt-oss-20b-BF16-abliterated",
         source_revision="b73ec897fe872d5e3fea830d516520537decdde6", source_format="safetensors",
         base_model="openai/gpt-oss-20b", base_author="OpenAI",
         derivation="abliteration (BF16)", derivation_author="huihui-ai",
         derivation_note="The huihui official BF16 (user decision 2026-10-15); its GGUF/ directory is not used.",
         architecture="GptOssForCausalLM", model_type="gpt_oss", family="gpt-oss-moe",
         license="apache-2.0", target_repo="Misakachain/Huihui-gpt-oss-20b-BF16-abliterated-PALW",
         stem="Huihui-gpt-oss-20b-BF16-abliterated-PALW", converter="tir",
         convert_options={"context": GENESIS_CONTEXT, "calib": DENSE_CALIB},
         e2e_adjudicable=True, tentative=False, components=["decoder"]),
    dict(id=6, slug="huihui-qwen38-27b-coder390", source_repo="huihui-ai/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-GGUF",
         source_revision="a0b17c71d9943529aefe88b83f494bf01c6b4d68", source_format="gguf",
         base_model="nerkyor/Qwen3.8-27B-Coder390-EfficientThink-Opus5.5-GPT6Astra-Grok4.7-DSV4Pro-K3-SFT-RLOO-MTP-DFlash2",
         base_author="nerkyor (community SFT of Qwen/Qwen3.8-27B by the Qwen team, Alibaba Cloud)",
         derivation="abliteration of a community SFT, GGUF", derivation_author="huihui-ai",
         derivation_note="GGUF only. User decision 2026-10-15: converted from the bf16 GGUF, streamed end to end (HTTPS range "
                         "reads at the pinned revision, whole-file SHA-256 over the streamed bytes). The IQ3_S files stay "
                         "registered as an alternative. Text generation only: the vision projector is out of scope and not fetched.",
         out_of_scope=["vision projector mmproj-model-bf16.gguf (not fetched)", "MTP block blk.64.nextn.* (dropped by the GGUF reader)"],
         architecture="Qwen3_5ForCausalLM (GGUF qwen35)", model_type="qwen35", family="qwen35-gdn-hybrid",
         license="apache-2.0", target_repo="Misakachain/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW",
         stem="Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW", converter="tir-gguf",
         convert_options={"context": GENESIS_CONTEXT, "gdn_core_wide": True, "calib": RECURRENT_CALIB, "gguf_variant": "bf16"},
         gguf_variants={"bf16": "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-bf16.gguf",
                        "iq3_s1": "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-IQ3_S1.gguf",
                        "iq3_s2": "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-IQ3_S2.gguf",
                        "iq3_s3": "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-IQ3_S3.gguf"},
         tokenizer_source={"repo": "huihui-ai/Huihui-Qwen3.8-27B-abliterated", "revision": "739e3c5b89849f6c238ce1e5b70008612ae42cdd",
                           "files": ["tokenizer.json", "tokenizer_config.json"],
                           "why": "the GGUF repo has no tokenizer.json; calibration tokenises with the Qwen3.8 tokenizer, checked "
                                  "token by token against the GGUF's own tokenizer.ggml.tokens before use"},
         e2e_adjudicable=True, tentative=False, components=["decoder"]),
    dict(id=8, slug="qwen36-35b-a3b-palwq361", source_repo="Misakachain/Qwen3.6-35B-A3B-PALW-runtime",
         source_revision="bbc2f6c3dc7940efe12698a4b09a4cd983e21b38", source_format="palwq361",
         base_model="Qwen/Qwen3.6-35B-A3B", base_author="Qwen team (Alibaba Cloud)",
         derivation="PALWQ361 integer runtime derived from the official weights (NOT an uncensored variant)",
         derivation_author="MISAKA (Misakachain)",
         derivation_note="User decision 2026-10-15: #8 is the EXISTING official-derived PALWQ361 integer runtime, not re-converted "
                         "and the only non-uncensored genesis entry. Its published legacy file is carried byte for byte in a "
                         "PALWART3 index and segments added to the SAME repository under palwart/ (no legacy file replaced).",
         architecture="Qwen3_5MoeForConditionalGeneration", model_type="qwen3_5_moe", family="qwen35-gdn-hybrid-moe",
         license="apache-2.0", target_repo="Misakachain/Qwen3.6-35B-A3B-PALW-runtime", upload_prefix="palwart/",
         stem="Qwen3.6-35B-A3B-PALW", converter="palwq36-legacy-wrap",
         convert_options={"n_ctx": 512},
         served_context=512,
         blocked="the published runtime serves 512 positions: its PALWQ361 header (read 2026-10-16, bytes 0..1023 at bbc2f6c3) says "
                 "max_position 512 and a 512-row rotary table, and the PALWQ361 context is baked into the artifact (shape, rotary "
                 "table, the GDN state exponents' horizon correction). Every genesis model is served at 32,768 (user decision "
                 "2026-10-16): re-convert with qwen36-convert --context 32768 from a pinned qwen35moe GGUF of a source the user "
                 "names (docs/design/palw/genesis8-status.md §5). The repository's own profile pins the Claude-4.7-Opus-abliterated "
                 "Q4_K_M GGUF as its inference artifact, so the published file's provenance needs the user's confirmation too.",
         legacy_artifact={"repo": "Misakachain/Qwen3.6-35B-A3B-PALW-runtime",
                          "revision": "bbc2f6c3dc7940efe12698a4b09a4cd983e21b38",
                          "path": "palw-runtime/qwen36-35b-a3b.palwq36",
                          "bytes": 36492831232,
                          "sha256": "5fe1d15caf0e544b718a4a2398db4f902b9a6722b266dd53824fbc9723578b29",
                          "xet_hash": "4aab592f35f66e211c4c1da91faadeac1232b4f5e4f1dc7b8f98ad4d98c2fcb6",
                          "converter": "the published PALWQ361 runtime (qwen36-convert); not re-run"},
         tokenizer_source={"repo": "Qwen/Qwen3.6-35B-A3B", "revision": "995ad96eacd98c81ed38be0c5b274b04031597b0",
                           "files": ["tokenizer.json", "tokenizer_config.json", "chat_template.jinja", "config.json", "generation_config.json"],
                           "why": "the runtime repository carries no tokenizer: the official tokenizer and chat template of the base model"},
         out_of_scope=["vision tower", "MTP head"],
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
         convert_options={"context": GENESIS_CONTEXT, "calib": DENSE_CALIB},
         e2e_adjudicable=False, tentative=False, components=["decoder"]),
]


# What each family's checkpoint carries beyond text generation (never computed, covered, admitted or rewarded).
OUT_OF_SCOPE_DEFAULT = {
    "qwen2-dense": [],
    "qwen35-gdn-hybrid": ["vision tower model.visual.* (ignored by the adapter)", "MTP head mtp.* (ignored by the adapter)"],
    "qwen35-gdn-hybrid-moe": ["vision tower", "MTP head"],
    "gemma4-unified-dense": ["image embedder model.vision_embedder.* / model.embed_vision.*", "audio embedder model.embed_audio.*"],
    "gpt-oss-moe": [],
    "llama-dense": [],
}


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
            elif m["source_format"] == "palwq361":
                is_weight = name == m["legacy_artifact"]["path"]
            else:
                is_weight = name.endswith(".safetensors") and "/" not in name
            is_aux = False
            if not is_weight and not is_aux and name not in SMALL_KEEP:
                continue
            lfs = s.get("lfs") or {}
            kind = "weight" if is_weight else "metadata"
            files.append({"path": name, "bytes": s.get("size"), "kind": kind, "lfs_sha256": lfs.get("sha256"), "git_blob": s.get("blobId")})
        kept = {f["path"] for f in files}
        excluded = sorted(s["rfilename"] for s in meta["siblings"] if s["rfilename"] not in kept and s["rfilename"] != ".gitattributes")
        m2 = dict(m)
        m2.setdefault("served_context", GENESIS_CONTEXT)
        m2.setdefault("out_of_scope", OUT_OF_SCOPE_DEFAULT.get(m["family"], []))
        m2["scope"] = SCOPE["kind"]
        m2["gated"] = bool(meta.get("gated"))
        m2["card_license"] = (meta.get("cardData") or {}).get("license")
        m2["card_base_model"] = (meta.get("cardData") or {}).get("base_model")
        m2["weight_bytes"] = sum(f["bytes"] for f in files if f["kind"] == "weight")
        m2["files"] = files
        m2["excluded_files"] = excluded if len(excluded) <= 24 else {"count": len(excluded), "note": "the rest of the repository is not read and is left as it is"}
        out.append(m2)
    return {
        "schema": "misaka.genesis8.models.v2",
        "decision": "user-2026-10-15-genesis8 (uncensored variants; #5 BF16 b73ec897; #6 bf16 GGUF a0b17c71; #8 the existing "
                    "official-derived PALWQ361 runtime bbc2f6c3; text generation only); user-2026-10-16 every model served at 32K",
        "served_context": GENESIS_CONTEXT,
        "recorded_from": "anonymous Hub API /api/models/<repo>/revision/<sha>?blobs=true (metadata only; no weights read)",
        "numbering": "ids keep the user's original 1..10; 7 (Gemma 4 26B-A4B) and 9 (Qwen3-32B) were dropped",
        "scope": SCOPE,
        "calibration_texts": CALIB_TEXTS,
        "uncensored": "every entry is an uncensored variant except #8, the existing official-derived PALWQ361 runtime",
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
