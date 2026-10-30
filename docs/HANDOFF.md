# Handoff (2026-10-30, night — v1.10.0 in progress)

## Texture fix (white floors): root cause + fix

- Symptom: near street in WestLondon rendered white (sky showing through);
  also black patches. Textures were fine (0/2302 BSP polys missing).
- Diagnosis path: albedo-only render still white -> UV-grid viz ->
  no-sky render proved a real hole -> `--pick` raycast found the street
  batch behind a transparent `voidtex` plane -> no-cull render showed the
  street -> front-cull isolated winding. The fan triangulation assumed one
  vertex order; order varies per node, and some fans start with degenerate
  tris that fool a single-triangle test.
- Fix (`scene.rs`): per-node fan winding aligned with the surf normal via
  Newell's method over the whole polygon. Screenshots refreshed.
- Note: section stored diffuse UVs are NOT texel-scale (0..1-ish, some
  garbage) — keep using the surf-vector `texel_uv` recompute. Documented
  in code.

---

# Handoff (2026-10-30, late night — v1.9.0 done)

## Done this session (v1.9.0)

- **Articulated ragdolls** (`openkf-viewer::ragdoll`, 64 bones): joint
  limits + spring-damper limbs; corpses flail then settle. Skinning
  applies quaternion deltas post-multiplied (`skin.rs`). Tests:
  `limbs_flail_then_settle`, `joint_limit_holds`, `seed_varies_fall_direction`,
  `falls_and_settles_flat`.
- **Projected decals** (`particles.rs`): surface-aligned basis, expiry,
  budget recycles oldest. Tests: `decal_basis`, `decals_expire`,
  `budget_recycles_oldest`.
- **F2 in-viewport browser** (`browser_ui.rs`): 6-row overlay, cursor,
  rescan, join; visually verified in-window (screenshot in README).
  Tests: `lines_always_six`, `cursor_moves`, `pump_merges`.
- **TLS + Steam Web API internet list** (`openkf-network::browser`):
  `ureq` with `native-tls` (needs an explicit `AgentBuilder` TLS
  connector, not the shortcuts) + `IGameServersService/GetServerList`
  behind `OPENKF_STEAM_API_KEY`. Live 403 invalid-key path verified.
  New deps: `ureq 2.12`, `serde_json 1.0` (workspace + network manifests).
- **Perk auto-select fallback** (`live_view.rs`): after 10 s pawnless on a
  custom gametype, send `SelectVeterancy` + `ServerRestartPlayer`. Gated
  so retail gametypes are untouched.
- **RGameType limbo verdict**: an 8-minute join on a custom-gametype
  public server never opened a `KFHumanPawn` (no possession) even with
  the fallback — H1 (perk flow is the missing piece) rejected for
  `ServerPerks`-style servers. Documented in STATUS; retail spawns fine.
- **Retail 120 FPS = hard engine ceiling**: windowed `MaxClientFrameRate`
  is rewritten by the engine (only the `+400.0` format sticks in the
  ini), and even fullscreen-exclusive 1600x900 at cap 400 reads 117 cur
  / 120 avg `stat fps`. Proof shot: `docs/screenshots/shot-retail-cap.png`.
  Retail `KillingFloor.ini` restored afterwards (120.000000, windowed).
- **English client**: HUD/console strings translated (`DOSH`, sounds);
  `£` has no bitmap-font glyph so dosh reads `DOSH 300`.
- **Benchmarks re-run** (see BENCHMARKS.md): solo 671 FPS / 0.14 s load /
  713 MB; live 95-99 FPS at 150-158 actors in wave combat with kills;
  retail 3.32 s / 597 MB. Live is down from v1.8.0's 201 (new per-frame
  effects cost) — reported honestly, profiling is next.
- **Toolchain note**: scoop's `mingw-winlibs` dlltool is an i686-only
  build (default machine `i386`, no `i386:x86-64`) and fails fresh
  `windows-sys` builds with `invalid bfd target`. Fix: put scoop `gcc`'s
  `bin` (binutils 2.45.1, default `i386:x86-64`) first in `PATH` before
  any `cargo build`. MSVC users are unaffected.
- `scripts/bench.ps1` kills stale `UCC.exe` before starting the local
  server (a leftover holding port 7717 used to wedge the live run).
- **Transient crash cluster (not a regression)**: 3 windowed-live joins in
  a row aborted with `memory allocation failed` right after a stuck
  PresentMon run left a stale ETW session and SIGKILLed processes behind.
  v1.8.0 survived the same map, debug survived too, and 10/10 targeted
  release re-runs after the system quiesced were clean — no new `unsafe`,
  no suspicious diff, no repro. Verdict: environmental fallout, not a
  code bug. If it ever recurs, suspect GPU/ETW wedging first.

## Suggested next steps (updated)

1. Profile the live frame time (95-99 FPS vs 201 in v1.8.0): find whether
   ragdoll articulation, projected decals, or particles dominate, and
   budget accordingly (fewer active ragdolls, decal LOD).
2. Custom-gametype spawn: trace what `ServerPerks`-style servers need
   after `SelectVeterancy` (retail packet capture of a working client).
3. Steam Web API key UX: document where to get a key, cache the list.
4. 3D browser polish (mouse, server details pane) if time allows.

---

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
