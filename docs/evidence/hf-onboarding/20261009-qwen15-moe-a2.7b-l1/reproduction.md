# Reproduce 20261009-qwen15-moe-a2.7b-l1

```bash
# worktree at the tested SHA, then:
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off
cargo build --offline --release --bin kaspad --bin misaka --bin palw-class
export RUN=20261009-qwen15-moe-a2.7b-l1 RUN_ID=<devnet id>
bash tests/hf-onboarding/devnet.sh up          # salted t12 drill, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}
bash tests/hf-onboarding/devnet.sh user        # U: own key, one funding send, own bond (transient registrar)
bash tests/hf-onboarding/onboard.sh source qwen15-moe-a2.7b
bash tests/hf-onboarding/onboard.sh preflight qwen15-moe-a2.7b
bash tests/hf-onboarding/onboard.sh artifact qwen15-moe-a2.7b        # CACHE=0 for the clean-source build
bash tests/hf-onboarding/onboard.sh conformance qwen15-moe-a2.7b
bash tests/hf-onboarding/onboard.sh register qwen15-moe-a2.7b
bash tests/hf-onboarding/onboard.sh observe qwen15-moe-a2.7b
bash tests/hf-onboarding/onboard.sh summary qwen15-moe-a2.7b
```
Model pin: `Qwen/Qwen1.5-MoE-A2.7B-Chat@ec052fda178e241c7c443468d2fa1db6618996be` (models.json). Binary hashes: environment.json.
