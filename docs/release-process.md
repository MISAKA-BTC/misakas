# Release process

**Operators run a tag, not `main`.** A release is a tag. It carries binaries for every supported
platform, a checksum file, a Sigstore signature over that file, build-provenance attestations and
an SBOM. The node's own startup lines remain the final check: the consensus params fingerprint
and the fence schedule must match the release notes.

Status: the tooling below is in `.github/workflows/deploy.yaml`, but **no release has been cut with
it yet**, and it has not had its first dry run (§2 step 3). testnet-12 shipped as commit `0e8ec984e` with no tag. The versioning and freeze rules in
§1 are a proposal until the maintainers adopt them in [mainnet-readiness.md](mainnet-readiness.md).

## 1. Versions (proposed)

| tag | meaning |
|---|---|
| `v0.9.0-rc.N` | release candidate. Consensus is frozen. After `rc.1`, a consensus rule changes only to fix a Critical safety issue, and that change is a new `rc.N+1` |
| `v1.0.0` | the mainnet release. Genesis and parameters are final |
| `testnet-main-<sha>` | legacy testnet builds (testnet-11 and earlier), kept for history |

A consensus rule change after `rc.1` is recorded in [mainnet-readiness.md](mainnet-readiness.md)
with its fence height and the finding that forced it, and the soak clock restarts.

## 2. Cutting a release (maintainers)

1. Everything is green on the commit: `bash scripts/ci-gates.sh` locally and the `Tests` workflow
   in CI.
2. **`release.json` declares the release's identity**: the network, the consensus params
   fingerprint and schedule id every binary must print, the genesis, and whether consensus is
   frozen. It is the only place the workflow reads these from. When a re-pin moves the
   fingerprint, or the release targets another network (`testnet-12` → `mainnet`), this file
   changes in the same PR. A stale value fails the release: every platform's smoke start compares
   what its binary prints against the file.
3. **Dry-run the release on that exact commit.** Run *Build and upload assets* from the Actions tab
   (`workflow_dispatch`) on the branch that holds the commit. It runs every job below and
   publishes nothing. The files are named `dryrun-<run number>` and kept as workflow artifacts.
   The `verify` job then checks them the way a user would. Tag only a commit whose dry run is
   green, and put the dry run's link in the release notes.
4. Make a **signed, annotated tag** (`git tag -s`, or SSH signing with `gpg.format ssh`), and
   push it:

   ```bash
   git tag -s v0.9.0-rc.1 -m "v0.9.0-rc.1"
   git push origin v0.9.0-rc.1
   ```

5. Publish a GitHub release for the tag, with **pre-release** ticked for an RC. Publishing starts
   `deploy.yaml`:

   | job | produces |
   |---|---|
   | `build` (x86_64 Linux, static musl) | `rusty-kaspa-<tag>-linux-amd64.zip`, `components-x86_64-unknown-linux-musl.json` |
   | `build` (ARM64 Linux, glibc ≥ 2.39) | `rusty-kaspa-<tag>-linux-arm64.zip`, `components-aarch64-unknown-linux-gnu.json` |
   | `build` (x86_64 Windows) | `rusty-kaspa-<tag>-win64.zip`, `components-x86_64-pc-windows-msvc.json` |
   | `build` (ARM64 macOS) | `rusty-kaspa-<tag>-osx.zip`, `components-aarch64-apple-darwin.json` |
   | `build-wasm` | `kaspa-wasm32-sdk-<tag>.zip` |
   | `sign` | `RELEASE-INFO.json`, `SHA256SUMS`, `SHA256SUMS.sigstore.json`, `misakas-<tag>.spdx.json`, and a provenance attestation for every file |
   | `verify` | nothing. It downloads the release from a clean runner and checks the signature, every checksum and every attestation |

   Every `build` leg smoke-starts its own `kaspad` on the network in `release.json`
   (`scripts/misaka-release-smoke.py`), and fails unless the binary prints the declared
   fingerprint and schedule id. `sign` writes `RELEASE-INFO.json` from what the binaries printed
   (`scripts/misaka-release-info.py`), and fails if two platforms printed different identities.
   The run's summary page shows the same table.

   Nothing reaches the GitHub release until `sign` has passed. Then all of it is uploaded at
   once. `sign` needs every build leg, so if one platform fails, nothing is signed or published.
   Fix the leg and re-run the failed jobs. The source archives come from GitHub's own release
   page.

6. Update [Release status](../README.md#release-status) and
   [mainnet-readiness.md](mainnet-readiness.md) in the same PR as the release notes.

`RELEASE-INFO.json` looks like this. Nothing in it is typed into the workflow:

```json
{
  "schema": "misaka/release-info/v1",
  "release_tag": "v0.9.0-rc.1",
  "git_commit": "<sha>",
  "rust_toolchain": "rustc 1.93.0",
  "network": "testnet-12",
  "consensus_frozen": false,
  "consensus_params_fingerprint": "<printed by every platform's kaspad>",
  "consensus_schedule_id": "<printed>",
  "fence_schedule": ["1000"],
  "genesis_declared": "<from release.json>",
  "platforms": ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "x86_64-unknown-linux-musl"],
  "identity_verified_on_every_platform": true
}
```

## 3. Verifying a release (users)

```bash
TAG=v0.9.0-rc.1
gh release download "$TAG" --repo MISAKA-BTC/misakas

# 0. what the release is: network, fingerprint, commit
cat RELEASE-INFO.json

# 1. the checksum file was signed by this repository's release workflow, at a tag
cosign verify-blob SHA256SUMS \
  --bundle SHA256SUMS.sigstore.json \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github\.com/MISAKA-BTC/misakas/\.github/workflows/deploy\.yaml@refs/tags/'

# 2. the files you downloaded are the files it lists
sha256sum --check --ignore-missing SHA256SUMS

# 3. (optional) GitHub's provenance attestation for one file
gh attestation verify rusty-kaspa-$TAG-linux-amd64.zip --repo MISAKA-BTC/misakas

# 4. the tag itself is signed
git verify-tag "$TAG"
```

Then start the node and compare its fingerprint and fence schedule with the release notes. A
binary whose signature checks out can still be on the wrong network. Those two lines are the check
that it is on the right one.

## 4. What is not covered yet

- **Reproducible builds are not checked by CI.** The release is signed as built by the workflow.
  Nothing yet rebuilds it independently and compares bytes. testnet-12's local release build did
  match across two checkouts ([t12 launch note](t12-launch-2026-09-25.md)).
- **The ARM64 Linux leg has not run yet.** It is a native glibc build on `ubuntu-24.04-arm`. Its
  first test is the first dry run, not a release.
- **The SBOM describes the source tree** (the Cargo dependency graph), not a scan of each binary.
