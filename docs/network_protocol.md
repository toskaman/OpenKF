# Killing Floor v1065 Network Protocol — OpenKF Notes

Status as of 2026-10-25: **packet/bunch framing and handshake verified** on a real
session (`captures/capture_1792954639.txt`: 100s, 7330 datagrams, 0 errors, re-encoding
byte-identical). Code: `crates/openkf-network/src/wire.rs`, test `tests/capture_replay.rs`,
tool `cargo run -p openkf-network --example netdump -- <capture> [--ctl]`.

## Datagram (LSB-first bits)

`SerializeInt(v, Max)` writes bits of v while `v + mask < Max` (ceil(log2 Max) bits).

```
PacketId        SerializeInt(16384)                 14 bits
repeat:
  bIsAck        1 bit
  if ack:       AckPacketId SerializeInt(16384)
  else bunch:
    bControl    1 bit
    if bControl: bOpen 1 bit, bClose 1 bit
    bReliable   1 bit
    ChIndex     SerializeInt(1023)                  10 bits
    if bReliable: ChSequence SerializeInt(1024)     10 bits
    if bReliable || bOpen: ChType SerializeInt(8)   3 bits
    DataBits    SerializeInt(4096)                  12 bits
    Data        DataBits bits
end: terminal 1 bit (high bit of last byte)
```

Observed channel types: 1 = Control (channel 0), 2 = Actor, 4 = Voice (opened after `OPENVOICE 1`).
Each datagram individually acks received PacketIds (no bitmask).

## Control Channel

Data = sequence of `FString` (length as FCompactIndex, NUL-included; negative = UTF-16).

Actual observed sequence:

| t (ms) | Dir | Message |
|---|---|---|
| 4342 | C→S | `HELLO REVISION=0 MINVER=3180 VER=3339 STEAMID=<steam64>` |
| 4370 | S→C | `STEAMENCRYPTIONKEY STEAMID=<server id> SECURE=0` |
| 6171 | C→S | `STEAMCLIENTBLOB SIZE=242 CHUNK=0..15 BLOB=<hex>` (16 chunks) |
| 6172 | C→S | `STEAMTICKET SIZE=1 CHUNK=0..3` (empty ticket) |
| 6210 | S→C | `CHALLENGE VER=3339 CHALLENGE=<int> STATS=0 SEC=1 GZ=39` |
| 6212 | C→S | `AUTH HASH=<md5> GM=<hex>` |
| 6212 | C→S | `NETSPEED 15000` |
| 6212 | C→S | `LOGIN RESPONSE=<int> URL=Index.rom?Class=Engine.Pawn?Character=…?Name=…` |
| 6221 | S→C | `USES GUID=… PKG=… FLAGS=… SIZE=… GEN=… FNAME=…` × 336 (package map, in order) |
| 6224 | S→C | `DLMGR CLASS=Engine.ChannelDownload PARAMS=Enabled COMPRESSION=0` |
| 6224 | S→C | `WELCOME LEVEL=KF-WestLondon GAME=KFMod.KFGameType LONE=0` |
| 7763 | C→S | `PETE PKT=11 PKG=12` then 11 × `REPEAT <binary>` (integrity check?) |
| 7763 | C→S | `JOIN` |
| 7848 | C→S | `OPENVOICE 1` |
| 9734 | C→S | `SA N=<achievement>` (Steam achievement state) |
| 102233 | S→C | `FAILCODE STEAMAUTH` → disconnect |

Notes:
- KF handshake differs from UT2004: Steam step (key, client blob, ticket) before `CHALLENGE`.
- Disconnect at ~100s is server-side Steam auth (`STEAMTICKET SIZE=1`: empty ticket,
  likely due to relay/local server). Sufficient for analysis; for a real OpenKF client
  we will need a valid ticket or target non-Secure servers.
- `AUTH HASH`, `GM`, `RESPONSE` and `PETE/REPEAT` depend on `CHALLENGE`: needs
  reverse-engineering (Engine.dll) for OpenKF to connect.
- Captures contain player SteamID and Steam blob: do not publish.

## Package Map (verified)

Each `USES` (in order) adds `ObjectCount` = number of exports in generation `GEN` of
local package. `ObjectBase` = sum of previous; `MaxObjectIndex` = total (115,642 for this
336-package session). An object travels as `SerializeInt(ObjectBase + ExportIndex, MaxObjectIndex)`.
The level package (`.rom`) is first: its exports are static actors.
Code: `src/package_map.rs`.

## Actor Channel Open (verified)

```
1 bit       unknown, always 0
Object      SerializeInt(MaxObjectIndex)
  level object (Mover, Volume, LevelInfo…) -> nothing else (static actor)
  class (dynamic actor) -> Location: UE2 compressed vector
      Bits = SerializeInt(20); 3 × (SerializeInt(1 << (Bits+2)) - (1 << (Bits+1)))
  then initial rotation + properties (to decode)
```

On capture: 292 opens, 137 static, 154 dynamic correctly resolved
(KFPlayerController, KFHumanPawn, ZombieClot/Bloat/Gorefast, weapons, pickups, projectiles…),
1 suspicious (index 113218, end of table: late package counting to verify).
Tool: `cargo run -p openkf-network --example actors -- <capture> <KF folder> [n]`.

## Property Replication (verified: 3952/3952 bunches decoded exactly, 100%)

Sources: Core.dll / Engine.dll disassembly (addresses below), validated on capture.

**Network field table for a class** (`UClass::Link`, Core 0x10123440; `GetClassNetCache`, Core 0x101258E0)
- Per class: `CPF_Net` (0x20) properties + `FUNC_Net` (0x40) functions **without super-functions**,
  sorted by **LinkerIndex** = export index in package (comparator Core 0x10124280).
- RepIndex = (total fields of superclasses) + position. Max = class total.

**Actor bunch**: until end of bunch, `RepIndex = SerializeInt(Max)` then:
- property: if `ArrayDim != 1`, 1 byte element index; then value;
- RPC: for each parameter (order `Children`, `CPF_Parm` without `CPF_ReturnParm`):
  bool → 1 bit; else 1 bit "present" then value.

**Values** (`NetSerializeItem`)
| Type | Format |
|---|---|
| Int / Float | 32 bits |
| Bool | 1 bit |
| Byte | `ceil(log2(enum values))` bits, 8 if no enum |
| Name | `SerializeInt(total names across packages + 1)` (name tables concatenated in USES order) |
| Object / Class | 1 bit; 1 → dynamic actor `SerializeInt(ChIndex, 1023)`; 0 → `SerializeInt(object index, MaxObjectIndex)` (Engine 0x1046AE50) |
| Str | FString |
| Vector | compressed vector (`Bits = SerializeInt(20)`, 3 × `SerializeInt(1<<(Bits+2)) - (1<<(Bits+1))`) |
| Rotator | 3 × (1 bit + optional byte, <<8) |
| CompressedPosition | Vector (Location), byte Pitch, byte Yaw, Vector (Velocity) |
| Plane | 4 × int16 |
| other struct | each member (derived struct then super-struct), each element |

**Open**: after Location, a Rotation (Rotator format) if class has `bNetInitialRotation`.
Currently detected by trial (read from class defaults: TODO).

Verified values: `KFGameReplicationInfo.TimeToNextWave` countdown 59 → 1, Clot health
(65, then negative on death), `PawnPosition` of zeds advancing continuously.
Code: `src/schema.rs`, `src/props.rs`, `src/session.rs`; `examples/replay.rs`.

## Client-Side Calculations (Engine.dll, verified on capture)

- **`LOGIN RESPONSE`** = `UGameEngine::ChallengeResponse` (0x104225F0):
  `(c >> 16) ^ (c * 0xED) ^ (c << 16) ^ 0x93FE92CE` (32-bit, `>>` arithmetic). Verified: -705965475 → -1257404612.
- **`AUTH HASH`** = `GetCDKeyHash()`: constant per install (from capture).
- **`AUTH GM`** (0x10486BC5): 64 random bytes where 16 bytes of `GMD5` are interleaved by quartets
  starting at offset `k2`, xored by `k1+i`, followed by `k1`, `k2`; as hex. On v1065,
  `GMD5` is `0041b800…` (near-zero): server does not seem to check it.
- **`PETE PKT=n PKG=12` + `REPEAT`** (0x104289A0): for 12 code packages
  (Engine, Core, XGame, IpDrv, Editor, ROEffects, ROEngine, XInterface, GUI2K4, UWeb, ROInterface, xVoting),
  string `GUID(32 hex) + MD5 of .u file (32 hex)`; the 12 strings are interleaved column-wise,
  each char shifted by `(GZ + k) & 0x7f` (`GZ` from CHALLENGE, k incrementing),
  then split into ~50-char chunks. Capture decode: MD5 identical to local files.
- **Steam**: `STEAMTICKET SIZE=1 CHUNK=0..3` is a hardcoded dummy; real auth is
  `STEAMCLIENTBLOB` (242 bytes, 16 chunks), validated by server via
  `SendUserConnectAndAuthenticate`. A standalone client must produce it via the game's
  `steam_api.dll` (`InitiateGameConnection`).

Code: `src/handshake.rs` (calcs + tests), `src/live.rs` (UDP client), `examples/live.rs`.

## Tools

- `crates/openkf-netcap`: UDP relay that records every packet.
- `scripts/net_capture.ps1`: local dedicated server (7717, ini `OpenKF_Capture.ini` without
  MasterServerUplink which crashed on PORTSWAP) + relay (7707) + client.

## TODO

1. ~~Packet/bunch framing~~ ✔  2. ~~Handshake~~ ✔ (except AUTH/RESPONSE/PETE calcs)
3. ~~Package map~~ ✔
4. ~~Actor channel open (class, Location)~~ ✔ — remaining: initial Rotation and suspicious index.
5. ~~Property replication~~ ✔ (from defaults for bNetInitialRotation) (Pawns: position, rotation, velocity, health).
6. ~~Spectator replay in viewer~~ ✔ (`openkf-viewer --replay <capture>`)
7. ~~Live client~~ ✔ tested on local server: connection accepted, 127 actors, 0 errors.
   `openkf-viewer --connect host:port`. Client RPCs bit-identical (`write_rpc`, test vs retail client).
   Lobby "Ready" = RPC `ServerRestartPlayer` (LobbyFooter.uc): match starts and zeds
   appear live in viewer (auto-test `scripts/openkf_live_test.cmd`, 2026-10-25 22:05: 10 Pawns).
8. ~~Movement~~ ✔ `ServerMove`/`ShortServerMove` unreliable at 30 Hz (`LiveClient::send_move`):
   `InAccel` = accel ×10, `ClientLoc` = last `ClientAdjustPosition`, `View` = (Pitch/2 & 32767)·32768 + (Yaw/2 & 32767). **Prereq**: after spawn, send
   `ServerAcknowledgePossession(P = Pawn channel, NewHand = 1.0, bNewAutoTaunt = false)`, else
   server zeroes accel (AcknowledgedPawn). Tested: OpenKF kites zeds ~20s.
9. ~~Shooting~~ ✔ weapons are actors only relevant to owner (Welder, Syringe, Frag, Single, knife);
   `Pawn.ServerChangedWeapon(OldWeapon, newWeapon)` (Pawn channel), `Weapon.ServerStartFire/StopFire(Mode)` and
   `Weapon.ReloadMeNow` (weapon channel); aim = `View` from ServerMove. Kills in
   `PlayerReplicationInfo.Kills`. Test: 3 kills / 6 shots. Playable: `openkf-viewer --connect … --play`.
10. ~~Steam without capture~~ ✔ game's `steam_api.dll` is 32-bit → use `steamclient64.dll`
    (from registry Steam folder): `CreateInterface("SteamClient012")` → `CreateSteamPipe` (vtable 0) →
    `ConnectToGlobalUser` (2) → `GetISteamUser(…, "SteamUser017")` (5) → `InitiateGameConnection` (3)
    with server SteamID from `STEAMENCRYPTIONKEY`, server IP/port and `SECURE`. 2048-byte buffer
    goes as 16 × `STEAMCLIENTBLOB SIZE=n CHUNK=i BLOB=<128 hex bytes>`. Local SteamID =
    76561197960265728 + `HKCU\Software\Valve\Steam\ActiveProcess\ActiveUser`. Server:
    `STEAMAUTH: Accepted Client`. `AUTH HASH`/`GMD5`: defaults (unchecked). Viewer default.
11. ~~Keyboard weapons, grenade, trader, HUD~~ ✔: `ServerChangedWeapon` verified; grenade =
    `Frag.ServerStartThrow` then `ServerThrow` ~180 ms later (FragAmmo 4 → 3, explosion seen);
    `ServerBuyWeapon(WClass, ItemWeight)` / `ServerBuyAmmo(aClass, bOnlyClip)` sent with class indices
    from package map (`GameSchema::object_index`) — validate at trader; HUD rendered.
12. ~~Map change~~ ✔ server sends `PlayerController.ClientTravel(URL)` (e.g. after team wipe)
    then closes connection: client reconnects (`Traveling` state, or silence > 6s) and viewer
    reloads map. Tested: WestLondon → BioticsLab → Farm and WestLondon → Manor → Offices,
    **0 decode errors** (680 / 4255 / 279 bunches).
13. ~~Class defaults~~ ✔ read from `.uc` sources shipped with game
    (`<KF>/<Package>/Classes/*.uc`, 3,439 classes, `openkf-package::uc_defaults`): `bNetInitialRotation`
    (replaces trial "with/without rotation": 3952/3952 bunches), mesh per class (standard zeds,
    Husk, Christmas/Halloween/Circus variants), 3rd-person weapon meshes. Pitfall: match line *starting*
    with `class`, header comments often contain the word.
14. ~~Held weapons~~ ✔ `WeaponAttachment`: `Base` = carrier Pawn channel, `AttachmentBone` = name (package
    map name table, e.g. `WeaponR_Bone`); render: weapon point → global bone matrix of parent → parent mesh
    transform (`zeds.rs::skin_all`, 2 passes). Mesh = `Mesh` default of attachment class.
    Known quirk (now fixed): `ReceiveLocalizedMessage(pick.MessageClass,,,,pick.class)`
    (optional params *omitted* in UnrealScript) arrived with one fewer presence bit;
    flow re-syncs by skipping one bit (next = `ClientHearSound` of pickup). Fixed via CPF_OPTIONAL_PARM.
15. ~~Sound~~ ✔ `PlayerController.ClientHearSound(Actor, Id, S, SoundLocation, Parameters, Attenuate)`:
    `S` = `Sound` object (full WAV RIFF embedded after small header, `openkf-package::sound`) or `SoundGroup`
    (data = `None` + count + compact refs to `Sound`, often imported from another .uax;
    `GameSchema::sound_group_members`); `Parameters` = (volume×100, radius, pitch×100). Render: `openkf-audio`
    mixer (spatialized, resampled to device rate) + `cpal`/WASAPI (Windows) output.
    Test: 41 sounds played, 0 failures. Own shots (never echoed by server): local sound from
    `FireModeClass(0)` → `FireSound` (e.g. `KF_9MMSnd.9mm_Fire`) — 90 sounds / 25 shots.
16. Next: projectiles, sound enhancements, server browser, trader
    (bot able to finish wave 1: in progress; `MaxZombiesOnce=2` in `OpenKF_Capture.ini` helps).
