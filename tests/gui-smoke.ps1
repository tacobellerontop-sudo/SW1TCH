# Drives the sw1tch window (via window messages) against a fake Riot Client.
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
}
function Status($win) {
  ($win.FindAll($Tree::Children, [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object { $_.Current.Name }) -join ' | '
}
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr p, IntPtr a, string c, string t);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, int m, IntPtr w, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr SendMessage(IntPtr h, int m, IntPtr w, StringBuilder l);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, int m, IntPtr w, IntPtr l);
  public static string[] Items(IntPtr list) {
    int n = (int)SendMessage(list, 0x18B, IntPtr.Zero, IntPtr.Zero); // LB_GETCOUNT
    var r = new string[n];
    for (int i = 0; i < n; i++) { var sb = new StringBuilder(256); SendMessage(list, 0x189, (IntPtr)i, sb); r[i] = sb.ToString(); } // LB_GETTEXT
    return r;
  }
}
"@

Login 'A'; & $cli save main --no-launch | Out-Null
Login 'B'; & $cli save alt --no-launch | Out-Null
Check ((Get-Content $yaml) -eq 'B') 'alt is live before the GUI starts'
& $cli list

$proc = Start-Process (Join-Path $Bin 'sw1tch.exe') -PassThru
$hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 50 -and $hwnd -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 200; $hwnd = [W]::FindWindow('Sw1tchWindow', [NullString]::Value) }
Check ($hwnd -ne [IntPtr]::Zero) 'window opened'
Start-Sleep -Milliseconds 500
$win = $UIA::FromHandle($hwnd)
Shot $win 'gui-start.png'

$list = [W]::FindWindowEx($hwnd, [IntPtr]::Zero, 'ListBox', [NullString]::Value)
$items = [W]::Items($list)
Write-Host "list: $($items -join ', ')"
Check ($items.Count -eq 2 -and $items[0] -eq 'alt    (active)' -and $items[1] -eq 'main') 'list shows both accounts with alt active'
Check ((Status $win) -match 'Double-click an account') 'status shows the hint'

# Select 'main' and click Switch.
[void][W]::SendMessage($list, 0x186, [IntPtr]1, [IntPtr]::Zero)  # LB_SETCURSEL
$switch = [W]::FindWindowEx($hwnd, [IntPtr]::Zero, 'Button', 'Switch')
[void][W]::PostMessage($switch, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero)  # BM_CLICK

for ($i = 0; $i -lt 50 -and (Get-Content $yaml -ErrorAction SilentlyContinue) -ne 'A'; $i++) { Start-Sleep -Milliseconds 200 }
Check ((Get-Content $yaml) -eq 'A') 'clicking Switch restored main'
Start-Sleep -Milliseconds 800
$status = Status $win
Write-Host "status: $status"
Check ($status -match "Switched to 'main'") 'status shows the switch'
Check (Get-Process RiotClientServices -ErrorAction SilentlyContinue) 'Riot Client was started'
$items = [W]::Items($list)
Write-Host "list: $($items -join ', ')"
Check ($items[1] -eq 'main    (active)') 'list marks main as active'
Shot $win 'gui-switched.png'

[void][W]::PostMessage($hwnd, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)  # WM_CLOSE
Check ($proc.WaitForExit(5000)) 'window closes cleanly'
Get-Process RiotClientServices -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Host 'gui smoke test passed'
exit 0
