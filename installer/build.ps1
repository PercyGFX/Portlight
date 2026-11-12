# Builds the release exe and the installer locally.
# Needs Node.js, Rust and Inno Setup 6:  winget install JRSoftware.InnoSetup
# Output: installer\Output\Portlight-Setup-<version>.exe

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# The version lives in one place: [workspace.package] in the root Cargo.toml.
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "Building Portlight $version"

npm ci
if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
cargo test --workspace --release
if ($LASTEXITCODE -ne 0) { throw "Tests failed" }
# Builds the web UI, then the exe with the UI inside it.
npm run tauri -- build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "Build failed" }
# The background service for detailed monitoring, and the AI assistant bridge.
cargo build --release -p portlight-collector -p portlight-mcp
if ($LASTEXITCODE -ne 0) { throw "Service build failed" }
# The offline list of who owns which network (downloads about 11 MB).
& "$root\scripts\update-owners.ps1"
if ($LASTEXITCODE -ne 0) { throw "Owner list build failed" }

$iscc = @(
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 not found. Install it with: winget install JRSoftware.InnoSetup" }

& $iscc "/DAppVersion=$version" "installer\portlight.iss"
if ($LASTEXITCODE -ne 0) { throw "Installer build failed" }
Write-Host "Done: installer\Output\Portlight-Setup-$version.exe"
