# Drives the sw1tch window with UI Automation against a fake Riot Client.
# Usage (Windows PowerShell 5.1): powershell -File tests/gui-smoke.ps1 <release-dir> <fake_riot.exe> <screenshot-dir>
param([string]$Bin, [string]$Fake, [string]$Shots)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
$UIA = [System.Windows.Automation.AutomationElement]
$Tree = [System.Windows.Automation.TreeScope]

$root = Join-Path ([IO.Path]::GetTempPath()) 'sw1tch-gui-smoke'
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
$riot = Join-Path $root 'riot'; $rc = Join-Path $root 'rc'
New-Item -ItemType Directory -Force $riot, $rc, $Shots | Out-Null
$Shots = (Resolve-Path $Shots).Path
$Bin = (Resolve-Path $Bin).Path
$client = Join-Path $rc 'RiotClientServices.exe'
Copy-Item $Fake $client
$env:SW1TCH_RIOT_DATA = $riot
$env:SW1TCH_HOME = Join-Path $root 'store'
$env:SW1TCH_RIOT_CLIENT = $client
$cli = Join-Path $Bin 'sw1tch-cli.exe'
$yaml = Join-Path $riot 'Data\RiotGamesPrivateSettings.yaml'

function Login($who) {
  Remove-Item -Recurse -Force (Join-Path $riot 'Data') -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force (Join-Path $riot 'Data'), (Join-Path $riot 'Config') | Out-Null
  Set-Content $yaml $who
}
function Check($cond, $msg) { if (-not $cond) { throw "FAILED: $msg" }; Write-Host "ok: $msg" }
function Find($parent, $prop, $value) {
  for ($i = 0; $i -lt 50; $i++) {
    $c = New-Object System.Windows.Automation.PropertyCondition($prop, $value)
    $e = $parent.FindFirst($Tree::Descendants, $c)
    if ($e) { return $e }
    Start-Sleep -Milliseconds 200
  }
  $all = $parent.FindAll($Tree::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  $all | ForEach-Object { Write-Host ("  [{0}] '{1}'" -f $_.Current.ControlType.ProgrammaticName, $_.Current.Name) }
  throw "not found: $value"
}
function Shot($win, $file) {
  $r = $win.Current.BoundingRectangle
  $bmp = New-Object System.Drawing.Bitmap ([int]$r.Width), ([int]$r.Height)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen([int]$r.X, [int]$r.Y, 0, 0, $bmp.Size)
  $path = Join-Path $Shots $file
  $bmp.Save($path); $g.Dispose(); $bmp.Dispose()
  Write-Host "BEGIN-PNG $file"; Write-Host ([Convert]::ToBase64String([IO.File]::ReadAllBytes($path))); Write-Host "END-PNG"
}
function Status($win) {
  $c = New-Object System.Windows.Automation.PropertyCondition($UIA::ControlTypeProperty, [System.Windows.Automation.ControlType]::Text)
  ($win.FindAll($Tree::Descendants, $c) | ForEach-Object { $_.Current.Name }) -join ' | '
}

Login 'A'; & $cli save main --no-launch | Out-Null
Login 'B'; & $cli save alt --no-launch | Out-Null
Check ((Get-Content $yaml) -eq 'B') 'alt is live before the GUI starts'
& $cli list

$proc = Start-Process (Join-Path $Bin 'sw1tch.exe') -PassThru
$win = Find $UIA::RootElement $UIA::NameProperty 'SW1TCH - Valorant accounts'
Check ($win -ne $null) 'window opened'
Start-Sleep -Milliseconds 500
Shot $win 'gui-start.png'

$item = Find $win $UIA::NameProperty 'main'
$item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
$btn = Find $win $UIA::NameProperty 'Switch'
$btn.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()

for ($i = 0; $i -lt 50 -and (Get-Content $yaml -ErrorAction SilentlyContinue) -ne 'A'; $i++) { Start-Sleep -Milliseconds 200 }
Check ((Get-Content $yaml) -eq 'A') 'clicking Switch restored main'
Start-Sleep -Milliseconds 800
$status = Status $win
Write-Host "status: $status"
Check ($status -match "Switched to 'main'") 'status shows the switch'
Check (Get-Process RiotClientServices -ErrorAction SilentlyContinue) 'Riot Client was started'
$null = Find $win $UIA::NameProperty 'main    (active)'
Write-Host 'ok: list marks main as active'
Shot $win 'gui-switched.png'

$win.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
Check ($proc.WaitForExit(5000)) 'window closes cleanly'
Get-Process RiotClientServices -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Host 'gui smoke test passed'
exit 0
