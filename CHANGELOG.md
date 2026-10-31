# Changelog

All notable changes to OpenKF. Dates are 2026.

## v1.10.0 — Texture-artifact fixes, browser cache + details pane, lower RAM (October 31)

### Fixed
- **BSP floors/walls no longer vanish**: section vertex order varies per
  node (additive vs subtractive brushes) and some fans start with
  degenerate triangles, so whole streets were backface-culled and the sky
  showed through (the "white floor" reports). Fan winding is now aligned
  per node with the surf normal using Newell's method. Verified on
  KF-WestLondon, KF-Manor and KF-Offices; screenshots refreshed.
- **Mirrored static meshes no longer see-through**: placements with a
  negative-determinant scale (the mapper mirrors tunnel shells with
  `DrawScale3D.y < 0`) kept their authored winding, so every face was
  backface-culled from inside — the WestLondon road tunnel rendered as a
  black void. Winding is now reversed for mirrored placements (the
  existing `is_mirrored` helper, previously unused). The tunnel interior
  renders with brick walls.
- **Stale black vertex lighting no longer renders pitch black**: some
  `StaticMeshInstance` color streams contain exactly-zero verts (lighting
  never rebuilt for those faces — e.g. the church end caps in
  KF-WestLondon, seen as a floating "black quad" against the sky). The
  vertex-color shading branch is now floored at 45% of the approximate
  (hemispheric + key) shading: deep shadow stays dark but textured.
- **Fleshpound chest renders its glow**: all skeletal-mesh sections of
  an actor shared the single opaque pipeline, so the amber chest
  capacitor (`FinalBlend`, brighten) drew as a black square. Sections now
  each use their own pipeline (opaque / alpha / additive / modulate).
- **Bounded sound cache**: decoded sounds are capped at 1024 entries with
  FIFO eviction of the oldest half (re-decode on demand), instead of
  growing unbounded in long sessions.
- **Lower steady-state RAM**: CPU-side texture/lightmap mips and (outside
  `--pick`) batch vertices are released after GPU upload (~165 MB saved
  on KF-WestLondon).
- **FPS cap**: `--fps N` (default 120, `0` = uncapped) caps the live loop,
  cutting CPU usage roughly in half at the same perceived smoothness.

### Added
- `--pick px,py` diagnostic: raycasts through a screen pixel and prints
  the batch stack (blend, texture, lightmap, UVs). Used to find the
  culled street batches.
- More `OPENKF_DEBUG` scene diagnostics (node paths, texture index table,
  non-opaque surfs, translucent mesh sections, floor winding).
- Pick now reports per-hit facing (front/back vs the rendered side) and
  the hit triangle's vertex lights + UVs (`OPENKF_DEBUG`).
- `OPENKF_HIDE_TEX=<substr>` / `OPENKF_MAGENTA_TEX=<substr>` bake-time
  debug hooks to isolate a texture's on-screen contribution.
- `static_dump` package example: lists static mesh sections (material,
  UV/position bounds) for mesh triage.
- `is_mirrored` unit tests + retail test asserting WestLondon's mirrored
  tunnel shells.
- Internet server list cache: Steam results + GameSpy details are reused
  for 10 minutes (`netcache.txt`); `net!` forces a refresh, and a stale
  cache still serves when no API key is configured.
- F2 browser details pane: the footer shows the selected server's
  gametype, map title and version once querying finishes.
- Benchmark split timers: `[Bench]` now reports skin / upload /
  render+wait ms separately (live .240: 1.6 / 1.6 / 4.1 ms at 99 actors).
- README documents the Steam API key setup for the internet list.

## v1.9.0 — Articulated ragdolls, decals, in-game browser, internet list (October 30)

### Added
- **Articulated ragdolls**: corpses are now multi-body (64 bones, joint
  limits, spring-damper) — limbs flail on death then settle, instead of a
  single rigid fall-over. Unit-tested (flail/settle, joint limits).
- **Projected decals**: blood and impact decals align to the hit surface,
  expire over time, and recycle under a budget (oldest first).
- **In-viewport server browser (F2)**: the server list now renders inside
  the 3D viewport (6 rows, up/down cursor, rescan, join) — no more
  console-only `--browse`.
- **Internet server list**: TLS client (`ureq` + native TLS) plus the Steam
  Web API `IGameServersService/GetServerList` feed real public servers
  into the browser (needs an `OPENKF_STEAM_API_KEY` key; the 403
  invalid-key path is covered live).
- **Perk auto-select fallback**: on custom `RGameType` servers where the
  client would sit pawnless in limbo, it now sends `SelectVeterancy` +
  `ServerRestartPlayer` after 10 s (retail gametypes unaffected).

### Changed
- HUD and console output are fully English (`DOSH`, `sounds (failed N)`);
  the bitmap font has no `£` glyph, so dosh shows as `DOSH 300`.
- Benchmarks re-run: solo 671 FPS / 0.14 s load / 713 MB; live 95-99 FPS
  at ~155 actors in wave combat (see below); retail 3.32 s load / 597 MB.
- Retail's 120 FPS is a **hard engine ceiling**, not a config default:
  fullscreen-exclusive 1600x900 with `MaxClientFrameRate=400` still reads
  117 cur / 120 avg `stat fps` (screenshot in `docs/screenshots/`).

### Fixed
- The benchmark script no longer hangs when a stale dedicated server
  still holds port 7717 (leftovers are killed first).

### Verified
- 251 tests green across the shipped crates (viewer 19, network 54,
  render 159, package 8, audio 8, physics 3); release build clean.
- Live wave combat: bot moves, fires, kills (2 kills), 0 sound failures,
  0 decode failures at 150-158 actors.
- Fullscreen-exclusive retail proof shot: 120 avg FPS uncapped attempt.

### Known limits
- Live orbit FPS is down from v1.8.0's 201 to ~95-99 at similar actor
  counts — the cost of per-frame articulated ragdolls, projected decals,
  and heavier combat effects. Profiling the live frame time is the next
  perf step; real play (narrower frustum than the orbit) stays above 60.
- Custom-gametype spawn still unsolved on `ServerPerks`-style servers
  (client stays pawnless even after the perk fallback); retail gametypes
  spawn normally.

## v1.8.0 — Memory, browser details, benchmark automation (October 30)

### Added
- **Server browser v2** (`--browse`): live GameSpy details (hostname, map,
  players) next to every favorite/recent, queried in parallel, plus a `lan`
  command that discovers KF1 servers on the local network via broadcast.
  KF1 servers answer `\status\` on `game + 10` (e.g. 7717) — the v1.7.0
  "no usable query protocol" note was wrong for direct queries; only the
  internet list still needs Steamworks (UDP masters verified dead: NXDOMAIN
  DNS, no A2S, timed-out master IPs).
- **`sound_sweep` example**: decodes every `Sound` export in a folder through
  the exact live chain and reports ok/fail by reason.
- **NETSPEED regression test**: scripted mock server drives
  `HELLO -> CHALLENGE -> LOGIN -> USES/DLMGR/WELCOME -> file bytes -> JOIN`
  and locks the retail-exact `NETSPEED 25000` handshake.

### Changed
- **RAM -64%**: packages are memory-mapped instead of read+leaked whole
  (only touched pages resident), and pool actors share parsed meshes and
  animations via `Arc` instead of re-parsing per slot (99 zed + 303 prop
  slots). Live peak RSS 2244 MB -> 1084 MB, steady state ~660 MB.
- **Exact-GUID lookup scans all candidates**: a stale same-named copy in the
  retail cache no longer shadows the fresh download (it used to return
  `None`, re-fetching 136 MB over the slow file channel every join).
- Retail FPS is now automated (PresentMon 2.6.0), no longer manual `stat fps`.
- `docs/BENCHMARKS.md`: new runs + a presented-FPS (PresentMon) section.

### Verified
- Custom sounds: 2839/2839 decode across all custom packages (0 failures);
  live 160 sounds with 0 failures, plus a public modded-server join.
- Rejoining the public modded server downloads nothing (all GUIDs cached).
- Benchmarks (RTX 4060 Ti): solo 651 FPS / 0.39 s load / 835 MB; live
  201 FPS at 162 actors / 1084 MB; retail 3.26 s load / 596 MB.
- Presented FPS (PresentMon, 20 s, windowed): OpenKF 313 FPS avg in a loaded
  live scene vs retail's 120 FPS engine-capped solo baseline.
- Full workspace suite green; release build clean.

## v1.7.0 — Downloads, effects, browser, benchmarks (October 30)

### Added
- **In-connection downloads (`Engine.ChannelDownload`)**: packages missing from
  the HTTP redirect (e.g. custom maps) are fetched over `CHTYPE_FILE` channels
  (GUID request, throttled stream, GUID-verified save), one file at a time with
  stall-based timeout. Verified against a retail dedicated server up to 1.2 MB.
- **Background FastDL**: HTTP downloads run on a worker thread while the
  connection stays alive (acks + keepalives); unified progress bar
  (`DL HTTP/CHAN file pct bar LEFT`) in the HUD and console.
- **Particles and decals**: CPU pool (2048) with billboard Vulkan pass —
  blood bursts, muzzle flashes, emitter smoke, sticky blood/impact decals.
- **Karma ragdolls**: corpses without a death animation collapse with rigid-body
  translation (gravity, ground bounce, friction) plus tip-over torque and yaw
  spin; the timed fall-over remains as a legacy fallback.
- **Server browser** (`--browse`): favorites + auto-saved recents + direct IP
  entry in the viewer (`fav`/`unfav` commands). The old UDP masters are gone
  and KF1 answers no usable query protocol, so there is no internet list yet —
  Steamworks `ISteamMatchmakingServers` is the documented next step. The
  matchmaking code moved from `openkf-app` to `openkf-network::browser`.
- **Benchmarks**: headless `--bench N` orbit (skinning + render timed),
  `scripts/bench.ps1` (OpenKF solo/live + retail load/RAM/CPU/threads),
  `docs/BENCHMARKS.md` with method, hardware, table, and raw data.
- **Sound hardening**: 256-byte RIFF search, nested `SoundGroup` flattening,
  typed play-failure reasons (`NoData`/`NoRiff`/`BadWav`/`Engine`), failures
  are no longer cached (a late download becomes playable).
- `--bench` works without `--screenshot`; `[SceneLoad]` timing marker.

### Changed
- `WELCOME` no longer blocks on downloads: FastDL (background) then channel
  downloads run before `JOIN` while the map announcement waits.
- Death path: authored death animations play upright in place; physics drives
  only the no-anim fallback (plus a small position settle).
- `http_get` streams the body with progress callbacks (header-first read).

### Verified
- Channel download of 4 KB / 15 KB / 1.2 MB from a retail server (GUID-checked).
- Full workspace suite green; release build with zero warnings.
- Live dedicated server, KF-WestLondon, 30 s: 154 actors, 10 pawns, 3 kills,
  90 sounds with 0 failures, 77 particles, decode failures 0.
- Benchmarks (RTX 4060 Ti): solo 670 FPS / 0.29 s load; live 103 FPS at
  155 actors; retail 3.29 s load / 682 MB / 6% CPU.

## v1.6.0 — Client rework: live world & animation (October 30)

### Added
- **Pickups and projectiles**: replicated movable static-mesh actors (`Pickup` and `Projectile`
  subclasses drawn as `DT_StaticMesh`) are rendered in the live world. The mesh pool is built
  from the game's class defaults (95 meshes / 303 slots on retail data); `DrawScale`,
  `DrawScale3D`, `PrePivot`, replicated `StaticMesh` and `bHidden` are honoured; projectiles
  face their velocity when no rotation is replicated. New module `openkf-viewer/src/props.rs`.
- **Direction-aware locomotion**: `MovementAnims(0..3)`, `WalkAnims`, `CrouchAnims`,
  `IdleRestAnim`, `IdleCrouchAnim` from class defaults, selected from velocity vs. yaw.
- **One-shot animations** from the replicated `Pawn.AnimAction` (attacks, hit reactions,
  door bashing, reloads, throws).
- **Deaths**: corpses kept 6 s; death sequence when the mesh has one, otherwise a fall-over
  fallback (retail uses Karma ragdolls).
- `OPENKF_LIST_SEQS=1` debug output listing every animation sequence of loaded meshes.
- Prop visibility counters in the live status line.

- **FastDL**: missing server packages are downloaded from the HTTP redirect before the level loads (`.uz2` decompression, GUID check), stored in `%LOCALAPPDATA%\OpenKF\Downloads`.
- `fastdl` example tool.

### Changed
- Weapons held by pawns sample their owner's current one-shot animation time.
- HUD and on-screen controls are in English.
- On non-local servers the automated test player is disabled; the client joins as a spectator
  unless `--play` is given.
- `openkf-app` build script locates `windres` from `WINDRES`, scoop or `PATH`.

### Verified
- Local dedicated server, KF-WestLondon, 150 s: 0 bunch decode failures, 0 sound failures,
  locomotion sequences resolved from class defaults (e.g. `JogF_Bullpup`).

## v1.5.11 — Render pipeline completion
- Frustum culling (6-plane extraction from view-projection) and front-to-back early-Z.
- Indirect draw structures (`DrawIndexedIndirectArgs`), lightmap atlas through array textures,
  back-face culling for opaque pipelines.

## v1.5.10 — Network stability and multi-core loading
- Exponential retransmit back-off with jitter, per-channel and global incoming buffer caps,
  voice / file channel isolation, connection health states.
- FastDL: `DLMGR` / `NEEDED` / `RedirectToURL` parsing and a download manager
  (256 MB per-file cap) fed by `CHTYPE_FILE` bunches.
- Parallel BSP GPU building (rayon fold/reduce) with a sequential fallback for small maps.
- BC texture decode fallback capped at 4096².

## v1.5.9 — Transient effect crash hardening
- Zero-stride `FArray` guard in every package reader and a per-tick transient effect budget
  (64 per tick, 512 alive), fixing the retail `ROBulletHitRockEffect` / `SpriteEmitter` GPF
  triggered by nail-gun pellet spam on modded servers.

## v1.5.8 — Mesh hardening
- Allocation caps for skeletal / static meshes (textures, bones, vertices, indices, LODs,
  sections), pink fallback material and out-of-bounds triangle skipping, fixing the retail
  `SetMaterial` / `FArray 0*2` GPF on KF-BoxTest.
- `CompactIndex` shift overflow fix.
- Optional RPC parameters (`CPF_OptionalParm`): 3952 / 3952 bunches decoded on the reference
  capture.

## Earlier
- Trader purchases validated end-to-end (`ServerBuyWeapon`, `ServerBuyAmmo`).
- Fix: incoming reliable sequence numbers persist per channel index for the whole connection
  (UE2 `InReliable`), which previously stalled every actor opened on a reused channel.
- First-person weapons (`PlayerViewOffset`, `DisplayFOV`, fire / reload animations).
- Long sessions: 10 minutes, several consecutive matches with automatic reconnect.
- 3D sound, HUD, crosshair, server travel, Steam authentication, actor replication,
  map rendering.
