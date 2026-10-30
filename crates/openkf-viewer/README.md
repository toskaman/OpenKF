# openkf-viewer

Killing Floor (v1065) map viewer in Rust, Vulkan rendering via wgpu.
Reads installed game files directly: `.rom`, `.usx`, `.utx`.

## Run (Windows, PowerShell)

```
cd OpenKF
cargo run --release -p openkf-viewer -- KF-WestLondon
```

- Name alone is enough (`KF-Farm`, `KF-BioticsLab`…), or a full path to a `.rom`.
- `--kf <folder>`: game folder (default `E:\SteamLibrary\steamapps\common\KillingFloor`).
- `--size 2560x1440`: window size.
- `--screenshot out.png [--cam x,y,z,yaw,pitch]`: headless render, saved as PNG
  (Unreal coordinates, angles in degrees).

## Live connection (spectator)

```
cargo run --release -p openkf-viewer -- --connect 127.0.0.1:7717
```

Connects to a KF v1065 server with the Rust client (`openkf-network::live`), loads the map
announced by the server and displays replicated Pawns (zeds, players) live.
Options: `--auth steam` (default: identity and ticket from running Steam client)
or `--auth <capture>` (identity from retail client capture), `--name <nick>` (default OpenKF),
`--no-ready` (don't mark ready: `ServerSetReadyToStart` + `ServerRestartPlayer`).
Audio: server-sent `ClientHearSound` played in 3D around player (Windows).
HUD rendered inline (embedded 5x7 bitmap font, `src/hud.rs`): HP, 9mm, grenades, kills,
wave, and crosshair in play mode. Keys in play mode: 1-5 weapons (knife, 9mm, grenade, syringe,
welder), G grenade, B mag for held weapon, N shotgun (at trader).
**Play mode**: `--play` → first-person on our Pawn, WASD to move (`ServerMove`),
mouse to aim, left click to fire (9mm), R to reload.
Without `--play`, a test bot kites the nearest zed and shoots it (3 kills / 6 shots measured).
With `--screenshot out.png --time N`: stay connected N seconds then render image (auto-test).
Windows build (GNU toolchain): put `scoop\apps\gcc\current\bin` first on PATH, mingw-winlibs
`dlltool` fails with `invalid bfd target`.

## Network replay (spectator)

```
cargo run --release -p openkf-viewer -- --replay captures\capture_1792954639.txt [--speed 2]
```

Replays an `openkf-netcap` capture: map from capture `WELCOME`, each Pawn
(zeds and player) rebuilt from decoded server packets via `openkf-network`
(`PawnPosition` for zeds, `ClientAdjustPosition` for local player), interpolated, and
animated (idle / walk by speed).
Keys: P pause, R restart, F follow player, +/- speed.
Screenshot: `--screenshot out.png --time <seconds since replay start>`.

## Controls

| Key | Action |
|---|---|
| Left click | capture mouse |
| WASD / arrows | move (physical keys, AZERTY or QWERTY) |
| Mouse | look |
| Shift | sprint |
| Space / C or Ctrl | up / down |
| Esc | release mouse, then quit |

Window title shows FPS, frame time, draw calls and triangle count.

## What is rendered

- Level BSP with textures and UVs.
- All StaticMeshActor (and subclasses) with `Skins` overrides.
- UE2 materials: Texture, Shader, FinalBlend, Combiner, modifiers. Opaque, masked,
  alpha, additive and modulate modes.
- Sky zone, view through `PF_FakeBackdrop` surfaces.
- Baked lighting: BSP lightmaps (DXT1/DXT5, 39 of 42 maps) and per-vertex colors
  of props (`StaticMeshInstance`) with UE2 x2 modulation.
- Terrains (`TerrainInfo`): G16 heightmap and layer blending via alpha maps.
- Animated specimens: 9 zombies from `KF_Freaks_Trip.ukx`, lined up at spawn
  playing Idle. Parallel CPU skinning, one core per zombie (rayon).
  `--no-zeds` to remove, `--time <s>` to fix animation time in capture mode.
- DXT textures sent compressed to GPU (BC1/2/3), 16x anisotropic filtering.
- Parallel loading (rayon): mesh assembly and texture decoding.
- Batched rendering: one draw call per texture/mode pair (~100-250 per map).

## Not yet handled

- Lightmaps for KF-Clandestine, KF-Forgotten and KF-Hell, stored differently (approx lighting).
- Terrain lighting.
- Particle emitters, decals, projectors.
- Zombie AI and movement (they only play animation in place).
- Culling (frustum, zones): everything is drawn every frame.

`OPENKF_DEBUG=1` prints material and BSP diagnostics.
