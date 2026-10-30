# OpenKF — native 64-bit Killing Floor client

OpenKF is a clean-room, native 64-bit client for Killing Floor v1065 (Unreal
Engine 2.5): it joins real servers, downloads custom content, and renders the
live match with a modern Vulkan renderer. No retail binaries are reused — only
your legally installed game data (maps, meshes, textures, sounds).

![Solo: KF-WestLondon](docs/screenshots/shot-solo.png)
![Live: wave combat on a dedicated server](docs/screenshots/shot-live.png)

## Highlights (v1.7.0)

- **Live multiplayer**: handshake, Steam auth, actor replication, RPCs, movement,
  firing, reloading, grenades, trader purchases, server travel, auto-reconnect.
- **Content downloads**: FastDL (HTTP redirect, `.uz2`), the retail download
  cache (`Cache/Cache.ini`), and in-connection `ChannelDownload` (`CHTYPE_FILE`)
  for maps missing from the redirect — all in the background with a progress bar.
- **Combat effects**: particle bursts (blood, muzzle flash, smoke), sticky
  decals, Karma ragdoll corpses.
- **Sound**: 3D positional audio, nested `SoundGroup`s, zero failures on retail data.
- **Server browser**: favorites + recents + direct IP (`--browse`).
- **Benchmarks**: headless `--bench` orbit; retail vs OpenKF numbers below.

## Performance (real numbers)

Same map (`KF-WestLondon`), 1280x720, 360° orbit. RTX 4060 Ti, Windows 11.
Method + raw data: [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

| Client | Scene | FPS | Map load | Peak RAM | CPU | Threads |
|---|---|---|---|---|---|---|
| OpenKF | solo | 670 | 0.29 s | 1239 MB | 13% | 30 |
| OpenKF | live, 155 actors | 103 | 0.29 s | 2244 MB | 2% | 34 |
| Retail v1065 | solo | manual (`stat fps`) | 3.29 s | 682 MB | 6% | 43 |

OpenKF loads maps ~11x faster; it currently uses more RAM (decoded assets stay
resident) — see the benchmarks doc for findings and variance notes.

## Requirements

- Windows 10/11 64-bit, Vulkan-capable GPU.
- Killing Floor (retail, v1065) installed — Steam default:
  `C:\Program Files (x86)\Steam\steamapps\common\KillingFloor`.
  Override with `--kf <dir>` or the `KF_DIR` environment variable.
- Rust stable (to build).

## Build

```powershell
cargo build --release -p openkf-viewer
# -> target\release\openkf-viewer.exe
```

## Use

```powershell
# Solo: render a map
openkf-viewer KF-WestLondon --kf "C:\...\KillingFloor"

# Join a server (IP from the browser, a friend, or a listing)
openkf-viewer --connect 127.0.0.1:7717 --play

# Server browser (favorites / recents / direct)
openkf-viewer --browse

# Headless screenshot + benchmark
openkf-viewer --connect 127.0.0.1:7717 --screenshot shot.png --time 30
openkf-viewer KF-WestLondon --bench 300 --size 1280x720
```

Downloads (FastDL / channel / retail cache) land in
`%LOCALAPPDATA%\OpenKF\Downloads` — never inside the game install.

## Layout

| Crate | Role |
|---|---|
| `openkf-viewer` | Vulkan client: scene, actors, HUD, audio, particles, ragdolls, browser |
| `openkf-network` | UE2 protocol: handshake, replication, downloads, browser queries |
| `openkf-package` | `.u/.utx/.uax/.rom` readers (meshes, textures, sounds, BSP, anims) |
| `openkf-physics` | Collision + Karma rigid-body runtime |
| `openkf-audio` | 3D mixer (HRTF-capable), WAV decode |
| `openkf-netcap` | UDP relay capture tool (protocol research) |
| `openkf-app`, `openkf-world`, … | Engine pillars (VM, world sim, BSP, animation, mesh, render) |

Docs: [HANDOFF](docs/HANDOFF.md) (state + next steps),
[STATUS](docs/STATUS.md) (feature matrix),
[network protocol](docs/network_protocol.md),
[BENCHMARKS](docs/BENCHMARKS.md).

## Scripts

- `scripts/live_test.cmd` — local server + viewer smoke test.
- `scripts/start_test_server.ps1` — local dedicated server (port 7717).
- `scripts/bench.ps1` — full retail-vs-OpenKF benchmark (see above).

## Release

Each release ships `openkf-viewer.exe` (see
[CHANGELOG](CHANGELOG.md)). Checksums are published with the release notes.
No installer: the exe reads the game path from `--kf` / `KF_DIR`.

## Legal

Clean-room reimplementation for interoperability. You need a licensed copy of
Killing Floor; no game content is redistributed. GPL-3.0-or-later, see LICENSE.
