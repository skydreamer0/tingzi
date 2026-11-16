#Requires -RunAsAdministrator
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess) { throw 'Use 64-bit PowerShell.' }
$target = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'TingziStandaloneRC'
$retry = Test-Path -LiteralPath $target
$cls = 'Software\Classes\CLSID\{39FB929D-D7BE-4CD2-8972-91DF9841C1BE}'
foreach ($hive in @([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryHive]::CurrentUser)) {
    foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64,[Microsoft.Win32.RegistryView]::Registry32)) {
        $root = [Microsoft.Win32.RegistryKey]::OpenBaseKey($hive,$view)
        try {
            $key = $root.OpenSubKey("$cls\InprocServer32")
            if ($key) {
                $registered = $key.GetValue('')
                $key.Dispose()
                $bits = if ($view -eq [Microsoft.Win32.RegistryView]::Registry64) { 64 } else { 32 }
                if ($hive -ne [Microsoft.Win32.RegistryHive]::LocalMachine -or $registered -ne (Join-Path $target "TingziStandaloneService$bits.dll")) { throw 'Standalone class registered at another location.' }
            }
        } finally { $root.Dispose() }
    }
}
$files = @('TingziStandaloneService64.dll','TingziStandaloneService32.dll','TingziStandaloneSetup64.exe','TingziStandaloneSetup32.exe','TingziStandaloneLearning.exe','Tingzi.cin','phrases.txt','McBopomofo-LICENSE.txt','README.txt','RemoveStandalone.ps1','data-provenance.json','release-manifest.json')
foreach ($name in $files) { if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot $name) -PathType Leaf)) { throw "Missing $name" } }
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'release-manifest.json') -Raw | ConvertFrom-Json
if (@($manifest.files).Count -ne $files.Count -or @($manifest.files.path | Sort-Object -Unique).Count -ne $files.Count) { throw 'Incomplete or duplicate manifest entries.' }
foreach ($name in ($files | Where-Object { $_ -ne 'release-manifest.json' }) + @('InstallStandalone.ps1')) {
    if ($name -notin $manifest.files.path) { throw "Manifest missing $name" }
}
if ($retry -and (Test-Path -LiteralPath (Join-Path $target 'release-manifest.json')) -and
    (Get-FileHash -LiteralPath (Join-Path $target 'release-manifest.json')).Hash -ne
    (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'release-manifest.json')).Hash) { throw 'Installed manifest differs.' }
foreach ($entry in $manifest.files) {
    if ($entry.path -notin ($files + @('InstallStandalone.ps1'))) { throw 'Unexpected manifest entry.' }
    if ((Get-FileHash -LiteralPath (Join-Path $PSScriptRoot $entry.path) -Algorithm SHA256).Hash -ne $entry.sha256) { throw "Hash mismatch $($entry.path)" }
    if ($retry -and $entry.path -ne 'InstallStandalone.ps1') {
        $installed = Join-Path $target $entry.path
        if ((Test-Path -LiteralPath $installed) -and (Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash -ne $entry.sha256) { throw 'Existing standalone files differ. Retain them and use a different RC directory/version.' }
    }
}
if (-not $retry) {
    New-Item -ItemType Directory -Path $target | Out-Null
}
foreach ($name in $files) { if (-not (Test-Path -LiteralPath (Join-Path $target $name))) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $target } }
try {
    foreach ($bits in @(64,32)) {
        & (Join-Path $target "TingziStandaloneSetup$bits.exe") install
        if ($LASTEXITCODE -ne 0) { throw "Registration failed $bits" }
    }
    foreach ($bits in @(64,32)) {
        $timer = [Diagnostics.Stopwatch]::StartNew()
        do {
            & (Join-Path $target "TingziStandaloneSetup$bits.exe") status
            $status = $LASTEXITCODE
            if ($status -eq 0) { break }
            if ($timer.ElapsedMilliseconds -ge 5000) { throw "Status failed $bits after bounded TSF visibility checks" }
            Start-Sleep -Milliseconds 250
        } while ($true)
    }
    $tip = '0404:{39FB929D-D7BE-4CD2-8972-91DF9841C1BE}{F6C789D2-A52D-4C60-BCF4-7911DBC7901C}'
    if ($tip -notin @(Get-WinUserLanguageList | ForEach-Object { $_.InputMethodTips })) { throw 'Standalone input method missing from current-user language list.' }
} catch {
    if (-not $retry) { foreach ($bits in @(32,64)) { & (Join-Path $target "TingziStandaloneSetup$bits.exe") remove } }
    throw
}
Write-Output 'Tingzi standalone RC registered. Select it manually with Win+Space. Learning starts off.'
