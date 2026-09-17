param(
    [Parameter(Mandatory = $true)][string]$Browser
)
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$probeRoot = Join-Path $workspace ('.local/browser-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $probeRoot | Out-Null
$stdout = Join-Path $probeRoot 'stdout.json'
$stderr = Join-Path $probeRoot 'stderr.txt'
$profile = Join-Path $probeRoot 'profile'
$arguments = @('browser-probe', '--browser', ('"' + $Browser + '"'), '--profile', ('"' + $profile + '"'), '--hold-seconds', '45')
$probe = Start-Process -FilePath (Join-Path $workspace 'target/debug/cxweb.exe') -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
try {
    $observation = $null
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        if ((Test-Path $stdout) -and (Get-Item $stdout).Length -gt 0) {
            $observation = Get-Content -Raw $stdout | ConvertFrom-Json
            break
        }
        if ($probe.HasExited) { throw 'Browser probe exited before reporting a pipe connection.' }
        Start-Sleep -Milliseconds 100
    }
    if (-not $observation) { throw 'Timed out waiting for private browser pipe.' }
    $ownedPids = [System.Collections.Generic.HashSet[int]]::new()
    [void]$ownedPids.Add([int]$observation.pid)
    $allProcesses = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
    do {
        $changed = $false
        foreach ($process in $allProcesses) {
            if ($ownedPids.Contains([int]$process.ParentProcessId)) {
                if ($ownedPids.Add([int]$process.ProcessId)) { $changed = $true }
            }
        }
    } while ($changed)
    $listeners = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $ownedPids.Contains([int]$_.OwningProcess) })
    if ($listeners.Count -ne 0) { throw 'Managed browser unexpectedly owns a TCP listener.' }
    # Crash only our launcher: job-object cleanup must close its owned child tree.
    Stop-Process -Id $probe.Id -Force
    $probe.WaitForExit()
    $survivors = @()
    for ($attempt = 0; $attempt -lt 50; $attempt++) {
        $survivors = @(Get-Process -Id @($ownedPids) -ErrorAction SilentlyContinue)
        if ($survivors.Count -eq 0) { break }
        Start-Sleep -Milliseconds 100
    }
    if ($survivors.Count -ne 0) { throw 'Owned browser processes survived launcher termination.' }
    $evidence = [ordered]@{
        schema = 'cxweb.browser-probe.v1'
        timestamp = [DateTime]::UtcNow.ToString('o')
        browser = $observation.version
        protocol = $observation.protocol
        transport = $observation.transport
        tcpListeners = $listeners.Count
        ownedProcessCount = $ownedPids.Count
        crashCleanup = 'PASS'
        login = 'NOT RUN'
        profileIsolation = 'new dedicated directory; no personal profile accessed'
        result = 'PASS local transport and containment only'
    }
    $evidence | ConvertTo-Json | Set-Content (Join-Path $probeRoot 'evidence.json')
    $evidence | ConvertTo-Json
    Write-Output ('Evidence: ' + (Join-Path $probeRoot 'evidence.json'))
} finally {
    if (-not $probe.HasExited) { Stop-Process -Id $probe.Id -Force }
    $probe.Dispose()
}
