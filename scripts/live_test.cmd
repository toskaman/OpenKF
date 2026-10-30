@echo off
rem OpenKF - automated live test: local dedicated server + client --connect + screenshot.
rem Results in output\live\ (status.txt, build.txt, viewer.txt, server.log, live.png).
rem Set KF_DIR to your Killing Floor folder. Optional: LIVE_SECONDS, LIVE_EXTRA.
setlocal
set ROOT=%~dp0..
if "%KF_DIR%"=="" set KF_DIR=C:\Program Files (x86)\Steam\steamapps\common\KillingFloor
if "%LIVE_SECONDS%"=="" set LIVE_SECONDS=90
cd /d "%ROOT%"
if not exist output\live mkdir output\live
echo [%date% %time%] start > output\live\status.txt
taskkill /IM UCC.exe /F >nul 2>&1
start "OpenKF test server" /min /D "%KF_DIR%\System" "%KF_DIR%\System\UCC.exe" server KF-WestLondon.rom?game=KFmod.KFGameType?VACSecured=false?MaxPlayers=6 -port=7717 -log=OpenKF_TestServer.log
cargo build --release -p openkf-viewer > output\live\build.txt 2>&1
echo [%date% %time%] build exit %ERRORLEVEL% >> output\live\status.txt
timeout /t 5 /nobreak >nul
target\release\openkf-viewer.exe --connect 127.0.0.1:7717 --kf "%KF_DIR%" --screenshot output\live\live.png --time %LIVE_SECONDS% %LIVE_EXTRA% --size 1280x720 > output\live\viewer.txt 2>&1
echo [%date% %time%] viewer exit %ERRORLEVEL% >> output\live\status.txt
taskkill /IM UCC.exe /F >nul 2>&1
copy /y "%KF_DIR%\System\OpenKF_TestServer.log" output\live\server.log >nul
echo [%date% %time%] done >> output\live\status.txt
