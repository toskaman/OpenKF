# Project status

Last update: 2026-10-30 (v1.8.0).

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
| Particles, decals, Karma ragdolls | `openkf-viewer::particles`, `ragdoll` |
| Server browser (favorites / recents / LAN / direct, live details) | `--browse`, `openkf-viewer::browser` |
| Headless benchmarks + PresentMon presented FPS | `--bench`, `scripts/bench.ps1` |

## Verified numbers (v1.8.0)

- Live, KF-WestLondon, 25 s: 162 actors, 7 pawns, 2 kills, 50 sounds with
  0 failures, decode failures 0.
- Custom sounds: 2839/2839 decode across all custom packages, 0 failures
  (`sound_sweep` example through the exact live chain).
- Public modded server: joins as spectator, custom map renders; rejoin
  downloads nothing (all GUIDs cached).
- Channel download of 4 KB / 15 KB / 1.2 MB from a retail server, GUID-checked.
- Full workspace suite green; release build clean.
- Benchmarks: solo 651 FPS / 0.39 s load / 835 MB; live 201 FPS at
  162 actors / 1084 MB; retail 3.26 s load / 596 MB / 5% CPU; presented
  FPS 313 (OpenKF live) vs 120 (retail solo cap) — see `docs/BENCHMARKS.md`.

## Known limits

- No internet server list: the UDP masters are dead (NXDOMAIN, no A2S) —
  needs Steamworks `ISteamMatchmakingServers` or the Steam Web API list.
  Direct queries (GameSpy `\status\`) and LAN discovery work.
- Channel downloads are throttled by the server (~14 KB/s at `NETSPEED 25000`);
  big files take minutes. A `NETSPEED` boost was measured to have no effect
  (throttle is per-tick, not rate-based); retail-exact 25000 is kept.
- Single-body ragdolls (no articulation); decals are camera-facing quads.
