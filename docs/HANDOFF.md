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

## Retail capture diff (public modded server)
- Retail join captured through `openkf-netcap` (captures stay local, never committed).
- Control messages now match retail: `AUTH ... USERNAME= PASSWORD=`, `NETSPEED 25000`, `?VAC=1` in the LOGIN URL. Challenge response and GMD5 verified identical to retail.
- Server still silent after LOGIN for OpenKF only. Remaining differences are below the control layer (packet acks / timing; retail sends the Steam blob ~1.7 s after STEAMENCRYPTIONKEY) or Steam-side session state.
- Next: capture OpenKF through the same relay and diff packet by packet against the retail capture.

## Root cause found: the server was never silent
- A capture of OpenKF through `openkf-netcap` showed the server sends DLMGR + WELCOME right after LOGIN. OpenKF was busy downloading the server's ~1-2 GB of custom packages (FastDL) inside the WELCOME handler, so the viewer's 180 s join timeout fired first.
- Fixes: retail download cache (`<KF>/Cache/Cache.ini`, `<GUID>.uxx`) is searched before downloading; download progress printed live; join wait raised to 900 s. Downloads persist in `%LOCALAPPDATA%\OpenKF\Downloads`, so later joins are fast.
- TODO: download in a background thread while keeping the connection alive (retail behaviour), and show progress in the HUD.

## Retail cache + first public join (done)
- First successful join of the public modded server (wave 5, 44 zeds rendered, spectator).
- Retail cache format fixed: `Cache.ini` keys are `<GUID>-<GEN>`, files `<GUID>-<GEN>.uxx`; lookup is now by exact USES GUID (`find_package_by_guid`), so packages the retail client already downloaded are reused.
- Open: maps that are not on the HTTP redirect (e.g. KF-BOX -> 404) need UE2 ChannelDownload (CHTYPE_FILE). Needs a retail capture of a channel download to implement exactly.
- Open: ~25% custom sounds fail to play; downloads should run in the background with a HUD progress bar.
