# Handoff (2026-10-30, evening — v1.7.0 done)

## Done this session

- **ChannelDownload** (`Engine.ChannelDownload`, `CHTYPE_FILE=3`): GUID-keyed
  file channels, verified against a retail server (4 KB / 15 KB / 1.2 MB,
  GUID-checked). Stall-based timeout (30 s without bytes); the server throttles
  to ~14 KB/s at `NETSPEED 25000`. High channel range (1000+) avoids actor
  collisions. Maps missing from the HTTP redirect now load.
- **Sound hardening**: 256-byte RIFF search, nested SoundGroup flattening,
  typed failures, no failure caching. 17,916 retail/cached sounds parse; live
  90 sounds with 0 failures.
- **Background downloads + progress bar**: FastDL on a worker thread
  (header-first streaming with progress callbacks), channel downloads already
  async; unified `DL HTTP/CHAN file pct bar LEFT` in HUD + console.
- **Particles/decals**: 2048 pool, billboard Vulkan pass, blood/muzzle/smoke/
  sticky decals; 77 live in the combat screenshot.
- **Ragdolls**: single-body Karma translation + tip-over physics for corpses
  without death anims; legacy timed fall-over kept as fallback.
- **Server browser**: `--browse` (favorites/recents/direct, `fav`/`unfav`),
  recents auto-saved on join. Matchmaking code moved to
  `openkf-network::browser`. No internet list (UDP masters dead, no usable
  query protocol) — Steamworks is the documented path.
- **Benchmarks**: `--bench N` orbit, `scripts/bench.ps1`, `docs/BENCHMARKS.md`
  with real numbers (solo 670 FPS, live 103 FPS @155 actors, retail 3.29 s).
- Release build: zero warnings. Full workspace suite: green.

## Suggested next steps

1. Public modded server re-test (FastDL bulk + channel for redirect misses);
   confirm the old ~25% sound failures are gone with all packages present.
2. Steamworks `ISteamMatchmakingServers` for a real internet server list.
3. Faster channel downloads: raise `NETSPEED` during downloads, then restore.
4. RAM: stream/share decoded textures instead of keeping everything resident.
5. Retail FPS automation (PresentMon) for a complete FPS comparison.
6. Articulated ragdolls (multi-body), projected decals, 3D browser UI.

## Notes

- `scripts/bench.ps1` uses `KF_DIR` or the default Steam path; 20 s cooldowns
  between runs (thermals move FPS up to 2x).
- Retail load marker: `Bringing Level <Map>.myLevel up for play ... appSeconds:`.
- Old UDP masters (`hl2master.steampowered.com`, GameSpy) are NXDOMAIN/dead;
  KF1 dedicated servers answer neither A2S nor GameSpy usefully.
