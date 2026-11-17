# Windows PowerShell 5.1 / Windows Forms
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$preview = [System.Windows.Forms.ApplicationContext]::new()
$timer = [System.Windows.Forms.Timer]::new()
$images = [System.Collections.Generic.List[System.Drawing.Icon]]::new()
$items = [System.Collections.Generic.List[System.Windows.Forms.NotifyIcon]]::new()
$variants = [ordered]@{
    'upright.ico' = 'Tingzi A | Upright 16 px'
    'sideways.ico' = 'Tingzi B | Sideways 16 px'
}

try {
    foreach ($filename in $variants.Keys) {
        $image = [System.Drawing.Icon]::new((Join-Path $PSScriptRoot $filename), 16, 16)
        $images.Add($image)
        $item = [System.Windows.Forms.NotifyIcon]::new()
        $items.Add($item)
        $item.Icon = $image
        $item.Text = $variants[$filename]
        $item.Add_DoubleClick({ $preview.ExitThread() })
        $item.Visible = $true
    }
    $timer.Interval = 600000
    $timer.Add_Tick({ $preview.ExitThread() })
    $timer.Start()
    [System.Windows.Forms.Application]::Run($preview)
}
finally {
    $timer.Dispose()
    foreach ($item in $items) {
        $item.Visible = $false
        $item.Dispose()
    }
    foreach ($image in $images) { $image.Dispose() }
    $preview.Dispose()
}
