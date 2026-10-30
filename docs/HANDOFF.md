# Handoff (2026-10-30, night — v1.8.0 done)

## Done this session (v1.8.0)

- **RAM -64%**: packages memory-mapped (`AssetLibrary::load_file`, only
  touched pages resident) + pool actors share parsed meshes/animations via
  `Arc` (`zeds`/`props` `spawn_pool` parse once). Measured debug live:
  steady 1828 MB -> 657 MB; release bench peaks: solo 835 MB, live 1084 MB.
- **Server browser v2**: GameSpy `\status\` details for saved servers
  (`openkf-network::browser`, mock-tested) + `lan` broadcast discovery in
  `pick()`. Verified live against local UCC and a public server.
- **GUID shadow fix**: `find_package_by_guid` scans every same-named
  candidate instead of giving up on the first mismatch — a stale retail
  `Cache/*.uxx` no longer forces a full channel re-download each join.
  Regression tests (`select_by_guid`); verified by public-server rejoin
  with zero downloads.
- **NETSPEED experiment**: live A/B proved a `NETSPEED 100000` boost has no
  effect (server throttle is per-tick); reverted to retail-exact 25000 with
  a mock-server regression test.
- **PresentMon benchmarks**: retail 120 FPS (engine cap) + OpenKF 313 FPS
  presented, both 20 s windowed captures; `docs/BENCHMARKS.md` rewritten.
- **Custom sounds closed out**: `sound_sweep` decodes all 2839 custom sounds
  with 0 failures; the old ~25% failure report is resolved.
- Public modded server (ZLOY): spectator join OK, custom map renders; note
  the server ignores immediate re-HELLOs for a few minutes after a client
  vanishes mid-download (server-side session state).

## Suggested next steps (updated)

1. Internet server list via Steamworks `ISteamMatchmakingServers` (or Steam
   Web API `IGameServersService/GetServerList` + user key; needs a TLS
   client first — the tree is plain-HTTP only today).
2. RGameType-style custom gametypes: spawn/ready flow (the test bot gets no
   pawn there; retail gametypes work).
3. Articulated ragdolls (multi-body), projected decals, 3D browser UI.
4. Uncapped-retail FPS ceiling (PresentMon wrote no rows for D3D9 in the
   uncapped attempt; capped 120 stands as the reproducible baseline).

---

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
  KF1 answers no A2S. Correction (v1.8.0): KF1 dedicated servers DO answer
  GameSpy `\status\` on `game + 10` — only the internet list is missing.
