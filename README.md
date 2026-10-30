<div align="center">

# OpenKF

**A modern, native 64-bit client for Killing Floor (v1065), written in Rust.**

Reads the retail game files directly, speaks the original Unreal Engine 2.5 network protocol,
authenticates through Steam and renders with Vulkan (wgpu), so it can join standard
retail servers.

`Rust 2021` · `Windows 10/11 x64` · `Vulkan via wgpu 22` · `GPL-3.0-or-later`

</div>

---

## Why a client rework?

The retail Killing Floor client is a 32-bit, single-threaded Unreal Engine 2.5 executable from
2009. It hits the 2 GB address-space limit on heavy custom maps, crashes on corrupted effect
arrays (`FArray 0*2` GPFs) and uses only one CPU core to load maps and skin characters.

OpenKF rebuilds the client side from scratch as a set of focused Rust crates:

| | Retail client | OpenKF |
|---|---|---|
| Architecture | 32-bit x86 | Native 64-bit (no 2 GB ceiling) |
| Renderer | Direct3D 9 | Vulkan through wgpu, BC textures uploaded compressed, 16x AF |
| CPU usage | Single thread | Multi-threaded map baking, texture decoding and skinning (rayon) |
| Asset loading | Engine `ULinker` | Memory-mapped zero-copy package reader with hard allocation caps |
| Malformed data | GPF / crash to desktop | Bounds-checked readers, capped arrays, fallback materials |
| Network | `UNetDriver` | Bit-exact re-implementation of UE2.5 packets, bunches and replication |

> **No game content is included.** OpenKF loads assets from your own Steam installation of
> Killing Floor. It is an independent, non-commercial project and is not affiliated with or
> endorsed by Tripwire Interactive.

---

## What's new in the rework

### Live world: pickups and projectiles
Replicated movable static-mesh actors are now drawn in the world:

- Weapon pickups, ammo boxes, armour and every other `Pickup` subclass drawn with
  `DrawType=DT_StaticMesh`.
- Static-mesh projectiles (frag grenades, pipe bombs, rockets, bolts...), oriented along their
  velocity when the server does not replicate a rotation.
- The pool is built from the game's own class defaults at load time (95 meshes / 303 slots on
  retail data), uses `DrawScale`, `DrawScale3D` and `PrePivot`, honours a replicated
  `StaticMesh`, and hides sleeping pickups (`bHidden`).
- Hidden pool slots are neither re-transformed nor drawn.

### Character animation
- **Direction-aware locomotion**: `MovementAnims(0..3)` (forward / back / left / right),
  `WalkAnims` and `CrouchAnims` picked from the velocity relative to the pawn's yaw,
  plus `IdleRestAnim` / `IdleCrouchAnim`.
- **One-shot actions** from the replicated `Pawn.AnimAction`: zed attacks (`Claw`, `GoreAttack1`,
  `FPRageAttack`...), hit reactions, door bashing, weapon reloads and throws on players.
- **Deaths**: dying pawns are kept for six seconds. A death sequence plays when the mesh has
  one; otherwise (retail uses Karma ragdolls) the body tips over onto the floor.
- Held weapons follow their owner's current one-shot animation pose.

### Quality of life
- English HUD: `HP | AMMO | GRENADES | KILLS | WAVE | POUNDS`.
- On public servers the automated test player is disabled: without `--play` the client joins as
  a passive spectator. The test player only runs on loopback / LAN servers.
- `OPENKF_LIST_SEQS=1` prints every animation sequence of the loaded meshes.

See [CHANGELOG.md](CHANGELOG.md) for the full history.

---

## Feature overview

| Area | Status |
|---|---|
| Map rendering: BSP, static meshes, terrain layers, lightmaps, sky zones | ✅ |
| UE2 materials: Shader, FinalBlend, Combiner, modifiers; opaque / masked / alpha / additive / modulate | ✅ |
| UDP protocol: packets, bunches, acks, reliability, channel reuse | ✅ bit-exact against a reference capture |
| Handshake: HELLO, Steam, CHALLENGE / AUTH / LOGIN, USES, WELCOME, JOIN | ✅ |
| Steam authentication (`steamclient64`, `InitiateGameConnection`) | ✅ |
| Actor replication: package map, class net cache, properties, RPCs | ✅ 0 decode failures on live sessions |
| Zeds, players and held weapons, skinned on the CPU in parallel | ✅ |
| Locomotion, AnimAction one-shots, deaths | ✅ new |
| Pickups and static-mesh projectiles | ✅ new |
| First-person weapons with fire / reload animations, `DisplayFOV` scaling | ✅ |
| Play mode: move, aim, fire, reload, weapon slots, grenades | ✅ |
| Trader purchases (`ServerBuyWeapon`, `ServerBuyAmmo`) | ✅ validated end-to-end |
| HUD and crosshair | ✅ |
| 3D sound from `ClientHearSound`, SoundGroups, local weapon sounds | ✅ |
| Server travel and automatic reconnect | ✅ |
| FastDL / UFileChannel download manager (256 MB per-file cap) | ✅ |
| Particle emitters, decals, projectors | ⏳ planned |
| True ragdoll physics | ⏳ fall-over fallback for now |
| In-viewer server browser | ⏳ master-server code exists in `openkf-app`, not wired to the viewer yet |

---

## Repository layout

```
crates/
  openkf-package    UE2 package reader: names, imports/exports, properties, textures,
                    static & skeletal meshes, animations, sounds, terrain, navigation,
                    .uc defaultproperties
  openkf-network    UE2.5 net protocol: wire format, control channel, handshake, Steam,
                    replication schema, property / RPC codec, live session, FastDL
  openkf-viewer     The playable client: wgpu renderer, live view, HUD, audio,
                    skinned actors, pickups / projectiles, replay of captures
  openkf-render     Render HAL: PSO cache, vertex formats, culling, canvas, fonts
  openkf-bsp        BSP model parsing, triangulation, parallel GPU mesh building
  openkf-mesh       Static mesh LODs, kDOP-18/26, GPU instancing structures
  openkf-animation  Skeleton, keyframe sampling (slerp), skinning
  openkf-audio      WAV decoding, 3D attenuation / panning, voice pool, HRTF mixer
  openkf-physics    BSP / kDOP collision, ray casts
  openkf-world      ECS world (hecs) and level loading
  openkf-vm         UnrealScript bytecode interpreter (experimental)
  openkf-app        OpenKF launcher / supervisor for the retail game (DXVK and INI tuning)
  openkf-netcap     UDP relay that records client <-> server traffic for protocol work
docs/               Protocol notes and project status
scripts/            Local test server and automated live test
```

---

## Getting started

### Requirements
- Windows 10 or 11, 64-bit, with a Vulkan-capable GPU.
- [Rust](https://rustup.rs) stable (MSVC or GNU toolchain).
- Killing Floor (Steam, retail build 1065) installed.
- Steam running and logged in for online play.

### Build

```powershell
git clone https://github.com/toskaman/OpenKF.git
cd OpenKF
cargo build --release -p openkf-viewer
```

> With the GNU toolchain, put scoop's `gcc\current\bin` first on `PATH`
> (the mingw-winlibs `dlltool` fails with `invalid bfd target`).

### Run

```powershell
# Walk around a map (name or full path to a .rom)
target\release\openkf-viewer.exe KF-WestLondon --kf "D:\Steam\steamapps\common\KillingFloor"

# Join a server and play in first person
target\release\openkf-viewer.exe --connect 203.0.113.10:7707 --play --name MyName

# Join as a spectator
target\release\openkf-viewer.exe --connect 203.0.113.10:7707 --no-ready
```

| Option | Meaning |
|---|---|
| `--kf <dir>` | Killing Floor installation folder |
| `--connect <ip:port>` | Join a server |
| `--play` | First-person play mode |
| `--no-ready` | Stay in the lobby as a spectator |
| `--name <nick>` | Player name (default `OpenKF`) |
| `--size 2560x1440` | Window size |
| `--screenshot out.png --time N` | Headless capture after N seconds |
| `--replay <capture.txt>` | Replay a recorded network session |

**Play controls:** WASD / ZQSD move · mouse aim · left click fire · `R` reload ·
`1`-`5` knife / 9mm / grenade / syringe / welder · `G` throw grenade · `B` buy a magazine at the trader.

### Tests

```powershell
cargo test --workspace
```

Tests that need retail files look for the game in its default Steam location and skip when
the files are missing. `scripts\live_test.cmd` starts a local dedicated server, connects the
client for a set time and stores logs and a screenshot in `output\live\`.

---

## Benchmarks

A side-by-side comparison against the retail client (RAM, CPU usage, cores / threads used,
map load time, solo and multiplayer FPS) is in preparation and will be published here with the
measurement method and raw data.

---

## Contributing

Issues and pull requests are welcome. Please run `cargo fmt`, `cargo clippy` and
`cargo test --workspace` before submitting, and never commit network captures, logs or
anything produced by your Steam client: they contain authentication tickets.

## License

OpenKF is released under the GNU General Public License v3.0 or later. See [LICENSE](LICENSE).
Killing Floor and all of its assets are the property of Tripwire Interactive.
