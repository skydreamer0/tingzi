# Installs, upgrades over, and removes the single installer on a disposable Windows machine, such as the
# GitHub runner. It really registers the input method, so do not run it on a computer you use.
param([Parameter(Mandatory)][string]$Installer)
$ErrorActionPreference = 'Stop'
$uninstallKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{D8B8670C-C282-48DE-8673-AFC4631934BA}_is1'
$serviceKey = 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\CLSID\{B6ECF4B2-9955-4C24-9BA8-3F7641F0792B}\InprocServer32'
$root = Join-Path ${env:ProgramW6432} 'Tingzi'
$logs = Join-Path ([IO.Path]::GetDirectoryName((Resolve-Path $Installer))) 'installer-logs'
New-Item -ItemType Directory -Force $logs | Out-Null

function Report {
    # What the installer and the registry say, so a failure on a machine nobody can log in to can be traced.
    Get-ChildItem $logs -Filter *.log | ForEach-Object { Write-Host "----- $($_.Name)"; Get-Content $_.FullName -Tail 60 }
    foreach ($view in '/reg:64', '/reg:32') { Write-Host "----- HKLM CLSID $view"; reg query 'HKLM\SOFTWARE\Classes\CLSID\{B6ECF4B2-9955-4C24-9BA8-3F7641F0792B}' /s $view 2>&1 | Write-Host }
    Write-Host '----- HKCU CLSID'; reg query 'HKCU\SOFTWARE\Classes\CLSID\{B6ECF4B2-9955-4C24-9BA8-3F7641F0792B}' /s 2>&1 | Write-Host
    Write-Host "----- $root"; Get-ChildItem $root -Recurse -ErrorAction SilentlyContinue | Select-Object -First 60 | ForEach-Object { Write-Host $_.FullName }
}

function Check($condition, $message) { if (!$condition) { Report; throw "FAILED: $message" } else { Write-Host "ok: $message" } }

function Run($name) {
    (Start-Process -FilePath $Installer -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + (Join-Path $logs "$name.log") + '"') -Wait -PassThru).ExitCode
}
function Install($name) {
    $code = Run $name
    Check ($code -eq 0) "$name exits with 0 (got $code)"
}

# GitHub's Windows runners run as a service, where the text services framework cannot read back a
# profile it has just registered, so InstallTingzi.cmd fails there with code 6 whatever installs it.
# On such a machine the installer must still stop cleanly: exit code 7, nothing registered, no entry in
# the apps list. The full install, upgrade and removal is then checked on a real Windows computer.
$code = Run 'install'
if ($code -eq 7 -and (Select-String -Path (Join-Path $logs 'install.log') -Pattern 'GetProfile: 0x80004005' -Quiet)) {
    Check (!(Test-Path $uninstallKey)) 'a failed registration leaves no apps list entry'
    Check (!(Test-Path $serviceKey)) 'a failed registration leaves no text service registered'
    Check (!(Test-Path "$root\unins000.exe")) 'a failed registration leaves no uninstaller'
    Write-Host '::warning::This machine cannot register the text service (TSF GetProfile fails), so only the installer''s refusal was checked. Install, upgrade and removal still need a check on a real Windows computer.'
    exit 0
}
Check ($code -eq 0) "install exits with 0 (got $code)"
$entry = Get-ItemProperty $uninstallKey
Check ($entry.DisplayName -eq '聽字・注音與語音輸入法') 'the apps list shows the product name'
Check ($entry.DisplayVersion -match '^\d+\.\d+\.\d+$') "the apps list shows version $($entry.DisplayVersion)"
Check ($entry.Publisher -eq 'Tingzi contributors') 'the apps list shows the publisher'
Check (Test-Path $entry.DisplayIcon) 'the apps list icon exists'
$dll = (Get-ItemProperty $serviceKey -ErrorAction SilentlyContinue).'(default)'
Check ($dll) 'the text service is registered for the whole machine'
Check ($dll -like "$root\v$($entry.DisplayVersion)\TingziTextService64.dll") "the text service is registered from the version folder ($dll)"
Check (Test-Path $dll) 'the registered text service exists'
Check (Test-Path "$root\v$($entry.DisplayVersion)\TingziFloat\TingziFloat.exe") 'the voice window is installed'
foreach ($name in 'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll', 'vcomp140.dll') {
    Check (Test-Path "$root\v$($entry.DisplayVersion)\TingziFloat\$name") "the Visual C++ runtime for local Whisper is installed ($name)"
}

# Personal data that must survive an upgrade and a removal that keeps data.
$data = Join-Path $env:LOCALAPPDATA 'Tingzi'
New-Item -ItemType Directory -Force $data | Out-Null
Set-Content -LiteralPath (Join-Path $data 'installer-test.txt') -Value 'keep me'

# Reinstalling the same version over a different build must replace the files, not stop with code 6.
$readme = "$root\v$($entry.DisplayVersion)\README.md"
$shipped = Get-FileHash $readme
Add-Content -LiteralPath $readme -Value 'an earlier build'
Install 'upgrade'
Check ((Get-FileHash $readme).Hash -eq $shipped.Hash) 'reinstalling the same version replaces a file from a different build'
Check ((Get-ItemProperty $serviceKey).'(default)' -eq $dll) 'running the installer again keeps one registration'
Check ((Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall' | Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -eq '聽字・注音與語音輸入法' }).Count -eq 1) 'the apps list has one entry'

# A silent removal keeps personal data. The uninstaller hands over to a copy of itself, so wait for the entry to go.
Start-Process -FilePath $entry.UninstallString.Trim('"') -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + (Join-Path $logs 'uninstall.log') + '"') -Wait
for ($i = 0; $i -lt 120 -and (Test-Path $uninstallKey); $i++) { Start-Sleep -Seconds 1 }
Check (!(Test-Path $uninstallKey)) 'the apps list entry is gone'
Check (!(Test-Path $serviceKey)) 'the text service is no longer registered'
Check (!(Test-Path $dll)) 'the program files are gone'
Check (Test-Path (Join-Path $data 'installer-test.txt')) 'personal data is kept'
Write-Host 'Installer checks passed.'
