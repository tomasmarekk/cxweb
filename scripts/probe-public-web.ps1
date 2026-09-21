param(
    [Parameter(Mandatory)][string]$Client,
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Report
)
$ErrorActionPreference = 'Stop'
$result = @{ schema = 'cxweb.public-web-process.v1'; observedAt = [DateTime]::UtcNow.ToString('o'); result = 'FAIL' }
try {
    $entry = & $Client mcp get cxweb_web --json | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $entry.transport.command -ne $Executable -or ($entry.transport.args -join '|') -ne 'web-tools') { throw 'E_MCP_CONFIG' }
    $requests = @(
        @{ jsonrpc='2.0'; id=1; method='initialize'; params=@{ protocolVersion='2024-11-05'; capabilities=@{}; clientInfo=@{ name='cxweb-public-probe'; version='1' } } },
        @{ jsonrpc='2.0'; method='notifications/initialized' },
        @{ jsonrpc='2.0'; id=2; method='tools/list' },
        @{ jsonrpc='2.0'; id=3; method='tools/call'; params=@{ name='search'; arguments=@{ query='Rust programming language official website'; limit=3 } } }
    ) | ForEach-Object { ConvertTo-Json $_ -Depth 8 -Compress }
    $responses = $requests | & $Executable web-tools | ForEach-Object { ConvertFrom-Json $_ }
    if ($LASTEXITCODE -ne 0) { throw 'E_MCP_PROCESS' }
    $toolReply = $responses | Where-Object { $_.id -eq 3 }
    if ($toolReply.result.isError -ne $false) { throw 'E_SEARCH_FAILED' }
    $search = $toolReply.result.content[0].text | ConvertFrom-Json
    if ($search.provider -ne 'Bing' -or $search.results.Count -ne 3) { throw 'E_SEARCH_RESULTS' }
    $result.result = 'PASS'
    $result.installedConfigResolved = $true
    $result.actualSearchResults = $search.results.Count
    $result.executableSha256 = (Get-FileHash -LiteralPath $Executable).Hash.ToLowerInvariant()
} catch {
    $result.error = if ($_.Exception.Message -match '^E_[A-Z_]+$') { $_.Exception.Message } else { 'E_PUBLIC_WEB_PROBE' }
} finally {
    $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Report -Encoding utf8NoBOM
}
if ($result.result -ne 'PASS') { exit 1 }
