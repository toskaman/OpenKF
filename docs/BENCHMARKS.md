# OpenKF benchmarks: retail vs OpenKF

Method, hardware, and raw data for the performance comparison between the
retail Killing Floor client (v1065, 32-bit, D3D9) and OpenKF (64-bit, Vulkan).

## Method

- Same map: `KF-WestLondon` (retail).
- Same resolution: 1280x720. OpenKF renders headless (offscreen); retail runs
  windowed-minimized (engine default; resolution does not affect load/RAM/CPU,
  and retail FPS is measured separately, see below).
- Same camera path (OpenKF): 360-degree yaw orbit over N frames (`--bench N`),
  skinning + render timed with GPU-idle waits. One warmup frame, then N timed.
- OpenKF solo: map only. OpenKF live: joins a local dedicated server
  (`UCC.exe`, same map), waits 25 s (bot readies, match starts, ~150 actors
  replicate), then the same orbit.
- Retail solo: `KillingFloor.exe KF-WestLondon.rom -log=...`; map load is the
  engine's own `Bringing Level ... up for play ... appSeconds:` marker.
  RSS/CPU/threads sampled at 4 Hz for 20 s after a 15 s settle.
- Script: `scripts/bench.ps1`. Raw JSON per run in `output/bench/`.
- Cooldowns of 20 s between runs (GPU thermals move FPS by up to 2x).

## Hardware

- CPU: 13th-gen Intel desktop (see run JSON `host` for the exact model),
  GPU: NVIDIA GeForce RTX 4060 Ti, Windows 11.

## Results (2026-10-30)

| Client | Scene | FPS avg | Frame p95 | Map load | Peak RSS | CPU avg | Threads |
|---|---|---|---|---|---|---|---|
| OpenKF | solo orbit | 670 | 2.1 ms | 0.29 s | 1239 MB | 13% | 30 |
| OpenKF | live, 155 actors | 103 | ~10 ms | 0.29 s* | 2244 MB | 2% | 34 |
| Retail | solo | manual† | n/a | 3.29 s | 682 MB | 6% | 43 |

\* Live scene load equals solo load (same map); the 25 s join/wait is gameplay,
not loading.

† Retail FPS is not automated: open the console in-game (`stat fps`) on the
same map and spot. Automating it needs frame-capture tooling (PresentMon/FRAPS);
contributions welcome. Retail is a 32-bit D3D9 client; OpenKF is 64-bit Vulkan.

## Live FPS variance

Live FPS depends on replicated actor count and server state: 211 FPS at 1 actor
(join), ~100-190 FPS at 150+ actors (wave running). The table reports a loaded
wave (155 actors). Solo orbit is stable at 600-670 FPS with cooldowns.

## Findings

- Map load: OpenKF is ~11x faster (0.29 s vs 3.29 s) — parallel package IO,
  no menu/entry levels, no shader precompilation stalls.
- RAM: OpenKF uses more (1.2 GB solo, 2.2 GB live vs 0.7 GB retail). Retail
  streams and shares more aggressively; OpenKF keeps decoded textures/meshes
  resident. Optimization opportunity, not a correctness issue.
- CPU: OpenKF spreads load over threads (rayon skinning, net/audio threads);
  retail leans on fewer cores. Both stay well under saturation on this host.
- Threads: retail spawns more OS threads (43-47, engine subsystems); OpenKF
  runs 30-34 (render, net, audio, skinning pool).

## Reproduce

```powershell
powershell -ExecutionPolicy Bypass -File scripts/bench.ps1 -Frames 120 -LiveSeconds 25
```

Requires the retail game (for assets + retail comparison) and a Rust toolchain.
Set `KF_DIR` if the game is not under the default Steam path.
