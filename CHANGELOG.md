# Changelog

All notable changes to OpenKF. Dates are 2026.

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
