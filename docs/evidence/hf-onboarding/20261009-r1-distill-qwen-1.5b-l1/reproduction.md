# Reproduce 20261009-r1-distill-qwen-1.5b-l1

```bash
# worktree at the tested SHA, then:
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off
cargo build --offline --release --bin kaspad --bin misaka --bin palw-class
export RUN=20261009-r1-distill-qwen-1.5b-l1 RUN_ID=<devnet id>
bash tests/hf-onboarding/devnet.sh up          # salted t12 drill, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}
bash tests/hf-onboarding/devnet.sh user        # U: own key, one funding send, own bond (transient registrar)
bash tests/hf-onboarding/onboard.sh source r1-distill-qwen-1.5b
bash tests/hf-onboarding/onboard.sh preflight r1-distill-qwen-1.5b
bash tests/hf-onboarding/onboard.sh artifact r1-distill-qwen-1.5b        # CACHE=0 for the clean-source build
bash tests/hf-onboarding/onboard.sh conformance r1-distill-qwen-1.5b
bash tests/hf-onboarding/onboard.sh register r1-distill-qwen-1.5b
bash tests/hf-onboarding/onboard.sh observe r1-distill-qwen-1.5b
bash tests/hf-onboarding/onboard.sh summary r1-distill-qwen-1.5b
```
Model pin: `deepseek-ai/DeepSeek-R1-Distill-Qwen-1.5B@ad9f0ae0864d7fbcd1cd905e3c6c5b069cc8b562` (models.json). Binary hashes: environment.json.
