# Benchmark de notty de punta a punta: arranque (hasta el primer pintado, abrir el
# archivo incluido), memoria, CPU en reposo y fluidez al hacer scroll.
#
# Uso:
#   tools/bench.ps1                                   # target/release/notty.exe
#   tools/bench.ps1 -Exe ruta\notty.exe -Runs 5 -Out resultados.json
#
# Cada ejecución usa un %APPDATA%/%LOCALAPPDATA% temporal propio (configuración de
# serie, sin sesión anterior y sin reenviar el archivo a otra ventana de notty abierta),
# así que no toca la configuración real ni otras instancias.

param(
    [string]$Exe = "$PSScriptRoot\..\target\release\notty.exe",
    [int]$Runs = 5,
    [int]$IdleSeconds = 10,
    [string]$Out = "",
    # Solo estos escenarios (por defecto, todos): -Scenarios vacio,log_50MB
    [string[]]$Scenarios = @()
)

$ErrorActionPreference = "Stop"
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class NB {
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("kernel32.dll")] public static extern bool QueryProcessCycleTime(IntPtr h, out ulong c);
    public static double Mcycles(IntPtr h) { ulong c; QueryProcessCycleTime(h, out c); return c / 1e6; }
    public struct RECT { public int L, T, R, B; }
}
"@

$Exe = (Resolve-Path $Exe).Path
$root = Join-Path $env:TEMP "notty-bench"
New-Item -ItemType Directory -Force $root | Out-Null

# --- Archivos de prueba (se generan una vez) ---
function New-TestFile([string]$name, [int]$bytes, [scriptblock]$line) {
    $path = Join-Path $root $name
    if ((Test-Path $path) -and (Get-Item $path).Length -ge $bytes) { return $path }
    $sb = [System.Text.StringBuilder]::new($bytes + 256)
    $i = 0
    while ($sb.Length -lt $bytes) { [void]$sb.Append((& $line $i)); $i++ }
    [System.IO.File]::WriteAllText($path, $sb.ToString(), [System.Text.UTF8Encoding]::new($false))
    return $path
}
$rust = "pub fn handle(req: &Request, n: usize) -> Result<Response, Error> {`n    let id = req.id + n; // comentario`n    if id % 2 == 0 { return Ok(Response::new(`"par`", id)); }`n    Err(Error::Odd(id))`n}`n`n"
$md = "## Sección`n`nTexto con **negrita**, *cursiva*, ``código`` y un [enlace](https://example.com).`n`n- uno`n- dos`n  - [x] tarea`n`n> cita`n`n``````rust`nfn main() { println!(`"hola`"); }`n```````n`n| a | b |`n|---|---|`n| 1 | 2 |`n`n"
$files = [ordered]@{
    "vacio"      = $null
    "rust_1MB"   = New-TestFile "code_1mb.rs" 1MB { param($i) $rust }
    "log_10MB"   = New-TestFile "log_10mb.txt" 10MB { param($i) "2026-09-27 12:00:$($i % 60) INFO request id=$i path=/api/items/$($i * 7) status=200 time=$($i % 97)ms`n" }
    "log_50MB"   = New-TestFile "log_50mb.txt" 50MB { param($i) "2026-09-27 12:00:$($i % 60) INFO request id=$i path=/api/items/$($i * 7) status=200 time=$($i % 97)ms`n" }
    "md_1MB"     = New-TestFile "readme_1mb.md" 1MB { param($i) $md }
}

function New-Profile {
    $p = Join-Path $root ("perfil-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force "$p\Roaming\notty", "$p\Local" | Out-Null
    @"
first_run_done = true

[ui]
md_open_mode = "preview"

[files]
open_in_existing_window = false

[hotkey]
start_with_windows = false
"@ | Set-Content -Encoding utf8 "$p\Roaming\notty\config.toml"
    return $p
}

function Get-Paints([string]$log) {
    if (-not (Test-Path $log)) { return @() }
    Get-Content $log | Where-Object { $_ -like "p *" } | ForEach-Object {
        $f = $_.Split(" "); [pscustomobject]@{ t = [int64]$f[1]; us = [int64]$f[2] }
    }
}

function Pct([double[]]$xs, [double]$p) {
    if ($xs.Count -eq 0) { return 0 }
    $s = $xs | Sort-Object; $s[[math]::Min($s.Count - 1, [int][math]::Floor($p * $s.Count))]
}

function Measure-Run([string]$file) {
    $prof = New-Profile
    $log = Join-Path $prof "bench.log"
    $psi = [System.Diagnostics.ProcessStartInfo]::new($Exe)
    if ($file) { $psi.Arguments = "`"$file`"" }
    $psi.UseShellExecute = $false
    $psi.Environment["APPDATA"] = "$prof\Roaming"
    $psi.Environment["LOCALAPPDATA"] = "$prof\Local"
    $psi.Environment["NOTTY_BENCH_LOG"] = $log
    $proc = [System.Diagnostics.Process]::Start($psi)
    $startMs = [DateTimeOffset]::new($proc.StartTime.ToUniversalTime()).ToUnixTimeMilliseconds()

    $deadline = (Get-Date).AddSeconds(60)
    while (-not (Get-Paints $log) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 20 }
    $first = (Get-Paints $log)[0]
    if (-not $first) { $proc.Kill(); throw "notty no llegó a pintar: $file" }
    $startup = $first.t - $startMs

    Start-Sleep -Milliseconds 1500            # que acaben animaciones y cargas diferidas
    $proc.Refresh()
    $hwnd = $proc.MainWindowHandle
    $private = $proc.PrivateMemorySize64 / 1MB
    $ws = $proc.WorkingSet64 / 1MB

    # --- Reposo ---
    $cpu0 = [NB]::Mcycles($proc.Handle)
    $idleFrom = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    Start-Sleep -Seconds $IdleSeconds
    $idleTo = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $proc.Refresh()
    $idleCpu = [NB]::Mcycles($proc.Handle) - $cpu0
    $idlePaints = @(Get-Paints $log | Where-Object { $_.t -ge $idleFrom -and $_.t -lt $idleTo }).Count

    # --- Scroll: 120 pasos de rueda a ~60 Hz (baja 60, sube 60) ---
    $r = New-Object NB+RECT; [void][NB]::GetWindowRect($hwnd, [ref]$r)
    $cx = [int](($r.L + $r.R) / 2); $cy = [int](($r.T + $r.B) / 2)
    $lp = [IntPtr](($cy -shl 16) -bor ($cx -band 0xFFFF))
    $cpu0 = [NB]::Mcycles($proc.Handle)
    $scrollFrom = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    for ($k = 0; $k -lt 120; $k++) {
        $delta = if ($k -lt 60) { -120 } else { 120 }
        $wp = [IntPtr]([int64](($delta -band 0xFFFF) -shl 16))
        [void][NB]::PostMessageW($hwnd, 0x020A, $wp, $lp)
        Start-Sleep -Milliseconds 16
    }
    Start-Sleep -Milliseconds 300
    $proc.Refresh()
    $scrollCpu = [NB]::Mcycles($proc.Handle) - $cpu0
    $frames = @(Get-Paints $log | Where-Object { $_.t -ge $scrollFrom } | ForEach-Object { $_.us / 1000.0 })

    [void]$proc.CloseMainWindow()
    if (-not $proc.WaitForExit(5000)) { $proc.Kill() }
    Remove-Item -Recurse -Force $prof -ErrorAction SilentlyContinue

    [pscustomobject]@{
        startup_ms      = $startup
        first_paint_ms  = [math]::Round($first.us / 1000.0, 1)
        private_mb      = [math]::Round($private, 1)
        working_set_mb  = [math]::Round($ws, 1)
        idle_cpu_mcycles = [math]::Round($idleCpu, 0)
        idle_paints     = $idlePaints
        scroll_cpu_mcycles = [math]::Round($scrollCpu, 0)
        scroll_frames   = $frames.Count
        frame_ms_avg    = if ($frames.Count) { [math]::Round(($frames | Measure-Object -Average).Average, 2) } else { 0 }
        frame_ms_p95    = [math]::Round((Pct $frames 0.95), 2)
    }
}

function Median([object[]]$rows, [string]$key) {
    $v = $rows | ForEach-Object { [double]$_.$key } | Sort-Object
    $v[[int][math]::Floor(($v.Count - 1) / 2)]
}

$results = [ordered]@{}
foreach ($name in $files.Keys) {
    if ($Scenarios.Count -and $name -notin $Scenarios) { continue }
    Write-Host "== $name ($Runs ejecuciones)"
    $rows = @(1..$Runs | ForEach-Object { Measure-Run $files[$name] })
    $m = [ordered]@{}
    foreach ($k in $rows[0].PSObject.Properties.Name) { $m[$k] = Median $rows $k }
    $results[$name] = $m
    [pscustomobject]$m | Format-List | Out-String | Write-Host
}

$json = [ordered]@{ exe = $Exe; runs = $Runs; idle_seconds = $IdleSeconds; date = (Get-Date).ToString("s"); results = $results } | ConvertTo-Json -Depth 5
if ($Out) { $json | Set-Content -Encoding utf8 $Out; Write-Host "Guardado en $Out" }
