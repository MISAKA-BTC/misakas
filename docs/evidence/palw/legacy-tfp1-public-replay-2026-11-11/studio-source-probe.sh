#!/bin/sh
set -eu
probe_deps=/tmp/misaka-public-contract-test.fd7edK/debug/deps
source_probe=/tmp/t12-studio-program-source-probe-20261111
studio_case=/tmp/misaka-studio-tfp1-native.k1U16c
rustc --edition 2024 -C debuginfo=0 -C strip=symbols -L "dependency=$probe_deps" \
  --extern "borsh=$probe_deps/libborsh-7cfec46d29ea91a2.rlib" \
  --extern "kaspa_consensus_core=$probe_deps/libkaspa_consensus_core-3cb0f3a40ba7fcd8.rlib" \
  --extern "misaka_palw_tir_artifact=$probe_deps/libmisaka_palw_tir_artifact-de7e0c5458af30f0.rlib" \
  --extern "misaka_palw_tir_exec=$probe_deps/libmisaka_palw_tir_exec-5b46fcfa659f157e.rlib" \
  "$source_probe.rs" -o "$source_probe"
"$source_probe" \
  /Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-135M/run/model.palwtir.testnet-12.palwtir \
  "$studio_case/traces/23d6e48c54bc369c1876f24e4c0fca3975b59a6afef9d277244d458faccbf73f3a51535bff050f315a7ba4542188f72fbd078d222e34f138cfe83cef1d806cde/material.bin" \
  "$studio_case/job.frame" "$studio_case/worker-result.frame" \
  /tmp/misaka-studio-t12-registration-live.iRmft4/candidate-commitment.borsh \
  8e31580dc81f6548b9e440c9b6d45fe63bff1d42ed2dbb4b342bdb58fc4590f0b8210f10a2a23e6c352c866a4313004b547ec363997e5757d2bce31a6838eec4
