# Measures the shipped runtime on Windows and fails when it exceeds a
# footprint target. `mise run rust:footprint` builds the runtime and runs it;
# wiki/developer/Architecture.md#footprint states the targets and what each
# phase measures.
#
# The runtime gets scratch LOCALAPPDATA and APPDATA folders holding one
# application, a window this script opens, and three automations that watch
# it. One automation fires once, which proves the runtime reads the window;
# its action sets the volume of a program that does not exist, so it changes
# nothing. The runtime runs idle while the window is closed, active while it
# is open and repainting every 16 ms the way a game does, and idle again once
# it closes.

param(
    [Parameter(Mandatory = $true)]
    [string]$Runtime
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# The targets. Memory is the private working set, the Memory column of Task
# Manager's Details tab; CPU is the share of one core over the sample.
$BinaryLimit = 2MB
$IdleMemoryLimit = 3MB
$IdleCpuLimit = 0.05
$ActiveMemoryLimit = 6MB
$ActiveCpuLimit = 2.0

# Sampling: how long each phase settles before its sample, and how long the
# sample runs.
$Settle = 5
$Sample = 30

$ProbeTitle = 'AppyTappy footprint probe'
$ProbeExecutable = 'appytappy-footprint-probe.exe'
$Failures = New-Object System.Collections.Generic.List[string]
$Rows = New-Object System.Collections.Generic.List[string]

function Add-Row([string]$Measure, [string]$Value, [string]$Limit, [bool]$Within) {
    $verdict = if ($Within) { 'ok' } else { 'OVER' }
    $Rows.Add("| $Measure | $Value | $Limit | $verdict |")
    if (-not $Within) { $Failures.Add("$Measure is $Value, over its target of $Limit") }
}

function Format-Bytes([long]$Bytes) {
    '{0:N2} MB ({1:N0} bytes)' -f ($Bytes / 1MB), $Bytes
}

$exe = (Resolve-Path $Runtime).Path

# Binary: the file the installer ships, and whether it needs the Visual C++
# Redistributable. An import names its DLL in plain ASCII, so the DLL's name
# appears in the file only when the program imports it.
$size = (Get-Item $exe).Length
Add-Row 'Binary size' (Format-Bytes $size) (Format-Bytes $BinaryLimit) ($size -le $BinaryLimit)
$text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($exe))
$redistributable = $text.IndexOf('vcruntime140', [StringComparison]::OrdinalIgnoreCase) -ge 0
$Rows.Add("| Needs the Visual C++ Redistributable | $(if ($redistributable) { 'yes' } else { 'no' }) | no | $(if ($redistributable) { 'OVER' } else { 'ok' }) |")
if ($redistributable) { $Failures.Add('the runtime imports vcruntime140.dll, so it needs the Visual C++ Redistributable') }

$scratch = Join-Path ([IO.Path]::GetTempPath()) "appytappy-footprint-$PID"
$local = Join-Path $scratch 'local'
$roaming = Join-Path $scratch 'roaming'
$appFolder = Join-Path $roaming 'AppyTappy\apps\probe'
New-Item -ItemType Directory -Force -Path $appFolder, $local | Out-Null
$log = Join-Path $local 'AppyTappy\logs\runtime.log'

# Three values in the probe's window: two color checks and a bar fill.
$region = { param($x, $y, $width, $height) @{ windowSize = @{ width = 400; height = 300 }; region = @{ x = $x; y = $y; width = $width; height = $height } } }
$light = & $region 10 10 120 40
$light.check = @{ kind = 'color'; color = @{ red = 192; green = 192; blue = 192 }; tolerance = 8 }
$green = & $region 140 10 120 40
$green.check = @{ kind = 'color'; color = @{ red = 0; green = 255; blue = 0 }; tolerance = 0 }
$fill = & $region 10 60 250 20
$fill.check = @{ kind = 'barFill'; filled = @{ red = 192; green = 192; blue = 192 }; empty = @{ red = 105; green = 105; blue = 105 } }
$app = @{
    format = 1
    name = 'Footprint probe'
    window = @{ title = $ProbeTitle; class = '*'; executable = $ProbeExecutable; mustMatch = @{ title = $true; class = $false }; fallback = 'none' }
    antiCheat = 'none'
    values = @(
        @{ id = 'light'; name = 'Light'; screen = $light },
        @{ id = 'green'; name = 'Green'; screen = $green },
        @{ id = 'fill'; name = 'Fill'; screen = $fill }
    )
    references = @()
}
$nothing = @(@{ kind = 'volume'; percent = 50; app = 'AppyTappyFootprintNoSuchProgram.exe' })
# The probe alternates silver and dim gray, so `light` fires on the first
# silver frame and its hour-long cooldown keeps it from firing again, and
# the bar reads full or empty, never above 1000. The green check never
# holds.
$automations = @{
    format = 1
    automations = @(
        @{ id = 'light'; name = 'Light'; enabled = $true; when = @{ app = 'probe'; value = 'light'; is = 'equals'; target = $true }; actions = $nothing; undo = $false; cooldownMs = 3600000 },
        @{ id = 'green'; name = 'Green'; enabled = $true; when = @{ app = 'probe'; value = 'green'; is = 'equals'; target = $true }; actions = $nothing; undo = $false },
        @{ id = 'fill'; name = 'Fill'; enabled = $true; when = @{ app = 'probe'; value = 'fill'; is = 'above'; target = 1000 }; actions = $nothing; undo = $false }
    )
}
$utf8 = New-Object Text.UTF8Encoding $false
[IO.File]::WriteAllText((Join-Path $appFolder 'app.json'), (ConvertTo-Json $app -Depth 10), $utf8)
[IO.File]::WriteAllText((Join-Path $roaming 'AppyTappy\automations.json'), (ConvertTo-Json $automations -Depth 10), $utf8)

# The probe: a program of its own, so no other process shares its
# executable's name, that shows a borderless 400 by 300 window and repaints
# it every 16 ms. It is DPI aware, so its size is in physical pixels at any
# display scale.
$probeExe = Join-Path $scratch $ProbeExecutable
Add-Type -OutputType WindowsApplication -OutputAssembly $probeExe -ReferencedAssemblies System.Windows.Forms, System.Drawing -TypeDefinition @'
using System.Drawing;
using System.Runtime.InteropServices;
using System.Windows.Forms;

public static class Probe {
    [DllImport("user32.dll")]
    static extern bool SetProcessDPIAware();

    [System.STAThread]
    public static void Main(string[] arguments) {
        SetProcessDPIAware();
        var form = new Form {
            Text = arguments[0],
            FormBorderStyle = FormBorderStyle.None,
            StartPosition = FormStartPosition.Manual,
            Location = new Point(40, 40),
            ClientSize = new Size(400, 300),
        };
        var colors = new[] { Color.Silver, Color.DimGray };
        var frame = 0;
        var timer = new Timer { Interval = 16 };
        timer.Tick += (sender, tick) => { frame = 1 - frame; form.BackColor = colors[frame]; };
        timer.Start();
        Application.Run(form);
    }
}
'@

function Wait-Log([string]$Line, [int]$Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        if ((Test-Path $log) -and (Select-String -Path $log -SimpleMatch -Quiet -Pattern $Line)) { return }
        if ($script:process.HasExited) { throw "the runtime exited with code $($script:process.ExitCode) before its log said '$Line'" }
        Start-Sleep -Milliseconds 250
    }
    throw "the runtime's log did not say '$Line' within $Seconds seconds"
}

function Get-PrivateWorkingSet([int]$Id) {
    (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$Id").WorkingSetPrivate
}

# Measures one phase: CPU over the sample as a share of one core, and the
# private working set at its end.
function Measure-Phase([string]$Name, [long]$MemoryLimit, [double]$CpuLimit) {
    Start-Sleep -Seconds $Settle
    $script:process.Refresh()
    $before = $script:process.TotalProcessorTime
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds $Sample
    $script:process.Refresh()
    if ($script:process.HasExited) { throw "the runtime exited during the $Name sample" }
    $cpu = 100 * ($script:process.TotalProcessorTime - $before).TotalSeconds / $clock.Elapsed.TotalSeconds
    $memory = Get-PrivateWorkingSet $script:process.Id
    Add-Row "$Name memory" (Format-Bytes $memory) (Format-Bytes $MemoryLimit) ($memory -le $MemoryLimit)
    Add-Row "$Name CPU" ('{0:N3} %' -f $cpu) ('{0:N3} %' -f $CpuLimit) ($cpu -le $CpuLimit)
    $script:details += "$($Name): working set $(Format-Bytes $script:process.WorkingSet64), commit $(Format-Bytes $script:process.PrivateMemorySize64), $($script:process.Threads.Count) threads, $($script:process.HandleCount) handles"
}

$script:details = @()
$script:process = $null
$probe = $null
$env:LOCALAPPDATA = $local
$env:APPDATA = $roaming
try {
    $script:process = Start-Process -FilePath $exe -PassThru
    Wait-Log 'runtime started' 10
    Wait-Log 'automations run for Footprint probe' 10

    Measure-Phase 'Idle' $IdleMemoryLimit $IdleCpuLimit

    $probe = Start-Process -FilePath $probeExe -ArgumentList "`"$ProbeTitle`"" -PassThru
    Wait-Log 'attached' 30
    Wait-Log 'Light: runs' 30
    Measure-Phase 'Active' $ActiveMemoryLimit $ActiveCpuLimit

    Stop-Process -Id $probe.Id
    $probe = $null
    Wait-Log 'detached' 10
    Measure-Phase 'Idle after' $ActiveMemoryLimit $IdleCpuLimit
}
catch {
    $Failures.Add($_.Exception.Message)
}
finally {
    if ($null -ne $probe -and -not $probe.HasExited) { Stop-Process -Id $probe.Id }
    $peak = $null
    if ($null -ne $script:process -and -not $script:process.HasExited) {
        $script:process.Refresh()
        $peak = $script:process.PeakWorkingSet64
        Stop-Process -Id $script:process.Id
        $script:process.WaitForExit()
    }
}

$report = @('| Measure | Value | Target | Result |', '| --- | --- | --- | --- |') + $Rows
$report += ''
$report += $script:details | ForEach-Object { "- $_" }
if ($null -ne $peak) { $report += "- Peak working set: $(Format-Bytes $peak)" }
$report | Write-Output
if ($env:GITHUB_STEP_SUMMARY) {
    @('## Runtime footprint', '') + $report | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Append -Encoding utf8
}
if ($Failures.Count -gt 0) {
    if (Test-Path $log) {
        Write-Output '', "The runtime's log:"
        Get-Content $log | Write-Output
    }
    Remove-Item -Recurse -Force $scratch
    $Failures | ForEach-Object { [Console]::Error.WriteLine("footprint: $_") }
    exit 1
}
Remove-Item -Recurse -Force $scratch
