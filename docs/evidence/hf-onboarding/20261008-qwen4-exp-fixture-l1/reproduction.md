# Reproduce 20261008-qwen4-exp-fixture-l1

```bash
# worktree at the tested SHA, then:
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off
cargo build --offline --release --bin kaspad --bin misaka --bin palw-class
export RUN=20261008-qwen4-exp-fixture-l1 RUN_ID=<devnet id>
bash tests/hf-onboarding/devnet.sh up          # salted t12 drill, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}
bash tests/hf-onboarding/devnet.sh user        # U: own key, one funding send, own bond (transient registrar)
bash tests/hf-onboarding/onboard.sh source qwen4-exp-fixture
bash tests/hf-onboarding/onboard.sh preflight qwen4-exp-fixture
bash tests/hf-onboarding/onboard.sh artifact qwen4-exp-fixture        # CACHE=0 for the clean-source build
bash tests/hf-onboarding/onboard.sh conformance qwen4-exp-fixture
bash tests/hf-onboarding/onboard.sh register qwen4-exp-fixture
bash tests/hf-onboarding/onboard.sh observe qwen4-exp-fixture
bash tests/hf-onboarding/onboard.sh summary qwen4-exp-fixture
```
Model pin: `None@None` (models.json). Binary hashes: environment.json.
