# Builds the release binary and assembles the Windows portable zip for
# OpenDrop-Native.
#
# Usage: packaging\windows\build-portable.ps1
# Run this ON the Windows build machine (PowerShell), from anywhere.
#
# Output: OpenDrop-Native-<version>-windows-x64.zip at the repo root,
# where <version> is read from app\Cargo.toml's [package].version.
#
# Runtime DLL bundling policy (Windows portable zip), determined by running
# `dumpbin /dependents` on the real release exe and on each candidate DLL
# (see task-15-report.md for the full output): not guessed:
#   Bundled next to opendrop-app.exe (resolved from the build machine's
#   real install locations, not committed to the repo):
#     - Processing.NDI.Lib.x64.dll   (NDI SDK; grafton-ndi links statically
#       against the SDK's import lib, but the runtime DLL is still needed
#       at load time; confirmed a direct import of opendrop-app.exe)
#     - projectM-4.dll               (vcpkg x64-windows, dynamic triplet;
#       confirmed a direct import of opendrop-app.exe)
#     - projectM-4-playlist.dll      (vcpkg x64-windows, dynamic triplet;
#       NOT found in opendrop-app.exe's import table or delay-load table
#       by dumpbin: the app doesn't currently call any playlist API.
#       Bundled anyway alongside projectM-4.dll as a same-family/
#       forward-compat precaution, since the plan explicitly names
#       "projectM DLL(s)" and the file is already resolved for free.)
#     - glew32.dll                   (vcpkg x64-windows; confirmed a
#       dependency of projectM-4.dll itself, so needed for projectM to
#       load even though opendrop-app.exe doesn't import it directly)
#     - vcruntime140.dll, vcruntime140_1.dll, msvcp140.dll (the VC++
#       Redistributable, NOT part of the OS the way the Universal CRT is
#      : confirmed direct/transitive dependencies of opendrop-app.exe
#       and/or projectM-4*.dll. Sourced from VS Build Tools' own
#       Redist\MSVC\<ver>\x64\Microsoft.VC143.CRT\ folder, the
#       Microsoft-sanctioned copy meant for bundling with an app, not
#       from System32.)
#   Never bundled: everything else `dumpbin /dependents` reports
#   (KERNEL32.dll, USER32.dll, ADVAPI32.dll, ntdll.dll, WS2_32.dll,
#   OPENGL32.dll, GDI32.dll, and the api-ms-win-crt-*.dll Universal CRT
#   API sets): these ship with Windows itself (the Universal CRT has
#   been part of the OS since Windows 10), unlike the VC++ Redistributable
#   above (same "don't bundle the base system" principle as the AppImage
#   script's driver-library exclusions).

$ErrorActionPreference = "Stop"

$ScriptDir = $PSScriptRoot
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..")).Path

$Binary = Join-Path $RepoRoot "target\release\opendrop-app.exe"
$CargoToml = Join-Path $RepoRoot "app\Cargo.toml"
$License = Join-Path $RepoRoot "LICENSE"

# Real directory on this build machine holding the 9795-file preset pack,
# scp'd over from /srv/http/opendrop-presets on the Linux side (see
# task-15-report.md). Not part of the repo and not fetched by this script;
# there is no other source to read it from on this machine.
$PresetsSrc = "C:\opendrop-presets"

if (-not (Test-Path $PresetsSrc -PathType Container)) {
    Write-Error "Presets source directory not found: $PresetsSrc"
    exit 1
}

# --- 1. Build ---

Write-Output "Building release binary (cargo build --workspace --release) ..."
Push-Location $RepoRoot
try {
    cargo build --workspace --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "cargo build failed with exit code $LASTEXITCODE"
        exit 1
    }
} finally {
    Pop-Location
}

if (-not (Test-Path $Binary -PathType Leaf)) {
    Write-Error "Release binary not found at $Binary after build"
    exit 1
}

# --- 2. Read version from app\Cargo.toml ---

$inPackage = $false
$version = $null
foreach ($line in Get-Content $CargoToml) {
    if ($line -match '^\s*\[package\]\s*$') { $inPackage = $true; continue }
    if ($line -match '^\s*\[') { $inPackage = $false; continue }
    if ($inPackage -and ($line -match '^\s*version\s*=\s*"([^"]+)"\s*$')) {
        $version = $Matches[1]
        break
    }
}

if (-not $version) {
    Write-Error "Could not read [package].version from $CargoToml"
    exit 1
}

$folderName = "OpenDrop-Native-$version-windows-x64"
$OutDir = Join-Path $RepoRoot $folderName
$ZipPath = Join-Path $RepoRoot "$folderName.zip"

# --- 3. Locate runtime DLLs on this build machine ---

# NDI SDK: known path from Step 13 (task-13-report.md), verified fresh here
# rather than trusted blindly, since SDK installer versions can lay out
# subfolders differently. Falls back to a recursive search under
# "C:\Program Files\NDI" if the known path has moved.
$ndiKnownPath = "C:\Program Files\NDI\NDI 6 SDK\Bin\x64\Processing.NDI.Lib.x64.dll"
if (Test-Path $ndiKnownPath -PathType Leaf) {
    $ndiDll = $ndiKnownPath
} else {
    Write-Output "NDI DLL not at the known path, searching under C:\Program Files\NDI ..."
    $found = Get-ChildItem -Path "C:\Program Files\NDI" -Recurse -Filter "Processing.NDI.Lib.x64.dll" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $found) {
        Write-Error "Processing.NDI.Lib.x64.dll not found anywhere under C:\Program Files\NDI (is the NDI SDK installed?)"
        exit 1
    }
    $ndiDll = $found.FullName
}
Write-Output "NDI DLL: $ndiDll"

# vcpkg-installed DLLs (dynamic x64-windows triplet). VCPKG_ROOT is a
# machine-wide env var set on this build machine (Step 12); fall back to
# the well-known default install path if unset.
$vcpkgRoot = $env:VCPKG_ROOT
if (-not $vcpkgRoot) { $vcpkgRoot = "C:\vcpkg" }
$vcpkgBin = Join-Path $vcpkgRoot "installed\x64-windows\bin"

$vcpkgDlls = @(
    "projectM-4.dll",
    "projectM-4-playlist.dll",
    "glew32.dll"
)

$resolvedVcpkgDlls = @()
foreach ($name in $vcpkgDlls) {
    $path = Join-Path $vcpkgBin $name
    if (-not (Test-Path $path -PathType Leaf)) {
        Write-Error "Expected vcpkg DLL not found: $path (was 'vcpkg install projectm:x64-windows' run with the dynamic triplet?)"
        exit 1
    }
    $resolvedVcpkgDlls += $path
}
Write-Output "vcpkg DLLs: $($resolvedVcpkgDlls -join ', ')"

# VC++ Redistributable DLLs (not part of the OS, unlike the Universal CRT).
# Sourced from VS Build Tools' own Redist folder, the Microsoft-sanctioned
# copy meant for bundling with an app. The exact path has a toolset-version
# component that changes across VS updates, so search for it rather than
# hardcoding it (same reasoning as the dumpbin search below).
$vcRedistDlls = @("vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll")
$vsRedistRoots = @(
    "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Redist\MSVC",
    "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Redist\MSVC"
) | Where-Object { Test-Path $_ }

$resolvedVcRedistDlls = @()
foreach ($name in $vcRedistDlls) {
    # Exclude the "onecore" variant: it targets the OneCore kernel API
    # surface (IoT Core / Xbox), not a standard desktop Win32 app like
    # opendrop-app.exe. Match the plain "<ver>\x64\Microsoft.VCnnn.CRT\"
    # path only.
    $found = Get-ChildItem -Path $vsRedistRoots -Recurse -Filter $name -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "\\[\d.]+\\x64\\Microsoft\.VC\d+\.CRT\\" } |
        Select-Object -First 1
    if (-not $found) {
        Write-Error "VC++ redistributable DLL not found under VS Build Tools' Redist folder: $name"
        exit 1
    }
    $resolvedVcRedistDlls += $found.FullName
}
Write-Output "VC++ redist DLLs: $($resolvedVcRedistDlls -join ', ')"

if (-not (Test-Path $License -PathType Leaf)) {
    Write-Error "LICENSE not found at $License"
    exit 1
}

# --- 4. Assemble the flat portable folder ---

Write-Output "Assembling $OutDir ..."
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Path $OutDir | Out-Null

Copy-Item $Binary (Join-Path $OutDir "opendrop-app.exe")
Copy-Item $ndiDll $OutDir
foreach ($dll in $resolvedVcpkgDlls) { Copy-Item $dll $OutDir }
foreach ($dll in $resolvedVcRedistDlls) { Copy-Item $dll $OutDir }
Copy-Item $License (Join-Path $OutDir "LICENSE")

# Sibling-of-exe "presets" folder, flat layout: matches preset_dir_from's
# Windows branch (app/src/main.rs), verified against real MSVC in Step 14.
Write-Output "Copying presets from $PresetsSrc ..."
Copy-Item $PresetsSrc (Join-Path $OutDir "presets") -Recurse

# --- 5. Zip it ---

Write-Output "Compressing to $ZipPath ..."
if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
Compress-Archive -Path $OutDir -DestinationPath $ZipPath

Write-Output "Built $ZipPath"
