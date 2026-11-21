# Installs, upgrades over, and removes the single installer on a disposable Windows machine, such as the
# GitHub runner. It really registers the input method, so do not run it on a computer you use.
param([Parameter(Mandatory)][string]$Installer, [switch]$LockProgramFiles)
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

Install 'install'
$entry = Get-ItemProperty $uninstallKey
Check ($entry.DisplayName -eq '聽字・注音與語音輸入法') 'the apps list shows the product name'
Check ($entry.DisplayVersion -match '^\d+\.\d+\.\d+$') "the apps list shows version $($entry.DisplayVersion)"
Check ($entry.Publisher -eq 'Tingzi contributors') 'the apps list shows the publisher'
Check (Test-Path $entry.DisplayIcon) 'the apps list icon exists'
$dll = (Get-ItemProperty $serviceKey -ErrorAction SilentlyContinue).'(default)'
Check ($dll) 'the text service is registered for the whole machine'
Check ($dll -like "$root\v$($entry.DisplayVersion)\TingziTextService64.dll") "the text service is registered from the version folder ($dll)"
Check (Test-Path $dll) 'the registered text service exists'
$profileKey = 'HKLM:\SOFTWARE\Microsoft\CTF\TIP\{B6ECF4B2-9955-4C24-9BA8-3F7641F0792B}\LanguageProfile\0x00000404\{CD4317C3-FFB3-4D28-BD1A-7BD8480EBB65}'
Check ((Get-ItemProperty $profileKey).IconFile -eq $dll) 'the Traditional Chinese input profile points to the installed service'
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
$wordListHash = (Get-FileHash (Join-Path (Split-Path $dll) 'Tingzi.cin')).Hash
Set-Content -LiteralPath (Join-Path $root 'phrases.txt') -Value 'legacy word list'
New-Item -ItemType Directory -Force (Join-Path (Split-Path $dll) 'empty-uninstall-test\nested') | Out-Null
$locked = $null
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class TingziUninstallLock {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll")]
    public static extern bool FreeLibrary(IntPtr module);
}
'@
if ($LockProgramFiles) {
    $locked = [TingziUninstallLock]::LoadLibraryEx($dll, [IntPtr]::Zero, 1)
    Check ($locked -ne [IntPtr]::Zero) 'the test holds the IME DLL mapped as an image'
}
try {
Start-Process -FilePath $entry.UninstallString.Trim('"') -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + (Join-Path $logs 'uninstall.log') + '"') -Wait
for ($i = 0; $i -lt 120 -and (Test-Path $uninstallKey); $i++) { Start-Sleep -Seconds 1 }
Check (!(Test-Path $uninstallKey)) 'the apps list entry is gone'
Check (!(Test-Path $serviceKey)) 'the text service is no longer registered'
$pending = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' -ErrorAction SilentlyContinue).PendingFileRenameOperations
$deletions = @()
for ($i = 0; $i -lt $pending.Count; $i += 2) {
    if ($pending[$i + 1] -eq '') { $deletions += $pending[$i] -replace '^(\*\d+)?\\\?\?\\', '' }
}
if ($LockProgramFiles) {
    Check (!(Test-Path $dll)) 'the locked DLL was renamed away from the install path'
    Check ($deletions -notcontains $dll) 'the original DLL path is not pending deletion'
    $renamed = @(Get-ChildItem (Split-Path $dll) -Filter 'TingziTextService64.dll.tingzi-delete-*')
    Check ($renamed.Count -eq 1) 'the locked DLL has a unique pending-removal name'
    Check ($deletions -contains $renamed[0].FullName) 'only the renamed DLL path is scheduled for deletion'
}
$remaining = @(Get-ChildItem $root -Recurse -Force -ErrorAction SilentlyContinue)
if (Test-Path $root) { $remaining += Get-Item $root }
foreach ($item in $remaining) {
    Check ($deletions -contains $item.FullName) "remaining path is pending deletion: $($item.FullName)"
}
Check ($LockProgramFiles -or !(Test-Path $root)) 'the whole program directory is gone when no files are locked'
Check (Test-Path (Join-Path $data 'installer-test.txt')) 'personal data is kept'
Check ((Get-FileHash (Join-Path $data 'kept-after-uninstall\Tingzi.cin')).Hash -eq $wordListHash) 'the local word list is preserved outside Program Files'
Check ((Get-Content (Join-Path $data 'kept-after-uninstall\legacy-root\phrases.txt')) -eq 'legacy word list') 'legacy root word lists are preserved separately'
if ($LockProgramFiles) {
    Install 'reinstall-before-reboot'
    Check (Test-Path $dll) 'reinstall before reboot creates a fresh DLL at the original path'
    Check ((Get-FileHash $dll).Hash -eq (Get-FileHash $renamed[0].FullName).Hash) 'reinstalled DLL matches the locked old DLL'
    $removeDirectoryFailed = $false
    try { [IO.Directory]::Delete((Split-Path $dll)) } catch [IO.IOException] { $removeDirectoryFailed = $true }
    Check $removeDirectoryFailed 'non-recursive directory deletion refuses a non-empty reinstalled version'
    Check (Test-Path $dll) 'non-recursive directory deletion leaves reinstalled contents intact'
}
} finally {
    if ($locked -and $locked -ne [IntPtr]::Zero) { [TingziUninstallLock]::FreeLibrary($locked) | Out-Null }
}
Write-Host 'Installer checks passed.'

function Uninstall($name) {
    $command = (Get-ItemProperty $uninstallKey).UninstallString.Trim('"')
    Start-Process -FilePath $command -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + (Join-Path $logs "$name.log") + '"') -Wait
}

if ($LockProgramFiles) { Uninstall 'reinstall-cleanup' }

Install 'mixed-folder-install'
Set-Content (Join-Path $root 'unrelated.txt') 'unrelated file'
New-Item -ItemType Directory (Join-Path $root 'v1.notes') | Out-Null
Set-Content (Join-Path $root 'v1.notes\sentinel.txt') 'unrelated folder'
Uninstall 'mixed-folder-uninstall'
Check (!(Test-Path $uninstallKey)) 'mixed-folder uninstall removes the apps entry'
Check (!(Test-Path (Join-Path $root "v$($entry.DisplayVersion)"))) 'mixed-folder uninstall removes the owned version folder'
Check ((Get-Content (Join-Path $root 'unrelated.txt')) -eq 'unrelated file') 'mixed-folder uninstall preserves an unrelated file'
Check ((Get-Content (Join-Path $root 'v1.notes\sentinel.txt')) -eq 'unrelated folder') 'mixed-folder uninstall preserves an unrelated subfolder'
Check (@(Get-ChildItem $root).Count -eq 2) 'mixed-folder uninstall leaves only unrelated items'
Remove-Item (Join-Path $root 'unrelated.txt')
Remove-Item (Join-Path $root 'v1.notes') -Recurse
[IO.Directory]::Delete($root)

Install 'junction-install'
$target = $root + '.junction-test-' + [Guid]::NewGuid().ToString('N')
Set-Content (Join-Path $root 'junction-sentinel.txt') 'junction target data'
[IO.Directory]::Move($root, $target)
try {
    New-Item -ItemType Junction -Path $root -Target $target | Out-Null
    $before = @(Get-ChildItem $target -Recurse -File | Get-FileHash | Sort-Object Path)
    Uninstall 'junction-uninstall'
    Check (Test-Path $uninstallKey) 'junction-root uninstall is refused before unregistering or deleting files'
    $after = @(Get-ChildItem $target -Recurse -File | Get-FileHash | Sort-Object Path)
    Check (!(Compare-Object $before $after -Property Path, Hash)) 'junction-root uninstall leaves every target file unchanged'
    Check ((Get-Content (Join-Path $target 'junction-sentinel.txt')) -eq 'junction target data') 'junction-root uninstall preserves the target sentinel'
    Check ((Get-Content (Join-Path $logs 'junction-uninstall.log') -Raw) -match 'Leaving reparse-point install root') 'junction-root refusal is logged'
} finally {
    if (Test-Path $root) { [IO.Directory]::Delete($root) }
    [IO.Directory]::Move($target, $root)
}
Remove-Item (Join-Path $root 'junction-sentinel.txt')
Uninstall 'junction-restored-uninstall'
Check (!(Test-Path $root)) 'restoring the real install directory permits normal cleanup'
Write-Host 'Uninstall ownership checks passed.'

Install 'legacy-root-install'
$pad = Join-Path $root 'TingziPad.exe'
Copy-Item $dll $pad
$legacyFiles = 'TingziKeyboard.exe', 'TingziLearning.exe', 'TingziVersion.exe', 'InstallTingzi.cmd', 'tingzi.svg', 'pairs.txt', 'pairs-NOTICE.txt', 'THIRD-PARTY-NOTICES.txt'
foreach ($name in $legacyFiles) { Set-Content (Join-Path $root $name) 'legacy program file' }
$padImage = [TingziUninstallLock]::LoadLibraryEx($pad, [IntPtr]::Zero, 1)
Check ($padImage -ne [IntPtr]::Zero) 'the test holds the legacy root pad mapped as an image'
try {
    Uninstall 'legacy-root-uninstall'
    Check (!(Test-Path $pad)) 'the locked legacy pad is renamed away from its original path'
    $pendingPad = @(Get-ChildItem $root -Filter 'TingziPad.exe.tingzi-delete-*')
    Check ($pendingPad.Count -eq 1) 'the locked legacy pad has a unique pending-removal name'
    $pending = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager').PendingFileRenameOperations
    $deletions = @()
    for ($i = 0; $i -lt $pending.Count; $i += 2) {
        if ($pending[$i + 1] -eq '') { $deletions += $pending[$i] -replace '^(\*\d+)?\\\?\?\\', '' }
    }
    Check ($deletions -notcontains $pad) 'the original legacy pad path is not pending deletion'
    Check ($deletions -contains $pendingPad[0].FullName) 'the renamed legacy pad is scheduled for reboot deletion'
    Check ($deletions -contains $root) 'the owned-only legacy root is scheduled for reboot deletion'
    foreach ($name in $legacyFiles) { Check (!(Test-Path (Join-Path $root $name))) "legacy root program file is removed: $name" }
    Check (@(Get-ChildItem $root).Count -eq 1) 'only the reboot-pending legacy pad remains in the root'
} finally {
    [TingziUninstallLock]::FreeLibrary($padImage) | Out-Null
}
Remove-Item $pendingPad[0].FullName
[IO.Directory]::Delete($root)
Write-Host 'Legacy root cleanup checks passed.'
