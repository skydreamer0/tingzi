param([switch]$Remove)
$ErrorActionPreference = 'Stop'
$machine = [Microsoft.Win32.RegistryKey]::OpenBaseKey('LocalMachine', 'Registry64')
$tip = $machine.OpenSubKey('SOFTWARE\Classes\CLSID\{B6ECF4B2-9955-4C24-9BA8-3F7641F0792B}\InprocServer32')
try {
    $root = Join-Path ${env:ProgramW6432} 'Tingzi'
    if (!$Remove -and !$tip) { throw 'The input method is not registered.' }
    $directory = if ($tip) { Split-Path -Parent $tip.GetValue('') } else { $root }
    $float = 'TingziFloat\TingziFloat.exe'
    $programs = [Environment]::GetFolderPath('Programs')
    $menu = Join-Path $programs '聽字・注音與語音輸入法'
    $shell = New-Object -ComObject WScript.Shell
    # WScript.Shell opens shortcut paths through the ANSI code page, which cannot spell 「・」.
    # Stage each shortcut under an ASCII name in the Programs folder and move it with Unicode APIs.
    function Stage { Join-Path $programs ('tingzi-' + [guid]::NewGuid().ToString('N') + '.lnk') }
    function Save-Link($path, $target, $arguments) {
        $staged = Stage
        $link = $shell.CreateShortcut($staged)
        $link.TargetPath = $target
        $link.Arguments = $arguments
        $link.WorkingDirectory = Split-Path -Parent $target
        $link.Save()
        Move-Item -LiteralPath $staged -Destination $path -Force
    }
    function Read-Link($path) {
        $staged = Stage
        Copy-Item -LiteralPath $path -Destination $staged
        try { $link = $shell.CreateShortcut($staged); @($link.TargetPath, $link.Arguments) } finally { Remove-Item -LiteralPath $staged }
    }
    # Only shortcuts that still point into this product's install folder are removed.
    function Remove-Ours($name, $relative) {
        $path = Join-Path $menu ($name + '.lnk')
        if (!(Test-Path -LiteralPath $path -PathType Leaf)) { return }
        $link = Read-Link $path
        if ($link[0].StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase) -and $link[0].EndsWith('\' + $relative, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $path
        }
    }
    # Each entry: shortcut name, path relative to the installed version, arguments.
    $items = @(@('聽字・注音與語音輸入法', $float, '--settings'), @('聽字設定', $float, '--settings'), @('語音浮窗', $float, '--show'))
    # Versions before the typing pad was retired left this shortcut behind.
    $retired = @('注音打字板', 'TingziPad.exe')
    if (!$Remove) {
        foreach ($item in $items) {
            $required = Join-Path $directory $item[1]
            if (!(Test-Path -LiteralPath $required -PathType Leaf)) { throw "Missing component $required" }
        }
        [void][IO.Directory]::CreateDirectory($menu)
        foreach ($item in $items) {
            Save-Link (Join-Path $menu ($item[0] + '.lnk')) (Join-Path $directory $item[1]) $item[2]
        }
        Remove-Ours $retired[0] $retired[1]
    } else {
        foreach ($item in $items) { Remove-Ours $item[0] $item[1] }
        Remove-Ours $retired[0] $retired[1]
        if ((Test-Path -LiteralPath $menu -PathType Container) -and !(Get-ChildItem -LiteralPath $menu -Force | Select-Object -First 1)) {
            Remove-Item -LiteralPath $menu
        }
    }
} finally {
    if ($tip) { $tip.Dispose() }
    $machine.Dispose()
}
