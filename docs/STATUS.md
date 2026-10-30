# Project status

Last update: 2026-10-30 (v1.7.0).

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
| Server browser (favorites / recents / direct) | `--browse`, `openkf-viewer::browser` |
| Headless benchmarks | `--bench`, `scripts/bench.ps1` |

## Verified numbers (v1.7.0)

- Live, KF-WestLondon, 30 s: 154 actors, 10 pawns, 3 kills, 90 sounds with
  0 failures, 77 particles, 0 bunch decode failures.
- Channel download of 4 KB / 15 KB / 1.2 MB from a retail server, GUID-checked.
- Full workspace suite green; release build with zero warnings.
- Benchmarks: solo 670 FPS / 0.29 s load; live 103 FPS at 155 actors; retail
  3.29 s load / 682 MB / 6% CPU (see `docs/BENCHMARKS.md`).

## Known limits

- Retail FPS has no automated capture yet (manual `stat fps`); see benchmarks doc.
- No internet server list: the UDP masters are gone and KF1 answers no usable
  query protocol — needs Steamworks `ISteamMatchmakingServers`.
- Channel downloads are throttled by the server (~14 KB/s at `NETSPEED 25000`);
  big maps take minutes. Higher `NETSPEED` during downloads is untried.
- RAM is higher than retail (decoded assets stay resident).
- Single-body ragdolls (no articulation); decals are camera-facing quads.
