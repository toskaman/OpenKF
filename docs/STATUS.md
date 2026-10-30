# Project status

Last update: 2026-10-30 (v1.6.0).

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
| 3D sound | `openkf-viewer::audio`, `openkf-audio` |
| Server travel and reconnect, FastDL | `openkf-network::live`, `download` |

## Known gaps

- Particle emitters, decals and projectors.
- Karma ragdolls (a fall-over fallback is used for corpses without a death sequence).
- Server browser in the client (master-server query code lives in `openkf-app`).
- Benchmarks against the retail client (RAM, CPU, threads, load times, FPS) are being prepared.

## Testing

- `cargo test --workspace` — unit and integration tests; retail-data tests skip when the game
  is not installed, the capture round-trip test skips without a local capture.
- `scripts\live_test.cmd` — local server + live client + screenshot.
