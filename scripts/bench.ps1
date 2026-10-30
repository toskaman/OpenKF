# OpenKF benchmark: retail vs OpenKF (RAM, CPU, threads, load time, FPS).
# Method: same map (KF-WestLondon), same resolution (1280x720), same camera path
# (360-degree orbit, N frames). OpenKF renders headless (Vulkan offscreen);
# retail is sampled while running the map (FPS is manual: console `stat fps`).
# Raw JSON + logs go to output\bench\.
param(
    [string]$Map = "KF-WestLondon",
    [string]$KF = $(if ($env:KF_DIR) { $env:KF_DIR } else { "C:\Program Files (x86)\Steam\steamapps\common\KillingFloor" }),
    [int]$Frames = 300,
    [string]$Size = "1280x720",
    [int]$LiveSeconds = 25,
    [switch]$SkipRetail,
    [switch]$SkipBuild
)
$ErrorActionPreference = "Continue"
$ROOT = Split-Path -Parent $PSScriptRoot
$OUT = Join-Path $ROOT "output\bench"
New-Item -ItemType Directory -Path $OUT -Force | Out-Null
$stamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
$raw = Join-Path $OUT "bench-$stamp.json"
$results = @()

function Sample-Process($proc, $seconds) {
    # Samples RSS/CPU/threads at 4 Hz; returns peak RSS MB, avg CPU %, threads.
    $rss = @(); $cpu0 = $proc.TotalProcessorTime; $t0 = Get-Date
    $threads = @()
    $n = [int]($seconds * 4)
    for ($i = 0; $i -lt $n; $i++) {
        Start-Sleep -Milliseconds 250
        try {
            $proc.Refresh()
            if ($proc.HasExited) { break }
            $rss += $proc.WorkingSet64 / 1MB
            $threads += $proc.Threads.Count
        } catch { break }
    }
    try { $proc.Refresh(); $cpu1 = $proc.TotalProcessorTime } catch { $cpu1 = $cpu0 }
    $wall = ((Get-Date) - $t0).TotalSeconds
    $cores = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors
    $cpuPct = if ($wall -gt 0) { ($cpu1 - $cpu0).TotalSeconds / $wall / $cores * 100 } else { 0 }
    $peak = if ($rss.Count -gt 0) { ($rss | Measure-Object -Maximum).Maximum } else { 0 }
    $thr = if ($threads.Count -gt 0) { ($threads | Measure-Object -Maximum).Maximum } else { 0 }
    return @{ peak_rss_mb = [math]::Round($peak, 1); cpu_pct = [math]::Round($cpuPct, 1); threads = $thr }
}

if (-not $SkipBuild) {
    Write-Host "[bench] building release viewer..."
    cargo build --release -p openkf-viewer 2>&1 | Select-Object -Last 2
}
$viewer = Join-Path $ROOT "target\release\openkf-viewer.exe"

# --- OpenKF solo: load + orbit FPS ---
Write-Host "[bench] OpenKF solo ($Map, $Frames frames)..."
$logSolo = Join-Path $OUT "bench-$stamp-openkf-solo.log"
$t = Get-Date
$p = Start-Process -FilePath $viewer -ArgumentList "$Map --kf `"$KF`" --bench $Frames --size $Size" -NoNewWindow -PassThru -RedirectStandardOutput $logSolo
$sample = Sample-Process $p 86400
$p.WaitForExit()
$wallSolo = ((Get-Date) - $t).TotalSeconds
$txt = Get-Content $logSolo -Raw -ErrorAction SilentlyContinue
$loadMs = if ($txt -match "\[SceneLoad\] ([\d.]+) ms") { [double]$Matches[1] } else { 0 }
$fps = if ($txt -match "\[Bench\] \d+ frames.*: ([\d.]+) FPS") { [double]$Matches[1] } else { 0 }
$p95 = if ($txt -match "p95 ([\d.]+)") { [double]$Matches[1] } else { 0 }
$results += @{ client = "OpenKF"; scene = "solo"; map = $Map; size = $Size; frames = $Frames; fps_avg = $fps; frame_p95_ms = $p95; load_ms = [math]::Round($loadMs, 0); wall_s = [math]::Round($wallSolo, 1); peak_rss_mb = $sample.peak_rss_mb; cpu_pct = $sample.cpu_pct; threads = $sample.threads }
Write-Host ("[bench] solo: {0} FPS, load {1} ms, RSS {2} MB, CPU {3}%, threads {4}" -f $fps, $loadMs, $sample.peak_rss_mb, $sample.cpu_pct, $sample.threads)

# --- OpenKF live: join local server, orbit FPS with actors ---
Write-Host "[bench] cooling down 20 s..."
Start-Sleep -Seconds 20
Write-Host "[bench] starting local server..."
$sys = Join-Path $KF "System"
Start-Process -FilePath (Join-Path $sys "UCC.exe") -WorkingDirectory $sys -WindowStyle Minimized -ArgumentList "server $Map.rom?game=KFmod.KFGameType?VACSecured=false?MaxPlayers=6 -port=7717 ini=OpenKF_Capture.ini -log=OpenKF_Bench.log" | Out-Null
Start-Sleep -Seconds 10
try {
    Write-Host "[bench] OpenKF live ($LiveSeconds s + $Frames frames)..."
    $logLive = Join-Path $OUT "bench-$stamp-openkf-live.log"
    $t = Get-Date
    $p = Start-Process -FilePath $viewer -ArgumentList "--connect 127.0.0.1:7717 --kf `"$KF`" --bench $Frames --size $Size --time $LiveSeconds" -NoNewWindow -PassThru -RedirectStandardOutput $logLive
    $sample = Sample-Process $p 86400
    $p.WaitForExit()
    $wallLive = ((Get-Date) - $t).TotalSeconds
    $txt = Get-Content $logLive -Raw -ErrorAction SilentlyContinue
    $loadMs = if ($txt -match "\[SceneLoad\] ([\d.]+) ms") { [double]$Matches[1] } else { 0 }
    $fps = if ($txt -match "\[Bench\] \d+ frames.*: ([\d.]+) FPS") { [double]$Matches[1] } else { 0 }
    $p95 = if ($txt -match "p95 ([\d.]+)") { [double]$Matches[1] } else { 0 }
    $m = Select-String -Path $logLive -Pattern "(\d+) actors, \d+ pawns, moves" | Select-Object -Last 1
    $actors = if ($m -and $m.Matches[0].Groups[1].Value -match "^\d+$") { [int]$m.Matches[0].Groups[1].Value } else { 0 }
    $results += @{ client = "OpenKF"; scene = "live"; map = $Map; size = $Size; frames = $Frames; fps_avg = $fps; frame_p95_ms = $p95; load_ms = [math]::Round($loadMs, 0); wall_s = [math]::Round($wallLive, 1); peak_rss_mb = $sample.peak_rss_mb; cpu_pct = $sample.cpu_pct; threads = $sample.threads; actors = $actors }
    Write-Host ("[bench] live: {0} FPS, actors {1}, RSS {2} MB, CPU {3}%, threads {4}" -f $fps, $actors, $sample.peak_rss_mb, $sample.cpu_pct, $sample.threads)
} finally {
    taskkill /IM UCC.exe /F 2>$null | Out-Null
}

# --- Retail: load + RSS/CPU while running the map (FPS manual via `stat fps`) ---
if (-not $SkipRetail) {
    Write-Host "[bench] cooling down 20 s..."
    Start-Sleep -Seconds 20
    Write-Host "[bench] retail client ($Map)..."
    $exe = Join-Path $sys "KillingFloor.exe"
    $benchLog = Join-Path $sys "KillingFloor_Bench.log"
    Remove-Item $benchLog -Force -ErrorAction SilentlyContinue
    $t = Get-Date
    $p = Start-Process -FilePath $exe -WorkingDirectory $sys -WindowStyle Minimized -ArgumentList "$Map.rom?game=KFMod.KFGameType -log=KillingFloor_Bench.log" -PassThru
    # Let the map load (marker appears ~3 s in), sample, kill, then parse the
    # engine log ("Bringing Level <Map>.myLevel up for play ... appSeconds:").
    Start-Sleep -Seconds 15
    $sample = Sample-Process $p 20
    # Parse while running (the log is reliably readable then).
    $ready = 0
    $hit = Select-String -Path $benchLog -Pattern ([regex]::Escape("Bringing Level $($Map).myLevel up for play") + ".*appSeconds: (\d+\.\d+)") -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($hit) {
        $ready = [double]$hit.Matches[0].Groups[1].Value
    }
    try { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } catch {}
    $results += @{ client = "Retail"; scene = "solo"; map = $Map; size = "windowed"; frames = 0; fps_avg = "manual (stat fps)"; frame_p95_ms = "n/a"; load_ms = [math]::Round($ready * 1000, 0); wall_s = [math]::Round($ready, 1); peak_rss_mb = $sample.peak_rss_mb; cpu_pct = $sample.cpu_pct; threads = $sample.threads }
    Write-Host ("[bench] retail: map up in {0} s, RSS {1} MB, CPU {2}%, threads {3}" -f [math]::Round($ready, 1), $sample.peak_rss_mb, $sample.cpu_pct, $sample.threads)
}

$meta = @{
    date = $stamp
    host = @{ cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name); cores = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors; ram_gb = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB, 1); gpu = (Get-CimInstance Win32_VideoController | Select-Object -First 1 -ExpandProperty Name); os = (Get-CimInstance Win32_OperatingSystem).Caption }
    method = "Same map, same 1280x720 offscreen orbit (N frames, 360 deg yaw). OpenKF: headless Vulkan, skinning+render timed on GPU-idle waits. Retail: window-ready time + 20 s RSS/CPU sample; FPS manual via console 'stat fps' (same map, same spot)."
    results = $results
}
$meta | ConvertTo-Json -Depth 6 | Set-Content $raw
Write-Host "[bench] raw data: $raw"
$results | ForEach-Object { [pscustomobject]$_ } | Format-Table client, scene, fps_avg, frame_p95_ms, load_ms, peak_rss_mb, cpu_pct, threads -AutoSize | Out-String | Write-Host
