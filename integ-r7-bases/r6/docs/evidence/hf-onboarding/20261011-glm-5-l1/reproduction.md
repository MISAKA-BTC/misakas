# Reproduce 20261011-glm-5-l1

```bash
# worktree at the tested SHA, then:
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off
cargo build --offline --release --bin kaspad --bin misaka --bin palw-class
export RUN=20261011-glm-5-l1 RUN_ID=<devnet id>
bash tests/hf-onboarding/devnet.sh up          # salted t12 drill, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}
bash tests/hf-onboarding/devnet.sh user        # U: own key, one funding send, own bond (transient registrar)
bash tests/hf-onboarding/onboard.sh source glm-5
bash tests/hf-onboarding/onboard.sh preflight glm-5
bash tests/hf-onboarding/onboard.sh artifact glm-5        # CACHE=0 for the clean-source build
bash tests/hf-onboarding/onboard.sh conformance glm-5
bash tests/hf-onboarding/onboard.sh register glm-5
bash tests/hf-onboarding/onboard.sh observe glm-5
bash tests/hf-onboarding/onboard.sh summary glm-5
```
Model pin: `zai-org/GLM-5@c183ef8c61faee82855eca1ed9bb3a9a7ce3b0b2` (models.json). Binary hashes: environment.json.
