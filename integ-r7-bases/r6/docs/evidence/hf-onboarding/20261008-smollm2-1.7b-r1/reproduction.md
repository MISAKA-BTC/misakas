# Reproduce 20261008-smollm2-1.7b-r1

```bash
# worktree at the tested SHA, then:
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off
cargo build --offline --release --bin kaspad --bin misaka --bin palw-class
export RUN=20261008-smollm2-1.7b-r1 RUN_ID=<devnet id>
bash tests/hf-onboarding/devnet.sh up          # salted t12 drill, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}
bash tests/hf-onboarding/devnet.sh user        # U: own key, one funding send, own bond (transient registrar)
bash tests/hf-onboarding/onboard.sh source smollm2-1.7b
bash tests/hf-onboarding/onboard.sh preflight smollm2-1.7b
bash tests/hf-onboarding/onboard.sh artifact smollm2-1.7b        # CACHE=0 for the clean-source build
bash tests/hf-onboarding/onboard.sh conformance smollm2-1.7b
bash tests/hf-onboarding/onboard.sh register smollm2-1.7b
bash tests/hf-onboarding/onboard.sh observe smollm2-1.7b
bash tests/hf-onboarding/onboard.sh summary smollm2-1.7b
```
Model pin: `HuggingFaceTB/SmolLM2-1.7B-Instruct@31b70e2e869a7173562077fd711b654946d38674` (models.json). Binary hashes: environment.json.
