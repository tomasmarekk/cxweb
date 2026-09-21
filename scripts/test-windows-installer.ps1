[CmdletBinding()]
param(
    [string]$MakeNsis = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'tauri/NSIS/makensis.exe'),
    [string]$InstallerTemplate = (Join-Path $PSScriptRoot '../packaging/windows/installer.nsi')
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
    let exit_file = if std::env::args().nth(1).as_deref() == Some("clear-local-session") && dir.join("clear-exit.txt").exists() { "clear-exit.txt" } else { "exit.txt" };
    let code = std::fs::read_to_string(dir.join(exit_file)).unwrap().trim().parse::<i32>().unwrap();
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
    @{ Name='remove-clear-session'; Hook='PREUNINSTALL'; Exit=0; Update=0; Clear=$true; ClearExit=0; Success=$true; Called=$true; Args='clear-local-session' },
    @{ Name='remove-clear-refused'; Hook='PREUNINSTALL'; Exit=0; Update=0; Clear=$true; ClearExit=1; Success=$false; Called=$true; Args='clear-local-session' },
    @{ Name='update-does-not-clear'; Hook='PREUNINSTALL'; Exit=1; Update=1; Clear=$true; Success=$true; Called=$false },
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
    if ($case.ContainsKey('ClearExit')) { $case.ClearExit | Set-Content -LiteralPath (Join-Path $caseDir 'clear-exit.txt') }
    $fixtureExe = Join-Path $caseDir 'fixture.exe'
    $source = $template.Replace('@HOOKS@', (Join-Path $repoRoot 'packaging/windows/hooks.nsh')).Replace('@OUT@', $fixtureExe).Replace('@DIR@', $caseDir).Replace('@UPDATE@', [string]$case.Update).Replace('@HOOK@', $case.Hook)
    $source | Set-Content -LiteralPath (Join-Path $caseDir 'fixture.nsi')
    & $MakeNsis /V1 (Join-Path $caseDir 'fixture.nsi')
    if ($LASTEXITCODE -ne 0) { throw "NSIS compilation failed: $($case.Name)" }
    $startOptions = @{ FilePath=$fixtureExe; WindowStyle='Hidden'; PassThru=$true }
    if ($case['Clear']) { $startOptions.ArgumentList = '/CLEARSESSION' }
    $process = Start-Process @startOptions
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
# Exercise the actual template's interactive replacement decision, not just
# PREUNINSTALL with an artificially supplied updater flag. The original bug
# entered the old uninstaller from this function during an ordinary upgrade.
$templateText = Get-Content -LiteralPath $InstallerTemplate -Raw
$leaveFunction = [regex]::Match($templateText, '(?s)Function PageLeaveReinstall\r?\n.*?FunctionEnd').Value
if (-not $leaveFunction) { throw 'Interactive replacement function missing.' }
# Fail rather than display a blocking dialog if the old uninstall path is hit.
$leaveFunction = $leaveFunction.Replace('MessageBox MB_ICONEXCLAMATION "$(unableToUninstall)"', 'SetErrorLevel 42')
$replacementTemplate = @'
Unicode true
RequestExecutionLevel user
SilentInstall silent
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!define MANUPRODUCTKEY "Software\cxweb-isolated-installer-fixture"
!define UNINSTKEY "Software\cxweb-isolated-installer-fixture-uninstall"
!define MAINBINARYNAME "fixture-never-installed"
!macro FixtureGetState CONTROL OUTPUT
  StrCpy ${OUTPUT} @SELECTION@
!macroend
!define NSD_GetState "!insertmacro FixtureGetState"
Name "cxweb replacement decision test"
OutFile "@OUT@"
Var UpdateMode
Var PassiveMode
Var WixMode
@FUNCTION@
Section
  StrCpy $UpdateMode 0
  StrCpy $WixMode 0
  StrCpy $R0 @VERSION@
  Call PageLeaveReinstall
  FileOpen $2 "@RECEIPT@" w
  FileWrite $2 "replacement-preserved"
  FileClose $2
SectionEnd
'@
foreach ($decision in @(
    @{ Name='ordinary-upgrade'; Version=1; Selection=1; Success=$true },
    @{ Name='ordinary-downgrade'; Version=-1; Selection=1; Success=$true },
    @{ Name='same-version-reinstall'; Version=0; Selection=1; Success=$true },
    @{ Name='explicit-removal-still-uninstalls'; Version=0; Selection=0; Success=$false }
)) {
    $caseDir = Join-Path $fixtureRoot $decision.Name
    New-Item -ItemType Directory -Path $caseDir | Out-Null
    $fixtureExe = Join-Path $caseDir 'fixture.exe'
    $receipt = Join-Path $caseDir 'continued.txt'
    $source = $replacementTemplate.Replace('@FUNCTION@', $leaveFunction).Replace('@OUT@', $fixtureExe).Replace('@RECEIPT@', $receipt).Replace('@VERSION@', [string]$decision.Version).Replace('@SELECTION@', [string]$decision.Selection)
    $source | Set-Content -LiteralPath (Join-Path $caseDir 'fixture.nsi')
    & $MakeNsis /V1 (Join-Path $caseDir 'fixture.nsi')
    if ($LASTEXITCODE -ne 0) { throw "Replacement fixture compilation failed: $($decision.Name)" }
    $process = Start-Process -FilePath $fixtureExe -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(20000)) { throw "Replacement fixture timed out: $($decision.Name)" }
    if ((Test-Path -LiteralPath $receipt) -ne $decision.Success) { throw "Replacement decision failed: $($decision.Name)" }
    Write-Output "PASS $($decision.Name)"
}
Write-Output "Preserved fixture evidence: $fixtureRoot"
