[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    if ((cargo tauri --version) -ne 'tauri-cli 2.11.2') {
        throw 'Install the pinned packaging tool: cargo install tauri-cli --version 2.11.2 --locked'
    }
    cargo build --locked --release -p cxweb -p cxweb-daemon
    if ($LASTEXITCODE -ne 0) { throw 'Runtime build failed.' }
    Push-Location (Join-Path $repoRoot 'apps/desktop/src-tauri')
    try {
        cargo tauri build --ci --config ../../../packaging/windows/tauri.bundle.json -- --locked
        if ($LASTEXITCODE -ne 0) { throw 'Installer build failed.' }
    } finally { Pop-Location }
    $appConfig = Get-Content -LiteralPath (Join-Path $repoRoot 'apps/desktop/src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
    $installer = Join-Path $repoRoot "target/release/bundle/nsis/cxweb_$($appConfig.version)_x64-setup.exe"
    if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) { throw 'Installer was not produced.' }
    $digest = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    "$digest  $([IO.Path]::GetFileName($installer))" | Set-Content -LiteralPath "$installer.sha256" -Encoding ascii
    Get-Item -LiteralPath $installer | Select-Object FullName, Length
    Write-Output "Authenticode: $((Get-AuthenticodeSignature -LiteralPath $installer).Status.ToString())"
} finally { Pop-Location }
