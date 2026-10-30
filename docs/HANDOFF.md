# OpenKF — work log and next steps (2026-10-30)

## Done this session
- Pickups / static-mesh projectiles rendered live (`openkf-viewer/src/props.rs`, pool from class defaults: 95 meshes / 303 slots).
- Pawn animation: direction-aware MovementAnims / Walk / Crouch, idle anims, replicated `AnimAction` one-shots, death handling (death anim or fall-over, 6 s corpses); attachments follow owner's one-shot pose.
- English HUD; automated test player disabled on public servers (spectator unless `--play`).
- FastDL implemented for real: `DLMGR` HTTP redirect parsing, missing `USES` packages downloaded before the level loads, `.uz2` decompression (flate2), GUID check, tweaked package tag (0x9E2A83C2) normalized, stored in `%LOCALAPPDATA%\OpenKF\Downloads`, searched by schema + asset loader + map resolution. Verified: KF-Space-Airlock[WSB].rom (1386 KB) fetched from a live redirect in 5 s.
- `examples/fastdl.rs` probe tool; map-load failure exits cleanly; WELCOME wait raised to 180 s.
- Tests: whole workspace green (39 test binaries), 3 new FastDL tests, 2 props tests.
- Live check on local server: 0 decode failures, locomotion anims resolved.
- Public modded server: handshake + Steam auth + WELCOME OK on first try; later attempts got no reply after LOGIN (server-side, unresolved).

## Next steps
1. Retry the public server (`--connect <ip:port>`) now that FastDL works; confirm all custom packages download and the map renders.
2. Particle emitters / decals; Karma ragdolls.
3. Server browser in the viewer (code in `openkf-app::steam_matchmaking`).
4. Benchmarks retail vs OpenKF: RAM, CPU %, cores/threads used, map load time, FPS solo and multiplayer (same map, same camera path, same resolution), publish method + raw data in README.

## Update (end of session)
- `steam_appid.txt` found in `KillingFloor\System` and renamed to `steam_appid.txt.openkf-bak` (not the cause).
- Retail client joins the public test server fine; OpenKF gets STEAMENCRYPTIONKEY + CHALLENGE, sends AUTH/LOGIN, then the server stays silent (no WELCOME). Name is not the cause.
- Added `TerminateGameConnection` before `InitiateGameConnection` and on client drop (retail behaviour); live verification pending.
- Next: record a retail join with `openkf-netcap` (relay) and diff the AUTH / LOGIN / Steam blob messages against OpenKF's (the STEAMTICKET chunks with SIZE=1 are a suspect).
