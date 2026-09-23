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
    @{ Name='install-idle'; Hook='PREINSTALL'; Exit=0; Update=0; Success=$true; Called=$false },
    @{ Name='install-unavailable-host'; Hook='PREINSTALL'; Exit=1; Update=0; Success=$true; Called=$false },
    @{ Name='payload-first-install'; Hook='POSTINSTALL'; Exit=0; Update=0; Success=$true; Called=$true; Args='stage-runtime-update' },
    @{ Name='payload-update'; Hook='POSTINSTALL'; Exit=3010; Update=1; Success=$true; Called=$true; Args='stage-runtime-update'; Restart=$true },
    @{ Name='payload-refused'; Hook='POSTINSTALL'; Exit=1; Update=1; Success=$false; Called=$true; Args='stage-runtime-update' },
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
  IfRebootFlag 0 +4
  FileOpen $2 "$INSTDIR\restart.txt" w
  FileWrite $2 "restart"
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
    if ($continued -ne $case.Success -or (($process.ExitCode -eq 0) -or ($case.Restart -and $process.ExitCode -eq 3010)) -ne $case.Success -or $called -ne $case.Called) {
        throw "Installer control-flow mismatch: $($case.Name)"
    }
    if ((Test-Path -LiteralPath (Join-Path $caseDir 'restart.txt')) -ne [bool]$case.Restart) { throw "Installer restart flag mismatch: $($case.Name)" }
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
# Use versioned harmless executables to exercise the actual desktop payload
# macro, including replacing a mapped old image. No real app is started.
$payloadTemplate = @'
Unicode true
RequestExecutionLevel user
SilentInstall silent
Name "cxweb payload fixture"
VIProductVersion "@VERSION@.0.0.0"
VIAddVersionKey "FileVersion" "@VERSION@.0.0.0"
OutFile "@OUT@"
Section
  Sleep 30000
SectionEnd
'@
foreach ($version in @(1, 2)) {
    $source = $payloadTemplate.Replace('@VERSION@', [string]$version).Replace('@OUT@', (Join-Path $fixtureRoot "payload-$version.exe"))
    $source | Set-Content -LiteralPath (Join-Path $fixtureRoot "payload-$version.nsi")
    & $MakeNsis /V1 (Join-Path $fixtureRoot "payload-$version.nsi")
    if ($LASTEXITCODE -ne 0) { throw 'Versioned payload fixture compilation failed.' }
}
$payloadInstaller = @'
Unicode true
RequestExecutionLevel user
SilentInstall silent
!define MAINBINARYNAME "desktop-fixture"
!define MAINBINARYSRCPATH "@PAYLOAD@"
!include "@MACRO@"
Name "cxweb desktop replacement fixture"
OutFile "@OUT@"
InstallDir "@DIR@"
Section
  !insertmacro CXWEB_INSTALL_DESKTOP
  FileOpen $0 "$INSTDIR\verified.txt" w
  FileWrite $0 "verified"
  FileClose $0
SectionEnd
'@
'fn main() { std::thread::sleep(std::time::Duration::from_secs(30)); }' | Set-Content -LiteralPath (Join-Path $fixtureRoot 'mapped.rs')
rustc --edition 2024 --crate-name mapped_fixture (Join-Path $fixtureRoot 'mapped.rs') -o (Join-Path $fixtureRoot 'mapped.exe')
if ($LASTEXITCODE -ne 0) { throw 'Mapped-image fixture compilation failed.' }
foreach ($name in @('fresh-desktop', 'old-desktop', 'mapped-desktop', 'locked-desktop', 'blocked-desktop')) {
    $caseDir = Join-Path $fixtureRoot $name
    New-Item -ItemType Directory -Path $caseDir | Out-Null
    $desktop = Join-Path $caseDir 'desktop-fixture.exe'
    if ($name -in @('old-desktop', 'locked-desktop')) { Copy-Item -LiteralPath (Join-Path $fixtureRoot 'payload-1.exe') -Destination $desktop }
    if ($name -eq 'mapped-desktop') { Copy-Item -LiteralPath (Join-Path $fixtureRoot 'mapped.exe') -Destination $desktop }
    if ($name -eq 'blocked-desktop') { New-Item -ItemType Directory -Path $desktop | Out-Null }
    $output = Join-Path $caseDir 'installer.exe'
    $source = $payloadInstaller.Replace('@PAYLOAD@', (Join-Path $fixtureRoot 'payload-2.exe')).Replace('@MACRO@', (Join-Path $repoRoot 'packaging/windows/desktop-payload.nsh')).Replace('@OUT@', $output).Replace('@DIR@', $caseDir)
    $source | Set-Content -LiteralPath (Join-Path $caseDir 'installer.nsi')
    & $MakeNsis /V1 (Join-Path $caseDir 'installer.nsi')
    if ($LASTEXITCODE -ne 0) { throw "Payload fixture compilation failed: $name" }
    $mapped = $null
    try {
        if ($name -in @('mapped-desktop', 'locked-desktop')) {
            $mapped = Start-Process -FilePath $desktop -WindowStyle Hidden -PassThru
            Start-Sleep -Milliseconds 300
            if ($mapped.HasExited) { throw 'Mapped-image fixture exited before replacement.' }
        }
        $process = Start-Process -FilePath $output -WindowStyle Hidden -PassThru
        if (-not $process.WaitForExit(15000)) { throw "Payload fixture timed out: $name" }
        $process.Refresh()
        $verified = Test-Path -LiteralPath (Join-Path $caseDir 'verified.txt')
        if ($name -in @('blocked-desktop', 'locked-desktop')) {
            if ($verified -or $process.ExitCode -eq 0) { throw 'Blocked desktop was falsely reported installed.' }
        } else {
            if (-not $verified -or (Get-FileHash -LiteralPath $desktop).Hash -ne (Get-FileHash -LiteralPath (Join-Path $fixtureRoot 'payload-2.exe')).Hash) { throw "Desktop payload differs: $name" }
            if ($mapped -and $mapped.HasExited) { throw 'Replacement terminated the old mapped image.' }
        }
        Write-Output "PASS $name"
    } finally {
        if ($mapped -and -not $mapped.HasExited) { $mapped | Stop-Process }
    }
}
Write-Output "Preserved fixture evidence: $fixtureRoot"
