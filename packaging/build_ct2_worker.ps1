# SPDX-License-Identifier: MIT
<#
.SYNOPSIS
  Builds liveaudio-asr-worker-cuda.exe (ct2rs / CTranslate2 + CUDA, static CRT) locally.

.PARAMETER CudaRoot
  CUDA toolkit root containing bin\nvcc.exe, include\ and lib\ (cuBLAS dev + cuDNN headers/libs
  must be reachable from it). Default: env CT2_CUDA_ROOT, then env CUDA_PATH.
.PARAMETER CudnnRoot
  Optional separate cuDNN root (include\ and lib\) when it is not inside CudaRoot.
.PARAMETER ArchList
  CUDA architectures. Default "7.5;8.6;8.9;12.0" (12.0 = Blackwell; needs a CUDA >= 12.8 toolkit).
.PARAMETER TargetDir
  SHORT cargo target dir (the CMake VS generator hits MAX_PATH otherwise). Default C:/t/ct2.
.PARAMETER VcVars
  Path to vcvars64.bat. Default: located with vswhere.
.PARAMETER Pin
  After a successful release build, stage the exe in dist-ct2/ and rewrite packaging/ct2-worker.pin.json
  (version, URL, sha256, size). Upload the staged exe to the GitHub release afterwards (docs/PACKAGING_AND_UPDATES.md).
.PARAMETER PinOnly
  Skip the build; stage -ExePath and write the pin file only.
.PARAMETER ExePath
  Worker exe used by -PinOnly. Default: <TargetDir>/release/liveaudio-asr-worker-cuda.exe.
.PARAMETER CmakeBin
  Directory holding cmake.exe if it is not on PATH (the Visual Studio bundled CMake is used if found).
#>
param(
    [string]$CudaRoot = $(if ($env:CT2_CUDA_ROOT) { $env:CT2_CUDA_ROOT } else { $env:CUDA_PATH }),
    [string]$CudnnRoot = $env:CT2_CUDNN_ROOT,
    [string]$ArchList = "7.5;8.6;8.9;12.0",
    [string]$TargetDir = "C:/t/ct2",
    [string]$VcVars = "",
    [string]$CmakeBin = "",
    [switch]$Test,
    [switch]$Pin,
    [switch]$PinOnly,
    [string]$ExePath = ""
)
$ErrorActionPreference = "Stop"

function Write-Pin([string]$Exe) {
    $root = Resolve-Path (Join-Path $PSScriptRoot "..")
    $version = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    $stage = Join-Path $root "dist-ct2"
    New-Item -ItemType Directory -Force $stage | Out-Null
    $staged = Join-Path $stage "liveaudio-asr-worker-cuda.exe"
    Copy-Item $Exe $staged -Force
    $pin = [ordered]@{
        version = $version
        url     = "https://github.com/plynte-labs/LiveAudio/releases/download/v$version/liveaudio-asr-worker-cuda.exe"
        sha256  = (Get-FileHash $staged -Algorithm SHA256).Hash.ToLower()
        size    = (Get-Item $staged).Length
    }
    $file = Join-Path $PSScriptRoot "ct2-worker.pin.json"
    [IO.File]::WriteAllText($file, (($pin | ConvertTo-Json) + "`n"))
    Write-Host "Staged $staged"
    Write-Host "Wrote $file"
    Get-Content $file
}

if ($PinOnly) {
    if (-not $ExePath) { $ExePath = Join-Path $TargetDir "release/liveaudio-asr-worker-cuda.exe" }
    if (-not (Test-Path $ExePath)) { throw "Worker exe not found: $ExePath" }
    Write-Pin (Resolve-Path $ExePath).Path
    return
}
if (-not $CudaRoot) { throw "Pass -CudaRoot (or set CT2_CUDA_ROOT / CUDA_PATH)." }
$CudaRoot = (Resolve-Path $CudaRoot).Path.Replace([string][char]92, '/')
$crate = Join-Path $PSScriptRoot "..\crates\liveaudio-asr-worker-ct2" | Resolve-Path

if (-not $VcVars) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $vs = & $vswhere -latest -products * -property installationPath
    if (-not $vs) { throw "Visual Studio with the C++ workload not found; pass -VcVars." }
    $VcVars = Join-Path $vs "VC\Auxiliary\Build\vcvars64.bat"
    if (-not $CmakeBin) {
        $bundled = Join-Path $vs "Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
        if (Test-Path $bundled) { $CmakeBin = $bundled }
    }
}

# Import the MSVC environment (nvcc needs cl.exe on PATH).
cmd /c "`"$VcVars`" >nul && set" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "env:$($Matches[1])" -Value $Matches[2] }
}

$bins = @("$CudaRoot/bin")
if ($CmakeBin) { $bins += $CmakeBin }
$env:PATH = ($bins -join ";") + ";" + $env:PATH
if ($CudnnRoot) {
    $CudnnRoot = (Resolve-Path $CudnnRoot).Path.Replace([string][char]92, '/')
    $env:INCLUDE = "$CudnnRoot/include;$env:INCLUDE"
    $env:LIB = "$CudnnRoot/lib;$CudnnRoot/lib/x64;$env:LIB"
    $env:PATH = "$CudnnRoot/bin;$env:PATH"
}

$env:RUSTFLAGS = "-C target-feature=+crt-static"
$env:CARGO_TARGET_DIR = $TargetDir
$env:CUDA_PATH = $CudaRoot                 # forward slashes: CMake rejects backslash escapes
$env:CUDA_TOOLKIT_ROOT_DIR = $CudaRoot
$env:CUDA_ARCH_LIST = $ArchList
$env:NVCC_APPEND_FLAGS = "-allow-unsupported-compiler"  # VS newer than the CUDA-supported range
Remove-Item env:CMAKE_PARALLEL -ErrorAction SilentlyContinue  # injects -j that MSBuild rejects

Push-Location $crate
try {
    if ($Test) { cargo test --release --lib } else { cargo build --release }
    if ($LASTEXITCODE -ne 0) { throw "cargo failed with exit code $LASTEXITCODE" }
    if (-not $Test) {
        $exe = Join-Path $TargetDir "release/liveaudio-asr-worker-cuda.exe"
        Write-Host ("Built {0} ({1:N1} MB)" -f $exe, ((Get-Item $exe).Length / 1MB))
        if ($Pin) { Write-Pin $exe }
    }
} finally { Pop-Location }
