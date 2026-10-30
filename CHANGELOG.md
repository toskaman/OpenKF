# Changelog

All notable changes to OpenKF. Dates are 2026.

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
