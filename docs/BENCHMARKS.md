# OpenKF benchmarks: retail vs OpenKF

Method, hardware, and raw data for the performance comparison between the
retail Killing Floor client (v1065, 32-bit, D3D9) and OpenKF (64-bit, Vulkan).

## Method

- Same map: `KF-WestLondon` (retail).
- Same resolution: 1280x720 for the scripted runs. OpenKF renders headless
  (offscreen); retail runs windowed-minimized (`1582x922`, engine default;
  resolution does not affect load/RAM/CPU).
- Same camera path (OpenKF): 360-degree yaw orbit over N frames (`--bench N`),
  skinning + render timed with GPU-idle waits. One warmup frame, then N timed.
- OpenKF solo: map only. OpenKF live: joins a local dedicated server
  (`UCC.exe`, same map, its own instance on port 7717), waits 25 s (bot
  readies, match starts, ~150 actors replicate), then the same orbit.
- Retail solo: `KillingFloor.exe KF-WestLondon.rom -log=...`; map load is the
  engine's own `Bringing Level ... up for play ... appSeconds:` marker.
  RSS/CPU/threads sampled at 4 Hz for 20 s after a 15 s settle.
- Presented FPS (both clients): captured with PresentMon 2.6.0
  (`--timed 20`, per-present `MsBetweenPresents`), windowed on the same
  display. Retail runs solo on the map; OpenKF runs windowed live against
  the local server (wave running, 99 actors + 303 props).
- Script: `scripts/bench.ps1`. Raw JSON per run in `output/bench/`.
- Cooldowns of 20 s between runs (GPU thermals move FPS by up to 2x).

## Hardware

- CPU: (see raw JSON `host.cpu`), logical cores (see `host.cores`)
- RAM: (see `host.ram_gb`), GPU: NVIDIA GeForce RTX 4060 Ti, Windows 11.

## Results (2026-10-30, night run, v1.9.0)

| Client | Scene | FPS avg | Frame p95 | Map load | Peak RSS | CPU avg | Threads |
|---|---|---|---|---|---|---|---|
| OpenKF | solo orbit | 671 | 1.69 ms | 0.14 s | 713 MB | 11% | 31 |
| OpenKF | live, ~155 actors‡ | 95-99 | 15.4-16.3 ms | 0.14 s* | 1085 MB | 2% | 33 |
| Retail | solo | 120† | 8.86 ms† | 3.32 s | 597 MB | 5% | 47 |

\* Live scene load equals solo load (same map); the 25 s join/wait is gameplay,
not loading.

† Retail presented FPS from PresentMon (see below), not the scripted orbit:
retail hits its own hard engine cap (120) in solo WestLondon.
Retail is a 32-bit D3D9 client; OpenKF is 64-bit Vulkan.

‡ Two live runs: 99 FPS at 150 actors, 95 FPS at 158 actors — wave combat
with the bot moving, firing and killing (2 kills), 0 sound/decode failures.

Previous run (v1.8.0, evening): solo 651 FPS / 0.39 s / 835 MB; live 201 FPS
at 162 actors / 1084 MB; retail 3.26 s / 596 MB.

## Interim (v1.10.0, 2026-10-31 — clean FPS re-run pending, retail busy)

- Live RAM (public server `185.97.253.240:7810`, custom map
  KF-TheWolfLair, 60 s soak, wave 10, 343 actors, 6877 sounds, 0 sound
  failures): **peak 641 MB** — on par with retail's 597–651 MB solo,
  down from 1085 MB (bounded sound cache + CPU upload release).
- Solo headless peak RSS: **679 MB** (was 713 MB).
- Live frame split (`--bench` orbit, 99 actors, 464 draws): skin 1.62 ms
  + upload 1.64 ms + render+wait 4.14 ms (135 FPS). The render+wait leg
  is inflated here — a retail match was running on the same GPU during
  the run; the CPU-side split (skin/upload) stands.
- FPS cap (`--fps`, default 120): live windowed went from 29.6% CPU at
  218 FPS (uncapped) to **17.4% at 117 FPS** (retail: 5.8% at its
  120 hard cap).
- Full clean-room numbers (scripted orbit + PresentMon, retail idle)
  will refresh the table above in the next run.

## Presented FPS (PresentMon 2.6.0, 20 s each, windowed)

| Client | Scene | Presents | FPS avg | Frame p50 | Frame p95 | Frame p99 |
|---|---|---|---|---|---|---|
| OpenKF v1.9.0 (release) | live, wave running, bot playing, 1600x900 | 4707 | 236 | 4.11 ms | 5.71 ms | 6.91 ms |
| OpenKF v1.8.0 (release) | live, wave running, 99 actors + 303 props, 1600x900 | 6153 | 313 | 3.13 ms | 4.15 ms | 4.70 ms |
| Retail | solo WestLondon, empty map | 2401 | 120 | 8.32 ms | 8.86 ms | 9.12 ms |

Notes:

- All captured windowed (DWM composed) on the same display after a 20 s
  cooldown. OpenKF uses uncapped `Immediate` presentation; retail caps at
  120 FPS.
- OpenKF's captures carry a heavier load (live server, wave running, full
  actor/prop pools skinned every frame) than retail's empty solo map.
- Retail's 120 is a **hard engine ceiling**: a fullscreen-exclusive
  1600x900 run with `MaxClientFrameRate=400` still reads 117 cur / 120 avg
  `stat fps` (proof: `docs/screenshots/shot-retail-cap.png`). The engine
  also rewrites a plain `MaxClientFrameRate=1000.0` back at boot; only the
  `+400.0` format sticks in the ini. The retail config was restored
  afterwards (120.000000, windowed).
- PresentMon needs `--terminate_after_timed` (without it the process sits
  idle after the timed window) and `--stop_existing_session` (a stale ETW
  session blocks new captures; `logman stop PresentMon -ets` clears it).
- Raw captures: `output/pm-openkf-v19.csv` (v1.9.0), `output/pm-openkf.csv`
  (v1.8.0), `output/pm-test.csv` (retail).

## Live FPS variance

Live FPS depends on replicated actor count and server state. v1.9.0: 95-99
at ~155 actors in wave combat (bot firing, killing, decals + ragdolls
active). Solo orbit is stable at 650-670 FPS with cooldowns.

## Findings

- Map load: OpenKF is ~24x faster (0.14 s vs 3.32 s) — parallel package IO,
  no menu/entry levels, no shader precompilation stalls.
- Presented FPS: OpenKF delivers 236 FPS in live wave combat vs retail's
  120 FPS hard-capped baseline (2x the frames, p95 5.71 ms vs 8.86 ms).
- RAM: OpenKF peaked at 1.1 GB live / 0.7 GB solo (packages memory-mapped,
  pool actors share parsed meshes/animations via `Arc`). Steady state
  after load is ~660 MB, near retail's 597 MB. Retail still streams more
  aggressively; the remaining gap is GPU staging and decoded-texture
  copies during load.
- CPU: OpenKF spreads load over threads (rayon skinning, net/audio threads);
  retail leans on fewer cores. Both stay well under saturation on this host.
- Threads: retail spawns more OS threads (47, engine subsystems); OpenKF
  runs ~31-33 (Tokio-free: render, net, audio, skinning pool).
- Live perf delta vs v1.8.0 (orbit 201 -> 95-99, presented 313 -> 236):
  the new per-frame articulated ragdolls, projected decals, and heavier
  combat effects cost frames. Profiling the live frame time is next;
  real play stays well above 60.

## Raw data

- `output/bench/bench-2026-10-30_20-22-33.json` (solo 671 FPS; live re-run
  clean: 99 FPS @150 actors, 95 FPS @158 actors — the scripted live run was
  poisoned by a stale dedicated server holding port 7717 and re-ran manually).
- Retail: load 3.319 s, RSS 597.3 MB, CPU 4.7%, 47 threads (same run).
- PresentMon: `output/pm-openkf-v19.csv` (4707 presents).
- Logs: `output/bench/bench-*-openkf-*.log`, retail engine log
  `<KF>\System\KillingFloor_Bench.log`.

## Reproduce

```powershell
powershell -ExecutionPolicy Bypass -File scripts/bench.ps1 -Frames 120 -LiveSeconds 25
```

Requires the retail game (for assets + retail comparison) and a Rust toolchain.
Retail-only: `powershell -ExecutionPolicy Bypass -File output/bench-retail.ps1`
(template; adjust paths).
