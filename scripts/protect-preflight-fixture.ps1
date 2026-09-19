param(
    [Parameter(Mandatory = $true)][string]$Path,
    [switch]$ForeignRead
)
$ErrorActionPreference = 'Stop'
# Only a new, empty, direct child of this repository's probe directory can be
# changed. This helper never edits a selected user home or traverses children.
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../.local/probes')).Path
$fixture = Get-Item -LiteralPath $Path -Force
if (-not $fixture.PSIsContainer -or
    $fixture.Parent.FullName -ne $root -or
    $fixture.Name -notmatch '^preflight-[a-zA-Z0-9]+$' -or
    ($fixture.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
    @(Get-ChildItem -LiteralPath $fixture.FullName -Force).Count -ne 0) {
    throw 'E_PREFLIGHT_FIXTURE_SCOPE'
}
$user = [Security.Principal.WindowsIdentity]::GetCurrent().User
$system = [Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$acl = [Security.AccessControl.DirectorySecurity]::new()
$acl.SetOwner($user)
$acl.SetAccessRuleProtection($true, $false)
foreach ($principal in @($user, $system)) {
    $rule = [Security.AccessControl.FileSystemAccessRule]::new(
        $principal, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
    $acl.AddAccessRule($rule)
}
if ($ForeignRead) {
    $everyone = [Security.Principal.SecurityIdentifier]::new('S-1-1-0')
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
        $everyone, 'ReadAndExecute', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
}
[IO.Directory]::SetAccessControl($fixture.FullName, $acl)
