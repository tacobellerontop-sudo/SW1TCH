# End-to-end smoke test on Windows with a fake Riot Client.
# Usage: pwsh tests/smoke.ps1 <path-to-sw1tch.exe> <path-to-fake_riot.exe>
param([string]$Exe, [string]$Fake)
$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) "sw1tch-smoke"
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
$riot = Join-Path $root 'riot'; $rc = Join-Path $root 'rc'
New-Item -ItemType Directory -Force $riot, $rc | Out-Null
$client = Join-Path $rc 'RiotClientServices.exe'
Copy-Item $Fake $client
$env:SW1TCH_RIOT_DATA = $riot
$env:SW1TCH_HOME = Join-Path $root 'store'
$env:SW1TCH_RIOT_CLIENT = $client
$launched = Join-Path $rc 'launched.txt'

function Login($who) {
  Remove-Item -Recurse -Force (Join-Path $riot 'Data') -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force (Join-Path $riot 'Data\Cookies'), (Join-Path $riot 'Config') | Out-Null
  Set-Content (Join-Path $riot 'Data\RiotGamesPrivateSettings.yaml') $who
  Set-Content (Join-Path $riot 'Data\Cookies\c') $who
  Set-Content (Join-Path $riot 'Config\RiotClientSettings.yaml') "cfg$who"
}
function Run { Write-Host "> sw1tch $args"; & $Exe @args; if ($LASTEXITCODE) { throw "sw1tch $args failed ($LASTEXITCODE)" } }
function Live { (Get-Content (Join-Path $riot 'Data\RiotGamesPrivateSettings.yaml')).Trim() }
function Check($cond, $msg) { if (-not $cond) { throw "FAILED: $msg" }; Write-Host "ok: $msg" }
function ClientRunning { @(Get-Process RiotClientServices -ErrorAction SilentlyContinue).Count -gt 0 }

Login 'A'; Run save main --no-launch
Check (-not (ClientRunning)) 'save with --no-launch starts nothing'

Start-Process $client; Start-Sleep 1
Check (ClientRunning) 'fake client running'
Run new --client; Start-Sleep 1
Check (-not (Test-Path (Join-Path $riot 'Data'))) 'new clears the live login'
Check ((Get-Content $launched) -eq $null -or (Get-Content $launched) -eq '') 'new relaunched the client without Valorant args'

Login 'B'; Run save 'Smurf acct'; Start-Sleep 1
Check (ClientRunning) 'save relaunches a client that was running'

Run switch main; Start-Sleep 1
Check ((Live) -eq 'A') 'switch restores account A'
Check ((Get-Content $launched) -match 'launch-product=valorant') 'switch starts Valorant'
Check (@(Get-Process RiotClientServices -ErrorAction SilentlyContinue).Count -eq 1) 'old client was closed, one new one started'

Run switch 'SMURF ACCT' --no-launch
Check ((Live) -eq 'B') 'case-insensitive switch restores account B'
Check ((& $Exe current) -eq 'Smurf acct') 'current reports the active account'

Run rename 'Smurf acct' alt
Check ((& $Exe current) -eq 'alt') 'rename keeps the active marker'
Run remove alt
Check ((& $Exe list) -match 'main') 'list still shows main'

& $Exe bogus 2>$null; Check ($LASTEXITCODE -ne 0) 'unknown command fails'
Get-Process RiotClientServices -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Host 'smoke test passed'
exit 0
