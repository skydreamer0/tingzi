#Requires -RunAsAdministrator
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess) { throw 'Use 64-bit PowerShell.' }
$target = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'TingziStandaloneRC'
foreach ($bits in @(32,64)) {
    & (Join-Path $target "TingziStandaloneSetup$bits.exe") remove
    if ($LASTEXITCODE -ne 0) { throw "Standalone removal failed $bits" }
}
Write-Output 'Standalone RC unregistered. Installation files and isolated learning are retained.'
