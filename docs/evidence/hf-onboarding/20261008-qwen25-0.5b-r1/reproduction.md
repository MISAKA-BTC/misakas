# Reproduce 20261008-qwen25-0.5b-r1

```bash
# worktree at the tested SHA, then:
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off
cargo build --offline --release --bin kaspad --bin misaka --bin palw-class
export RUN=20261008-qwen25-0.5b-r1 RUN_ID=<devnet id>
bash tests/hf-onboarding/devnet.sh up          # salted t12 drill, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}
bash tests/hf-onboarding/devnet.sh user        # U: own key, one funding send, own bond (transient registrar)
bash tests/hf-onboarding/onboard.sh source qwen25-0.5b
bash tests/hf-onboarding/onboard.sh preflight qwen25-0.5b
bash tests/hf-onboarding/onboard.sh artifact qwen25-0.5b        # CACHE=0 for the clean-source build
bash tests/hf-onboarding/onboard.sh conformance qwen25-0.5b
bash tests/hf-onboarding/onboard.sh register qwen25-0.5b
bash tests/hf-onboarding/onboard.sh observe qwen25-0.5b
bash tests/hf-onboarding/onboard.sh summary qwen25-0.5b
```
Model pin: `Qwen/Qwen2.5-0.5B-Instruct@7ae557604adf67be50417f59c2c2f167def9a775` (models.json). Binary hashes: environment.json.
