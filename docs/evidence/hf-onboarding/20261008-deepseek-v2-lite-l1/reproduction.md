# Reproduce 20261008-deepseek-v2-lite-l1

```bash
# worktree at the tested SHA, then:
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off
cargo build --offline --release --bin kaspad --bin misaka --bin palw-class
export RUN=20261008-deepseek-v2-lite-l1 RUN_ID=<devnet id>
bash tests/hf-onboarding/devnet.sh up          # salted t12 drill, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}
bash tests/hf-onboarding/devnet.sh user        # U: own key, one funding send, own bond (transient registrar)
bash tests/hf-onboarding/onboard.sh source deepseek-v2-lite
bash tests/hf-onboarding/onboard.sh preflight deepseek-v2-lite
bash tests/hf-onboarding/onboard.sh artifact deepseek-v2-lite        # CACHE=0 for the clean-source build
bash tests/hf-onboarding/onboard.sh conformance deepseek-v2-lite
bash tests/hf-onboarding/onboard.sh register deepseek-v2-lite
bash tests/hf-onboarding/onboard.sh observe deepseek-v2-lite
bash tests/hf-onboarding/onboard.sh summary deepseek-v2-lite
```
Model pin: `deepseek-ai/DeepSeek-V2-Lite-Chat@85864749cd611b4353ce1decdb286193298f64c7` (models.json). Binary hashes: environment.json.
