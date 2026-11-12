# Downloads the latest network-owner data and builds owners.bin, the file
# Portlight uses to show who owns each server's network and the country it is
# registered in. Lookups happen on the PC; nothing is sent anywhere.
#
# Data: ip2asn-combined.tsv.gz from https://iptoasn.com, public domain under
# the Open Data Commons PDDL v1.0 (https://opendatacommons.org/licenses/pddl/1-0/).
#
# Output: owners.bin in target\debug\ and target\release\, because Portlight
# looks for it next to its exe. The installer ships target\release\owners.bin.
# Usage: powershell -File scripts\update-owners.ps1

$ErrorActionPreference = "Stop"
# The progress bar makes Invoke-WebRequest many times slower in Windows PowerShell.
$ProgressPreference = "SilentlyContinue"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$source = "https://iptoasn.com/data/ip2asn-combined.tsv.gz"
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("portlight-owners-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null

try {
    $download = Join-Path $work "ip2asn-combined.tsv.gz"
    $built = Join-Path $work "owners.bin"

    Write-Host "Downloading $source"
    Invoke-WebRequest -Uri $source -OutFile $download -UseBasicParsing
    if ((Get-Item $download).Length -lt 1MB) { throw "The download is too small; iptoasn.com may be down." }

    cargo run --release -p portlight-geo --bin portlight-owners -- build $download $built
    if ($LASTEXITCODE -ne 0) { throw "Building owners.bin failed" }

    # A quick check that the file answers: 1.1.1.1 belongs to Cloudflare (AS13335).
    $check = cargo run --quiet --release -p portlight-geo --bin portlight-owners -- lookup $built 1.1.1.1
    if ($LASTEXITCODE -ne 0 -or "$check" -notmatch "AS13335") { throw "owners.bin failed its check: $check" }

    foreach ($buildProfile in "debug", "release") {
        $dir = Join-Path $targetDir $buildProfile
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        Copy-Item $built (Join-Path $dir "owners.bin") -Force
        Write-Host "Wrote $(Join-Path $dir 'owners.bin')"
    }
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
