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

## Results (2026-10-30, evening run)

| Client | Scene | FPS avg | Frame p95 | Map load | Peak RSS | CPU avg | Threads |
|---|---|---|---|---|---|---|---|
| OpenKF | solo orbit | 651 | 1.83 ms | 0.39 s | 835 MB | 10% | 30 |
| OpenKF | live, 162 actors | 201 | 5.81 ms | 0.27 s* | 1084 MB | 0.2% | 33 |
| Retail | solo | 120† | 8.86 ms† | 3.26 s | 596 MB | 5% | 45 |

\* Live scene load equals solo load (same map); the 25 s join/wait is gameplay,
not loading.

† Retail presented FPS from PresentMon (see below), not the scripted orbit:
retail hits its own engine cap (`MaxClientFrameRate=120`) in solo WestLondon.
Retail is a 32-bit D3D9 client; OpenKF is 64-bit Vulkan.

## Presented FPS (PresentMon 2.6.0, 20 s each, windowed)

| Client | Scene | Presents | FPS avg | Frame p50 | Frame p95 | Frame p99 |
|---|---|---|---|---|---|---|
| OpenKF (release) | live, wave running, 99 actors + 303 props, 1600x900 | 6153 | 313 | 3.13 ms | 4.15 ms | 4.70 ms |
| Retail | solo WestLondon, empty map | 2401 | 120 | 8.32 ms | 8.86 ms | 9.12 ms |

Notes:

- Both captured windowed (DWM composed) on the same display after a 20 s
  cooldown. OpenKF uses uncapped `Immediate` presentation; retail uses its
  default config, which caps at 120 FPS (`MaxClientFrameRate=+120.0`).
- OpenKF's capture carries a heavier load (live server, wave running, full
  actor/prop pools skinned every frame) than retail's empty solo map.
- An uncapped-retail capture was attempted (`MaxClientFrameRate=1000.0`) but
  PresentMon 2.6.0 wrote no rows for the retail D3D9 process in that
  configuration; the retail config file was restored byte-identical
  afterwards. The capped number above stands as the reproducible baseline.
- Raw captures: `output/pm-openkf.csv`, `output/pm-test.csv`.

## Live FPS variance

Live FPS depends on replicated actor count and server state: 211 FPS at 1 actor
(join), ~100-200 FPS at 150+ actors (wave running). The table reports a loaded
wave (162 actors). Solo orbit is stable at 600-670 FPS with cooldowns.

## Findings

- Map load: OpenKF is ~8x faster (0.39 s vs 3.26 s) — parallel package IO,
  no menu/entry levels, no shader precompilation stalls.
- Presented FPS: OpenKF delivers 313 FPS in a loaded live scene vs retail's
  120 FPS engine-capped solo baseline (2.6x the frames, p95 4.15 ms vs 8.86 ms).
- RAM: OpenKF peaked at 1.1 GB live / 0.8 GB solo (down from 2.2 / 1.2 GB
  before the memory work: packages are now memory-mapped instead of
  read+leaked whole, and pool actors share parsed meshes/animations via
  `Arc`). Steady state after load is ~660 MB, near retail's 596 MB.
  Retail still streams more aggressively; the remaining gap is GPU staging
  and decoded-texture copies during load.
- CPU: OpenKF spreads load over threads (rayon skinning, net/audio threads);
  retail leans on fewer cores. Both stay well under saturation on this host.
- Threads: retail spawns more OS threads (43-47, engine subsystems); OpenKF
  runs 30-33 (Tokio-free: render, net, audio, skinning pool).

## Raw data

- `output/bench/bench-2026-10-30_18-29-35.json` (solo 651 FPS / live 201 FPS).
- Retail: load 3.258 s, RSS 595.8 MB, CPU 5.2%, 45 threads (same run).
- PresentMon: `output/pm-openkf.csv` (6153 presents), `output/pm-test.csv`
  (2401 presents).
- Logs: `output/bench/bench-*-openkf-*.log`, retail engine log
  `<KF>\System\KillingFloor_Bench.log`.

## Reproduce

```powershell
powershell -ExecutionPolicy Bypass -File scripts/bench.ps1 -Frames 120 -LiveSeconds 25
```

Requires the retail game (for assets + retail comparison) and a Rust toolchain.
Retail-only: `powershell -ExecutionPolicy Bypass -File output/bench-retail.ps1`
(template; adjust paths).
