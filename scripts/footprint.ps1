# Measures the shipped runtime on Windows and fails when it exceeds a
# footprint target. `mise run rust:footprint` builds the runtime and runs it;
# wiki/developer/Architecture.md#footprint says what each phase and target
# measures. The targets are the constants below.
#
# The runtime gets scratch LOCALAPPDATA and APPDATA folders holding one
# application, a window this script opens, and three automations that watch
# it. One automation fires once each time the window opens, which proves the
# runtime reads the window; its action sets the volume of a program that does
# not exist, so it changes nothing. The runtime runs idle while the window is
# closed, active while it is open and repainting every 16 ms the way a game
# does, active again with the runtime trace recording everything, steady while
# the window still repaints but only outside the values, and idle again once
# it closes. The trace phase has no targets: it reports what recording costs.
# Between the steady and idle phases, a settle check opens the window once
# more, changes a value once and then stops drawing, and fails unless the
# runtime sees that change.

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
$IdleCpuLimit = 0.1
$ActiveMemoryLimit = 6MB
$ActiveCpuLimit = 2.0
# The most CPU samples a phase takes when its first is over its target; the
# lowest counts against the target.
$CpuSamples = 3

# Sampling: how long each phase settles before its sample, and how long the
# sample runs. Windows charges CPU time in 15.6 ms ticks, so an idle sample
# runs long enough that its limit spans several: 0.1 % of 60 s is 60 ms.
$Settle = 5
$IdleSample = 60
$ActiveSample = 30

$ProbeTitle = 'AppyTappy footprint probe'
$ProbeExecutable = 'appytappy-footprint-probe.exe'
$Failures = New-Object System.Collections.Generic.List[string]
$Rows = New-Object System.Collections.Generic.List[string]

function Add-Row([string]$Measure, [string]$Value, [string]$Limit, [bool]$Within) {
    $verdict = if ($Within) { 'ok' } else { 'OVER' }
    $Rows.Add("| $Measure | $Value | $Limit | $verdict |")
    if (-not $Within) { $Failures.Add("$Measure is $Value, over its target of $Limit") }
}

# A measurement with no target, which the run reports and never fails on.
function Add-Measure([string]$Measure, [string]$Value) {
    $Rows.Add("| $Measure | $Value | none | measured |")
}

function Format-Bytes([long]$Bytes) {
    '{0:N2} MB ({1:N0} bytes)' -f ($Bytes / 1MB), $Bytes
}

$exe = (Resolve-Path $Runtime).Path

# Binary: the file the installer ships, and whether it needs the Visual C++
# Redistributable. An import names its DLL in plain ASCII, so a DLL the
# program imports appears in the file by name; a text that happens to name
# one fails the check too, which errs the safe way.
$size = (Get-Item $exe).Length
Add-Row 'Binary size' (Format-Bytes $size) (Format-Bytes $BinaryLimit) ($size -le $BinaryLimit)
$text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($exe))
$imports = [regex]::Matches($text, '(vcruntime|msvcp|concrt)1\d\d[\w]*\.dll', 'IgnoreCase') | ForEach-Object { $_.Value.ToLowerInvariant() } | Sort-Object -Unique
$redistributable = @($imports).Count -gt 0
$Rows.Add("| Needs the Visual C++ Redistributable | $(if ($redistributable) { 'yes' } else { 'no' }) | no | $(if ($redistributable) { 'OVER' } else { 'ok' }) |")
if ($redistributable) { $Failures.Add("the runtime names $($imports -join ', '), so it needs the Visual C++ Redistributable") }

$utf8 = New-Object Text.UTF8Encoding $false
$scratch = Join-Path ([IO.Path]::GetTempPath()) "appytappy-footprint-$PID"
$local = Join-Path $scratch 'local'
$roaming = Join-Path $scratch 'roaming'
$appFolder = Join-Path $roaming 'AppyTappy\apps\probe'
$log = Join-Path $local 'AppyTappy\logs\runtime.log'
$traces = Join-Path $local 'AppyTappy\traces'
$script:details = @()
$script:memory = @{}
$script:cpu = @{}
$script:process = $null
$probe = $null
$peak = $null

# How many lines of the log contain `$Line`.
function Measure-Log([string]$Line) {
    if (-not (Test-Path $log)) { return 0 }
    @(Select-String -Path $log -SimpleMatch -Pattern $Line).Count
}

# Waits until the runtime's log holds `Line` at least `Count` times.
function Wait-Log([string]$Line, [int]$Seconds, [int]$Count = 1) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        if ((Measure-Log $Line) -ge $Count) { return }
        if ($script:process.HasExited) { throw "the runtime exited with code $($script:process.ExitCode) before its log said '$Line'" }
        Start-Sleep -Milliseconds 250
    }
    throw "the runtime's log did not say '$Line' within $Seconds seconds"
}

function Get-PrivateWorkingSet([int]$Id) {
    $counter = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$Id"
    if ($null -eq $counter -or -not $counter.WorkingSetPrivate) { throw "Windows reported no private working set for process $Id" }
    [long]$counter.WorkingSetPrivate
}

# The runtime's CPU over one sample of `Seconds`, as a share of one core.
function Measure-Cpu([string]$Name, [int]$Seconds) {
    $script:process.Refresh()
    $before = $script:process.TotalProcessorTime
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds $Seconds
    $script:process.Refresh()
    if ($script:process.HasExited) { throw "the runtime exited with code $($script:process.ExitCode) during the $Name sample" }
    100 * ($script:process.TotalProcessorTime - $before).TotalSeconds / $clock.Elapsed.TotalSeconds
}

# Measures one phase: CPU over the sample as a share of one core, and the
# private working set at its end. A phase with a memory limit of 0 has no
# targets, so it only reports. A shared CI machine can charge the runtime a
# stray tick, which is most of an idle target, so a CPU sample over its target
# is taken again, up to $CpuSamples in all, and the lowest counts. A runtime
# that is really over its target is over it in every sample, and the details
# list every sample taken.
function Measure-Phase([string]$Name, [int]$Seconds, [long]$MemoryLimit, [double]$CpuLimit) {
    Start-Sleep -Seconds $Settle
    $samples = @(Measure-Cpu $Name $Seconds)
    while ($MemoryLimit -gt 0 -and ($samples | Measure-Object -Minimum).Minimum -gt $CpuLimit -and $samples.Count -lt $CpuSamples) {
        $samples += Measure-Cpu $Name $Seconds
    }
    if ($samples.Count -gt 1) { $script:details += "$Name CPU samples: $(($samples | ForEach-Object { '{0:N3} %' -f $_ }) -join ', ')" }
    $cpu = ($samples | Measure-Object -Minimum).Minimum

    $memory = Get-PrivateWorkingSet $script:process.Id
    $script:memory[$Name] = $memory
    $script:cpu[$Name] = $cpu
    if ($MemoryLimit -gt 0) {
        Add-Row "$Name memory" (Format-Bytes $memory) (Format-Bytes $MemoryLimit) ($memory -le $MemoryLimit)
        Add-Row "$Name CPU" ('{0:N3} %' -f $cpu) ('{0:N3} %' -f $CpuLimit) ($cpu -le $CpuLimit)
    }
    else {
        Add-Measure "$Name memory" (Format-Bytes $memory)
        Add-Measure "$Name CPU" ('{0:N3} %' -f $cpu)
    }
    $script:details += "$($Name): working set $(Format-Bytes $script:process.WorkingSet64), commit $(Format-Bytes $script:process.PrivateMemorySize64), $($script:process.Threads.Count) threads, $($script:process.HandleCount) handles"
}

# Every value has to read without a problem, or an active sample measured
# the error path instead of the reads.
function Assert-Read {
    $unread = Select-String -Path $log -SimpleMatch -Pattern 'cannot be read', 'cannot be captured', 'capture ended badly'
    if ($unread) { throw "the runtime could not read the probe: $(@($unread)[0].Line)" }
}

# Saves settings with the trace on or off, recording every category with no
# end time, and asks the runtime to read them, as the setup app does after a
# save.
function Set-Trace([bool]$On) {
    $settings = @{
        format = 1
        trace = @{ on = $On; until = $null; record = @('readings', 'changes', 'rules', 'actions', 'lifecycle'); fileLimitMb = 50; keepFiles = 5 }
    }
    [IO.File]::WriteAllText((Join-Path $roaming 'AppyTappy\settings.json'), (ConvertTo-Json $settings -Depth 5), $utf8)
    $reload = [Threading.EventWaitHandle]::OpenExisting('Local\AppyTappy.Reload')
    try { $null = $reload.Set() } finally { $reload.Dispose() }
}

# A trace that could not start or stopped partway records less than the
# phase measured, so its numbers would understate what recording costs.
function Assert-Trace {
    $failed = Select-String -Path $log -SimpleMatch -Pattern 'the settings cannot be read', 'the trace cannot start', 'the trace cannot be written', 'the trace stopped badly'
    if ($failed) { throw "the trace could not record: $(@($failed)[0].Line)" }
}

# Reports the trace files: their size, the rows the runtime recorded, those
# rows each second, and the records the trace dropped because its buffer was
# full. The trace's own rows, on the trace thread, are left out of the count.
function Measure-Trace {
    Assert-Trace
    $files = @(Get-ChildItem -Path $traces -Filter 'trace-*.csv' -ErrorAction SilentlyContinue)
    if ($files.Count -eq 0) { throw "the trace wrote no file to $traces" }
    $recorded = 0
    $dropped = 0
    $last = 0
    foreach ($file in $files) {
        foreach ($line in [IO.File]::ReadLines($file.FullName)) {
            if ($line.StartsWith('time_ns,')) { continue }
            $fields = $line.Split(',')
            $last = [Math]::Max($last, [long]$fields[0])
            if ($fields[1] -ne 'trace') { $recorded += 1 }
            elseif ($fields[2] -eq 'dropped') { $dropped += [long]$fields[5] }
        }
    }
    $bytes = ($files | Measure-Object -Property Length -Sum).Sum
    $seconds = $last / 1e9
    Add-Measure 'Trace files' "$($files.Count), $(Format-Bytes $bytes)"
    Add-Measure 'Trace rows' ('{0:N0} over {1:N1} s, {2:N0} a second' -f $recorded, $seconds, ($recorded / [Math]::Max($seconds, 1)))
    Add-Measure 'Trace records dropped' ('{0:N0}' -f $dropped)
    Add-Measure 'Trace on adds' ('{0} memory, {1:N3} % CPU' -f (Format-Bytes ($script:memory['Active, trace on'] - $script:memory['Active'])), ($script:cpu['Active, trace on'] - $script:cpu['Active']))
}

try {
    New-Item -ItemType Directory -Force -Path $appFolder, $local | Out-Null

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
    # silver frame and its hour-long cooldown keeps it from firing again until
    # the window opens anew, which starts the cooldown over. The
    # bar reads full or empty, a share of at most 100, and the green check
    # never holds.
    $automations = @{
        format = 1
        automations = @(
            @{ id = 'light'; name = 'Light'; enabled = $true; when = @{ app = 'probe'; value = 'light'; is = 'equals'; target = $true }; actions = $nothing; undo = $false; cooldownMs = 3600000 },
            @{ id = 'green'; name = 'Green'; enabled = $true; when = @{ app = 'probe'; value = 'green'; is = 'equals'; target = $true }; actions = $nothing; undo = $false },
            @{ id = 'fill'; name = 'Fill'; enabled = $true; when = @{ app = 'probe'; value = 'fill'; is = 'above'; target = 1000 }; actions = $nothing; undo = $false }
        )
    }
    [IO.File]::WriteAllText((Join-Path $appFolder 'app.json'), (ConvertTo-Json $app -Depth 10), $utf8)
    [IO.File]::WriteAllText((Join-Path $roaming 'AppyTappy\automations.json'), (ConvertTo-Json $automations -Depth 10), $utf8)

    # The probe: a program of its own, so no other process shares its
    # executable's name, that shows a borderless 400 by 300 window and repaints
    # it every 16 ms. It is DPI aware, so its size is in physical pixels at any
    # display scale. Given `steady`, it keeps the values' part silver and
    # repaints only a strip below them, so the window changes while its
    # values hold still. Given `settle`, it does the same with the values'
    # part dim gray for eight seconds from when the window shows, long enough
    # for the capture to ask for few frames, then in one repaint turns that
    # part silver, stops the strip and draws nothing more.
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
        var mode = arguments.Length > 1 ? arguments[1] : "";
        var steady = mode == "steady" || mode == "settle";
        var settle = mode == "settle";
        var strip = new Panel { Location = new Point(0, 200), Size = new Size(400, 100) };
        form.Controls.Add(strip);
        form.BackColor = settle ? Color.DimGray : Color.Silver;
        var frame = 0;
        var shown = new System.Diagnostics.Stopwatch();
        form.Shown += (sender, shownEvent) => shown.Start();
        var timer = new Timer { Interval = 16 };
        timer.Tick += (sender, tick) => {
            if (settle && shown.ElapsedMilliseconds >= 8000) {
                form.BackColor = Color.Silver;
                timer.Stop();
                return;
            }
            frame = 1 - frame;
            if (steady) { strip.BackColor = colors[frame]; } else { form.BackColor = colors[frame]; strip.BackColor = colors[frame]; }
        };
        timer.Start();
        Application.Run(form);
    }
}
'@

    $env:LOCALAPPDATA = $local
    $env:APPDATA = $roaming
    $script:process = Start-Process -FilePath $exe -PassThru
    # Reading the handle once keeps it open, so the exit code survives the
    # runtime's exit.
    $null = $script:process.Handle
    Wait-Log 'runtime started' 10
    Wait-Log 'automations run for Footprint probe' 10

    Measure-Phase 'Idle' $IdleSample $IdleMemoryLimit $IdleCpuLimit

    $probe = Start-Process -FilePath $probeExe -ArgumentList "`"$ProbeTitle`"" -PassThru
    $null = $probe.Handle
    Wait-Log '): attached' 30
    Wait-Log 'Light: runs' 30
    Measure-Phase 'Active' $ActiveSample $ActiveMemoryLimit $ActiveCpuLimit
    Assert-Read

    # The same load with the trace recording every category, which costs the
    # most. Turning it on restarts the readers, which the settle time covers.
    Set-Trace $true
    try { Wait-Log 'the trace records to' 10 } catch { Assert-Trace; throw }
    Measure-Phase 'Active, trace on' $ActiveSample 0 0
    Assert-Read
    Assert-Trace
    $stops = Measure-Log 'the trace stopped'
    $reads = Measure-Log 'reading its values'
    Set-Trace $false
    Wait-Log 'the trace stopped' 10 ($stops + 1)
    # Stopping the trace starts the reader over. Its capture starts just after
    # it logs that it reads, and fails to start on a window that is closing,
    # so the probe stays open until it has.
    Wait-Log 'reading its values' 10 ($reads + 1)
    Start-Sleep -Seconds 2

    Stop-Process -Id $probe.Id
    $probe.WaitForExit()
    $probe = $null
    Wait-Log '): detached' 10

    $probe = Start-Process -FilePath $probeExe -ArgumentList "`"$ProbeTitle`"", 'steady' -PassThru
    $null = $probe.Handle
    Wait-Log '): attached' 30 2
    # The new attach starts the cooldown over, so `light` firing again shows
    # the reader reads this window too.
    Wait-Log 'Light: runs' 30 2
    Measure-Phase 'Steady' $ActiveSample $ActiveMemoryLimit $ActiveCpuLimit
    Assert-Read

    Stop-Process -Id $probe.Id
    $probe.WaitForExit()
    $probe = $null
    Wait-Log '): detached' 10 2

    # Settle: `light` turns true in the probe's last repaint, while the
    # capture asks for a frame only every half second. If Windows drops that
    # repaint rather than delivering it once the interval passes, it is never
    # read. The check first makes sure the repaint comes at least two seconds
    # after the runtime attached, or it would test nothing.
    $probe = Start-Process -FilePath $probeExe -ArgumentList "`"$ProbeTitle`"", 'settle' -PassThru
    $null = $probe.Handle
    Wait-Log '): attached' 30 3
    Start-Sleep -Seconds 2
    if ((Measure-Log 'Light: runs') -ge 3) {
        throw 'the settle probe turned silver before the runtime had read it for two seconds, so the settle check would test nothing'
    }
    try { Wait-Log 'Light: runs' 20 3 }
    catch { throw "the settle check failed, so a change drawn while the capture asked for few frames may be lost: $($_.Exception.Message)" }
    Assert-Read
    Stop-Process -Id $probe.Id
    $probe.WaitForExit()
    $probe = $null
    Wait-Log '): detached' 10 3

    Measure-Phase 'Idle after' $IdleSample $ActiveMemoryLimit $IdleCpuLimit
    # The stopped trace's writer has long finished its file by now.
    Measure-Trace
}
catch {
    $Failures.Add($_.Exception.Message)
}
finally {
    if ($null -ne $probe) {
        Stop-Process -Id $probe.Id -ErrorAction SilentlyContinue
        $probe.WaitForExit()
    }
    if ($null -ne $script:process -and -not $script:process.HasExited) {
        $script:process.Refresh()
        $peak = $script:process.PeakWorkingSet64
        Stop-Process -Id $script:process.Id -ErrorAction SilentlyContinue
        $script:process.WaitForExit()
    }
}

$report = @('| Measure | Value | Target | Result |', '| --- | --- | --- | --- |') + $Rows
$report += ''
$report += $script:details | ForEach-Object { "- $_" }
if ($null -ne $peak) { $report += "- Peak working set: $(Format-Bytes $peak)" }
$report | Write-Output
if ($env:GITHUB_STEP_SUMMARY) {
    [IO.File]::AppendAllText($env:GITHUB_STEP_SUMMARY, ((@('## Runtime footprint', '') + $report) -join "`n") + "`n", $utf8)
}
if ($Failures.Count -gt 0) {
    if (Test-Path $log) {
        Write-Output '', "The runtime's log:"
        Get-Content $log | Write-Output
    }
    $Failures | ForEach-Object { [Console]::Error.WriteLine("footprint: $_") }
}
Remove-Item -Recurse -Force $scratch -ErrorAction SilentlyContinue
if ($Failures.Count -gt 0) { exit 1 }
