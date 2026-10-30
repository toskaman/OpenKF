# Project status

Last update: 2026-10-30 (v1.9.0).

## Working (tested against a local KF v1065 dedicated server)

| Domain | Location |
|---|---|
| Map rendering (BSP, static meshes, terrain, lightmaps, sky zone) | `openkf-viewer` |
| UE2 UDP protocol, bit-exact against a reference capture | `openkf-network::wire`, `live` |
| Handshake and Steam authentication | `handshake`, `live`, `steam` |
| Actor replication (package map, class net cache, properties, RPCs) | `schema`, `props`, `session` |
| Class defaults from the game's `.uc` sources | `openkf-package::uc_defaults` |
| Live zeds, players, held weapons | `openkf-viewer::live_view`, `zeds` |
| Pickups and static-mesh projectiles | `openkf-viewer::props` |
| Locomotion, AnimAction one-shots, deaths | `openkf-viewer::zeds::apply_pawn_anim` |
| Play mode: movement, aiming, firing, reloading, weapon slots, grenades | `--play` |
| Trader purchases | `ServerBuyWeapon` / `ServerBuyAmmo` |
| HUD and crosshair | `openkf-viewer::hud` |
| 3D sound (nested SoundGroups, typed failures, no failure caching) | `openkf-viewer::audio`, `openkf-audio` |
| FastDL + retail cache + ChannelDownload, background with progress bar | `openkf-network::download`, `live` |
| Particles, projected decals, articulated Karma ragdolls | `openkf-viewer::particles`, `ragdoll` |
| Server browser (favorites / recents / LAN / direct, live details) | `--browse`, `openkf-viewer::browser` |
| In-viewport browser overlay (F2) | `openkf-viewer::browser_ui` |
| Internet server list (Steam Web API, needs API key) | `openkf-network::browser`, TLS `ureq` |
| Perk auto-select fallback on custom gametypes | `live_view` (10 s pawnless gate) |
| Headless benchmarks + PresentMon presented FPS | `--bench`, `scripts/bench.ps1` |

## Verified numbers (v1.9.0)

- Live, KF-WestLondon, 25 s: 150-158 actors, 6-9 pawns, 2 kills, 40-47
  sounds with 0 failures, decode failures 0.
- Custom sounds: 2839/2839 decode across all custom packages, 0 failures
  (`sound_sweep` example through the exact live chain).
- Public modded server: joins as spectator, custom map renders; rejoin
  downloads nothing (all GUIDs cached).
- Channel download of 4 KB / 15 KB / 1.2 MB from a retail server, GUID-checked.
- 251 tests green across the shipped crates; release build clean.
- Benchmarks: solo 671 FPS / 0.14 s load / 713 MB; live 95-99 FPS at
  ~155 actors / 1085 MB; retail 3.32 s load / 597 MB / 5% CPU; presented
  FPS 236 (OpenKF live) vs 120 (retail hard cap) — see `docs/BENCHMARKS.md`.
- Stability: 10/10 consecutive windowed live joins clean (35 s each).

## Known limits

- Internet list needs a Steam Web API key (`OPENKF_STEAM_API_KEY`); the UDP
  masters are dead (NXDOMAIN, no A2S). Direct queries (GameSpy `\status\`)
  and LAN discovery work without a key.
- Channel downloads are throttled by the server (~14 KB/s at `NETSPEED 25000`);
  big files take minutes. A `NETSPEED` boost was measured to have no effect
  (throttle is per-tick, not rate-based); retail-exact 25000 is kept.
- No spawn on `ServerPerks`-style custom gametypes (pawnless limbo even
  after the perk fallback); retail gametypes spawn normally.
- Live orbit FPS regressed to ~95-99 (v1.8.0: 201) under the new
  per-frame effects; profiling is the next perf step.
