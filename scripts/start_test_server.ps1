# OpenKF - start a local Killing Floor (v1065) dedicated test server on port 7717.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\start_test_server.ps1 [-Map KF-WestLondon] [-KF <game dir>]
param(
    [string]$Map = "KF-WestLondon",
    [string]$KF = $(if ($env:KF_DIR) { $env:KF_DIR } else { "C:\Program Files (x86)\Steam\steamapps\common\KillingFloor" })
)
$Sys = Join-Path $KF "System"
Start-Process -FilePath "$Sys\UCC.exe" -WorkingDirectory $Sys -WindowStyle Minimized `
    -ArgumentList "server $Map.rom?game=KFmod.KFGameType?VACSecured=false?MaxPlayers=6 -port=7717 -log=OpenKF_TestServer.log"
Write-Host "Server started on port 7717 (log: $Sys\OpenKF_TestServer.log)"
