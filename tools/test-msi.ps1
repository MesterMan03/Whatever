param([Parameter(Mandatory=$true)][string]$Package, [string]$UpgradePackage)
$ErrorActionPreference = 'Stop'
$packagePath = (Resolve-Path -LiteralPath $Package).Path
if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) { throw 'Run installer mutation tests on a disposable CI runner' }
$destination = Join-Path $env:RUNNER_TEMP 'Whatever MSI Test'
$expectedBin = Join-Path $destination 'bin'
$before = [Environment]::GetEnvironmentVariable('Path','Machine')
function Invoke-Msi([string[]]$Arguments) {
    $process = Start-Process -FilePath msiexec.exe -ArgumentList $Arguments -PassThru -Wait -WindowStyle Hidden
    if ($process.ExitCode -notin @(0,3010)) { throw "Windows Installer failed with $($process.ExitCode)" }
}
$installed = $false
$activePackage = $packagePath
try {
    Invoke-Msi @('/i', ('"' + $packagePath + '"'), '/qn', '/norestart', ('INSTALLFOLDER="' + $destination + '"'))
    $installed = $true
    $registered = [Environment]::GetEnvironmentVariable('Path','Machine') -split ';' | ForEach-Object { $_.TrimEnd('\') }
    if ($registered -notcontains $expectedBin.TrimEnd('\')) { throw 'Editor bin directory was not appended to machine PATH' }
    foreach ($part in ($before -split ';' | Where-Object { $_ })) {
        if ($registered -notcontains $part.TrimEnd('\')) { throw 'Installation changed an existing PATH entry' }
    }
    & (Join-Path $expectedBin 'wmod.exe') --version
    if ($LASTEXITCODE -ne 0) { throw 'Installed CLI failed' }
    # Exercise repair without duplicating the registered PATH segment.
    Invoke-Msi @('/i', ('"' + $packagePath + '"'), '/qn', '/norestart', 'REINSTALL=ALL', 'REINSTALLMODE=vomus')
    if ($UpgradePackage) {
        $upgradePath = (Resolve-Path -LiteralPath $UpgradePackage).Path
        Invoke-Msi @('/i', ('"' + $upgradePath + '"'), '/qn', '/norestart', ('INSTALLFOLDER="' + $destination + '"'))
        $activePackage = $upgradePath
    }
    $registered = @([Environment]::GetEnvironmentVariable('Path','Machine') -split ';' | Where-Object { $_.TrimEnd('\') -eq $expectedBin.TrimEnd('\') })
    if ($registered.Count -ne 1) { throw 'Repair or upgrade duplicated the PATH entry' }
} finally {
    if ($installed) { Invoke-Msi @('/x', ('"' + $activePackage + '"'), '/qn', '/norestart') }
}
$after = [Environment]::GetEnvironmentVariable('Path','Machine') -split ';' | ForEach-Object { $_.TrimEnd('\') }
if ($after -contains $expectedBin.TrimEnd('\')) { throw 'Uninstall left the editor directory on PATH' }
