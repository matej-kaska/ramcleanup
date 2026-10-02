param([string]$VtdRoot = "$env:USERPROFILE\Desktop\stt", [string]$NsisRoot, [switch]$Installer, [switch]$Check)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargo = Join-Path $VtdRoot '.tools\cargo'
    if (-not (Test-Path "$cargo\bin\cargo.exe")) { throw 'Rust is required. Install Rust or pass -VtdRoot with an existing VTD toolchain.' }
    $env:CARGO_HOME = $cargo
    $env:RUSTUP_HOME = Join-Path $VtdRoot '.tools\rustup'
    $env:PATH = "$cargo\bin;$env:PATH"
}
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw 'Visual Studio C++ Build Tools and Windows SDK are required.' }
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'C++ x64 build tools were not found.' }
$env:PATH = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer;$env:PATH"
$devcmd = '"' + $vs + '\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul && set'
cmd /c $devcmd | ForEach-Object { if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') } }
$env:CARGO_TARGET_DIR = Join-Path $root 'target'
$env:RUSTFLAGS = '-C target-feature=+crt-static'
if ($Check) {
    cargo fmt -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting failed.' }
    cargo clippy --locked --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
}
cargo build --release --locked --target x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw 'Rust build failed.' }
$output = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $output | Out-Null
Copy-Item -LiteralPath "$root\target\x86_64-pc-windows-msvc\release\ramcleanup.exe" -Destination $output
Copy-Item -LiteralPath "$root\target\x86_64-pc-windows-msvc\release\ramcleanup-helper.exe" -Destination $output
if ($Installer) {
    $compiler = if ($NsisRoot) { Join-Path $NsisRoot 'makensis.exe' } else { Join-Path $VtdRoot '.tools\installer\nsis-3.13\makensis.exe' }
    if (-not (Test-Path $compiler)) {
        $command = Get-Command makensis.exe -ErrorAction SilentlyContinue
        $installed = Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe'
        if ($command) { $compiler = $command.Source }
        elseif (Test-Path $installed) { $compiler = $installed }
        else { throw 'NSIS is required. Install it or pass -NsisRoot.' }
    }
    # This tiny native launcher is bundled only inside setup and exits after
    # asking Explorer to start the unprivileged tray. No extra installed runtime.
    $launcher = Join-Path $env:CARGO_TARGET_DIR 'installer-launch.exe'
    $launcherObject = Join-Path $env:CARGO_TARGET_DIR 'installer-launch.obj'
    & cl.exe /nologo /Os /GS- /GR- /Oi- /Zl /W4 /WX "/Fo$launcherObject" "$root\installer\launch.cpp" /link /NODEFAULTLIB /ENTRY:mainCRTStartup /SUBSYSTEM:WINDOWS /MACHINE:X64 /INCREMENTAL:NO /OPT:REF /OPT:ICF "/OUT:$launcher" kernel32.lib shell32.lib ole32.lib oleaut32.lib uuid.lib
    if ($LASTEXITCODE -ne 0) { throw 'Installer launcher build failed.' }
    $manifest = Get-Content -LiteralPath "$root\Cargo.toml" -Raw
    if ($manifest -notmatch '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"') { throw 'Missing package version.' }
    & $compiler /V2 "/DAPP_VERSION=$($matches[1])" "$root\installer\ramcleanup.nsi"
    if ($LASTEXITCODE -ne 0) { throw 'Installer build failed.' }
}
Get-ChildItem -LiteralPath $output -File | Select-Object Name,Length
