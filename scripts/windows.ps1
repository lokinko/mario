param([ValidateSet("dev", "build")][string]$Mode = "build")
$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $projectRoot
if (-not $IsWindows -and $env:OS -ne "Windows_NT") { throw "Run this script on Windows." }
$targetTriple = ((rustc -vV | Select-String '^host: ').Line -replace '^host: ', '').Trim()
if ($LASTEXITCODE -ne 0 -or $targetTriple -notmatch 'windows-msvc$') { throw "Install the Rust MSVC toolchain and Microsoft C++ Build Tools." }
if ($Mode -eq "build") {
  & npm.cmd run tauri --prefix client -- build --config src-tauri/tauri.windows.conf.json
} else {
  & npm.cmd run tauri --prefix client -- dev --config src-tauri/tauri.windows.conf.json
}
if ($LASTEXITCODE -ne 0) { throw "Windows client $Mode failed." }
if ($Mode -eq "build") {
  New-Item -ItemType Directory -Force outputs | Out-Null
  $installers = Get-ChildItem "client/src-tauri/target/release/bundle/nsis/*-setup.exe"
  if (-not $installers) { throw "The Windows installer was not generated." }
  $installers | Copy-Item -Destination outputs -Force
  Write-Host "Windows installers are ready in outputs/."
}
