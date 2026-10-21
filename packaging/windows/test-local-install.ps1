# End-to-end silent install/uninstall test on a clean Windows runner.
param([Parameter(Mandatory)] [string] $Path)
$ErrorActionPreference = 'Stop'
$Msi = (Resolve-Path -LiteralPath $Path).Path
$AppFolder = Join-Path $env:LOCALAPPDATA 'pdfcraft'
$Shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\PdfCraft (Local).lnk'
$InstallLog = Join-Path $env:TEMP 'pdfcraft-local-install.log'
$UninstallLog = Join-Path $env:TEMP 'pdfcraft-local-uninstall.log'
$Installed = $false

function Invoke-Msi([string] $Operation, [string] $Log) {
  $args = ('/{0} "{1}" /qn /norestart /L*v "{2}"' -f $Operation, $Msi, $Log)
  $process = Start-Process -FilePath 'msiexec.exe' -ArgumentList $args -Wait -PassThru
  if ($process.ExitCode -ne 0) {
    throw "msiexec /$Operation failed ($($process.ExitCode)); see $Log"
  }
}
if (Test-Path (Join-Path $AppFolder 'pdfcraft.exe')) {
  throw "Pre-existing PdfCraft installation would invalidate this smoke test: $AppFolder"
}
try {
  Invoke-Msi 'i' $InstallLog
  $Installed = $true
  foreach ($name in 'pdfcraft.exe', 'pdfcraft-cli.exe', 'portable.txt', 'models\text-detection.rten', 'models\text-recognition.rten') {
    if (-not (Test-Path -LiteralPath (Join-Path $AppFolder $name))) {
      throw "Installed payload missing: $name"
    }
  }
  if (-not (Test-Path -LiteralPath $Shortcut)) { throw "Start menu shortcut missing: $Shortcut" }
  & (Join-Path $AppFolder 'pdfcraft-cli.exe') --version
  if ($LASTEXITCODE -ne 0) { throw "Installed CLI could not run ($LASTEXITCODE)" }
  Write-Output "ok silent /qn install at $AppFolder"
}
finally {
  if ($Installed) { Invoke-Msi 'x' $UninstallLog }
}
if (Test-Path -LiteralPath (Join-Path $AppFolder 'pdfcraft.exe')) { throw 'Silent uninstall left pdfcraft.exe behind' }
if (Test-Path -LiteralPath $Shortcut) { throw 'Silent uninstall left the Start menu shortcut behind' }
Write-Output 'ok silent /qn uninstall'
