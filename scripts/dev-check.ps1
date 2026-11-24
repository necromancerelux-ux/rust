$ErrorActionPreference = 'Stop'

Write-Host '== cargo fmt --check =='
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw 'formatting failed' }

Write-Host '== cargo clippy =='
cargo clippy --locked --no-default-features --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'clippy failed' }

Write-Host '== cargo check =='
cargo check --locked --no-default-features
if ($LASTEXITCODE -ne 0) { throw 'check failed' }

Write-Host '== cargo test =='
cargo test --locked --no-default-features
if ($LASTEXITCODE -ne 0) { throw 'tests failed' }

Write-Host 'ALL CHECKS PASSED'
