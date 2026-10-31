param(
    [Parameter(Mandatory=$true)][string]$BunExecutable,
    [Parameter(Mandatory=$true)][string]$Checkout,
    [Parameter(Mandatory=$true)][string]$Tools,
    [int]$Jobs = 4,
    [switch]$DebugBuild
)
$ErrorActionPreference = 'Stop'
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if (!(Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio 2022 with C++ build tools is required.' }
$installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$installation) { throw 'Install the Visual Studio C++ toolchain and ATL component.' }
& (Join-Path $installation 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation
if (!$env:VSCMD_VER) { throw 'Could not initialize the Visual Studio development environment; use PowerShell 7.' }
# Optional workspace-local ATL files can supplement a toolchain without ATL.
$atl = Join-Path $Tools 'atl'
if (Test-Path -LiteralPath $atl) {
    $header = Get-ChildItem -LiteralPath $atl -Filter atlstr.h -Recurse -File | Select-Object -First 1
    $library = Get-ChildItem -LiteralPath $atl -Filter atls.lib -Recurse -File | Where-Object { $_.Directory.Name -eq 'x64' } | Select-Object -First 1
    if ($header) { $env:INCLUDE = $header.Directory.FullName + ';' + $env:INCLUDE }
    if ($library) { $env:LIB = $library.Directory.FullName + ';' + $env:LIB }
}
Set-Location -LiteralPath $Checkout
if ($DebugBuild) { & $BunExecutable bd "-j$Jobs" } else { & $BunExecutable run build:release "-j$Jobs" }
exit $LASTEXITCODE