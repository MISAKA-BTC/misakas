# Reproduce 20261008-llama32-1b-r1

```bash
# worktree at the tested SHA, then:
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_LTO=off
cargo build --offline --release --bin kaspad --bin misaka --bin palw-class
export RUN=20261008-llama32-1b-r1 RUN_ID=<devnet id>
bash tests/hf-onboarding/devnet.sh up          # salted t12 drill, fences {'fence_at': 6, 'fence2_at': 10, 'fence3_at': 14, 'tir_at': 16, 'tir2_at': 24, 'int11_at': 26, 'fence4_at': 'unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)'}
bash tests/hf-onboarding/devnet.sh user        # U: own key, one funding send, own bond (transient registrar)
bash tests/hf-onboarding/onboard.sh source llama32-1b
bash tests/hf-onboarding/onboard.sh preflight llama32-1b
bash tests/hf-onboarding/onboard.sh artifact llama32-1b        # CACHE=0 for the clean-source build
bash tests/hf-onboarding/onboard.sh conformance llama32-1b
bash tests/hf-onboarding/onboard.sh register llama32-1b
bash tests/hf-onboarding/onboard.sh observe llama32-1b
bash tests/hf-onboarding/onboard.sh summary llama32-1b
```
Model pin: `unsloth/Llama-3.2-1B-Instruct@5a8abab4a5d6f164389b1079fb721cfab8d7126c` (models.json). Binary hashes: environment.json.
