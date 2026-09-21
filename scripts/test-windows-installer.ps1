[CmdletBinding()]
param(
    [string]$MakeNsis = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'tauri/NSIS/makensis.exe')
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$fixtureRoot = Join-Path $repoRoot ('.local/installer-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
# Compile a harmless stand-in. The production CLI and real connections are
# never called by these installer control-flow tests.
$stub = @'
fn main() {
    let dir = std::env::current_exe().unwrap().parent().unwrap().to_owned();
    std::fs::write(dir.join("called.txt"), std::env::args().skip(1).collect::<Vec<_>>().join(" ")).unwrap();
    let code = std::fs::read_to_string(dir.join("exit.txt")).unwrap().trim().parse::<i32>().unwrap();
    std::process::exit(code);
}
'@
$stub | Set-Content -LiteralPath (Join-Path $fixtureRoot 'stub.rs')
rustc --edition 2024 --crate-name installer_stub (Join-Path $fixtureRoot 'stub.rs') -o (Join-Path $fixtureRoot 'stub.exe')
if ($LASTEXITCODE -ne 0) { throw 'Stub compilation failed.' }
$cases = @(
    @{ Name='remove-success'; Hook='PREUNINSTALL'; Exit=0; Update=0; Success=$true; Called=$true; Args='prepare-uninstall' },
    @{ Name='remove-refused'; Hook='PREUNINSTALL'; Exit=1; Update=0; Success=$false; Called=$true; Args='prepare-uninstall' },
    @{ Name='remove-missing-helper'; Hook='PREUNINSTALL'; Exit=0; Update=0; Missing=$true; Success=$false; Called=$false },
    @{ Name='update-retains-connection'; Hook='PREUNINSTALL'; Exit=1; Update=1; Success=$true; Called=$false },
    @{ Name='install-idle'; Hook='PREINSTALL'; Exit=0; Update=0; Success=$true; Called=$true; Args='prepare-uninstall --check' },
    @{ Name='install-busy'; Hook='PREINSTALL'; Exit=1; Update=0; Success=$false; Called=$true; Args='prepare-uninstall --check' },
    @{ Name='first-install'; Hook='PREINSTALL'; Exit=0; Update=0; Missing=$true; Success=$true; Called=$false }
)
$template = @'
Unicode true
RequestExecutionLevel user
SilentInstall silent
!include "LogicLib.nsh"
!include "@HOOKS@"
Name "cxweb isolated installer test"
OutFile "@OUT@"
InstallDir "@DIR@"
Var UpdateMode
Section
  StrCpy $UpdateMode @UPDATE@
  !insertmacro NSIS_HOOK_@HOOK@
  FileOpen $2 "$INSTDIR\continued.txt" w
  FileWrite $2 "continued"
  FileClose $2
SectionEnd
'@
foreach ($case in $cases) {
    $caseDir = Join-Path $fixtureRoot $case.Name
    New-Item -ItemType Directory -Path $caseDir | Out-Null
    if (-not $case.Missing) { Copy-Item -LiteralPath (Join-Path $fixtureRoot 'stub.exe') -Destination (Join-Path $caseDir 'cxweb.exe') }
    $case.Exit | Set-Content -LiteralPath (Join-Path $caseDir 'exit.txt')
    $fixtureExe = Join-Path $caseDir 'fixture.exe'
    $source = $template.Replace('@HOOKS@', (Join-Path $repoRoot 'packaging/windows/hooks.nsh')).Replace('@OUT@', $fixtureExe).Replace('@DIR@', $caseDir).Replace('@UPDATE@', [string]$case.Update).Replace('@HOOK@', $case.Hook)
    $source | Set-Content -LiteralPath (Join-Path $caseDir 'fixture.nsi')
    & $MakeNsis /V1 (Join-Path $caseDir 'fixture.nsi')
    if ($LASTEXITCODE -ne 0) { throw "NSIS compilation failed: $($case.Name)" }
    $process = Start-Process -FilePath $fixtureExe -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(20000)) { throw "Fixture did not finish: $($case.Name)" }
    $process.Refresh()
    $continued = Test-Path -LiteralPath (Join-Path $caseDir 'continued.txt')
    $called = Test-Path -LiteralPath (Join-Path $caseDir 'called.txt')
    if ($continued -ne $case.Success -or ($process.ExitCode -eq 0) -ne $case.Success -or $called -ne $case.Called) {
        throw "Installer control-flow mismatch: $($case.Name)"
    }
    if ($called -and (Get-Content -LiteralPath (Join-Path $caseDir 'called.txt') -Raw) -cne $case.Args) {
        throw "Installer arguments mismatch: $($case.Name)"
    }
    Write-Output "PASS $($case.Name)"
}
Write-Output "Preserved fixture evidence: $fixtureRoot"
