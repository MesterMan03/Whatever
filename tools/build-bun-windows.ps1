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
# Bun searches known LLVM installs before PATH, and only version-checks some
# tools. Pin the entire portable toolchain so llvm-nm cannot come from the
# runner's older C:\Program Files\LLVM installation.
if (!$env:BUN_TOOLCHAIN_LLVM -and (Test-Path -LiteralPath $Tools)) {
    $llvm = @(Get-ChildItem -LiteralPath $Tools -Directory -Filter 'clang+llvm-*' | Where-Object {
        Test-Path -LiteralPath (Join-Path $_.FullName 'bin/clang-cl.exe')
    })
    if ($llvm.Count -gt 1) { throw 'Multiple portable LLVM toolchains found; set BUN_TOOLCHAIN_LLVM explicitly.' }
    if ($llvm.Count -eq 1) { $env:BUN_TOOLCHAIN_LLVM = $llvm[0].FullName }
}
if ($env:BUN_TOOLCHAIN_LLVM) {
    foreach ($tool in @('clang-cl.exe', 'lld-link.exe', 'llvm-lib.exe', 'llvm-nm.exe')) {
        $executable = Join-Path $env:BUN_TOOLCHAIN_LLVM "bin/$tool"
        if (!(Test-Path -LiteralPath $executable)) { throw "LLVM toolchain is missing $executable" }
        & $executable --version
        if ($LASTEXITCODE -ne 0) { throw "Could not verify $executable" }
    }
}
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
