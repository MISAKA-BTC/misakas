# `cargo fmt --all` REWRITES the tree instead of failing, and its exit code was thrown away;
# `cargo clippy` with no flags lints neither tests nor benches nor examples, which is what the
# CI Lints job checks. Both gates now come from one place -- the same script the CI job runs.
# (bash ships with Git for Windows, which every Windows contributor here already has.)
bash ./scripts/ci-gates.sh fmt clippy
if ($LASTEXITCODE -ne 0) {
  Write-Output "`n--> host lint gates failed ($LASTEXITCODE gate(s))`n"
  exit $LASTEXITCODE
}

$crates = @(
  "kaspa-wrpc-wasm",
  "kaspa-wallet-cli-wasm",
  "kaspa-wasm",
  "kaspa-cli",
  "kaspa-os",
  "kaspa-daemon"
)

$env:AR="llvm-ar"
foreach ($crate in $crates)
{
  Write-Output "`ncargo clippy -p $crate --target wasm32-unknown-unknown"
  cargo clippy -p $crate --target wasm32-unknown-unknown
  $status=$LASTEXITCODE
  if($status -ne 0) {
    Write-Output "`n--> wasm32 check of $crate failed`n"
    break
  }
}
$env:AR=""