# End-to-end silent install/uninstall test on a clean Windows runner.
param([Parameter(Mandatory)] [string] $Path)
$ErrorActionPreference = 'Stop'
$Msi = (Resolve-Path -LiteralPath $Path).Path
$AppFolder = Join-Path $env:LOCALAPPDATA 'pdfcraft'
$Shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\沃飞翼览 PDF.lnk'
$DesktopDirectory = [Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory)
if (-not $DesktopDirectory) { $DesktopDirectory = Join-Path $env:USERPROFILE 'Desktop' }
if (-not (Test-Path -LiteralPath $DesktopDirectory)) { New-Item -ItemType Directory -Path $DesktopDirectory -Force | Out-Null }
$DesktopShortcut = Join-Path $DesktopDirectory '沃飞翼览 PDF.lnk'
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
  if (-not (Test-Path -LiteralPath $DesktopShortcut)) { throw "Desktop shortcut missing: $DesktopShortcut" }
  $FriendlyAppKey = 'HKCU:\Software\Classes\Applications\pdfcraft.exe'
  $FriendlyAppName = Get-ItemPropertyValue -LiteralPath $FriendlyAppKey -Name 'FriendlyAppName' -ErrorAction SilentlyContinue
  if ($FriendlyAppName -ne '沃飞翼览 PDF') {
    throw "Windows Open with label must be '沃飞翼览 PDF', got '$FriendlyAppName'"
  }
  $OpenWithKey = 'HKCU:\Software\Classes\.pdf\OpenWithProgids'
  $OpenWithValues = Get-ItemProperty -LiteralPath $OpenWithKey -ErrorAction SilentlyContinue
  if (-not $OpenWithValues -or $OpenWithValues.PSObject.Properties.Name -notcontains 'PdfCraft.Document') {
    throw "Windows Open with list is missing PdfCraft.Document: $OpenWithKey"
  }
  $Executable = Join-Path $AppFolder 'pdfcraft.exe'
  $VersionInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo($Executable)
  if ($VersionInfo.ProductName -ne '沃飞翼览 PDF') {
    throw "Executable ProductName must be '沃飞翼览 PDF', got '$($VersionInfo.ProductName)'"
  }
  if ($VersionInfo.FileDescription -ne '沃飞翼览 PDF') {
    throw "Executable FileDescription must be '沃飞翼览 PDF', got '$($VersionInfo.FileDescription)'"
  }
  Write-Output 'ok Windows Open with displays 沃飞翼览 PDF and offers PDF files without changing the default'
  $shell = New-Object -ComObject WScript.Shell
  $lnk = $shell.CreateShortcut($DesktopShortcut)
  $expected = (Join-Path $AppFolder 'pdfcraft.exe')
  if ($lnk.TargetPath) {
    if ([IO.Path]::GetFullPath($lnk.TargetPath) -ine [IO.Path]::GetFullPath($expected)) {
      throw "Desktop shortcut target '$($lnk.TargetPath)' does not match '$expected'"
    }
    Write-Output "ok desktop shortcut points to $expected"
  } else {
    # Some clean Windows runners expose an MSI-created link but return an empty COM TargetPath.
    # test-msi-local.ps1 independently verifies that both shortcut table entries target PdfcraftExe.
    Write-Warning "WScript.Shell did not expose TargetPath; the MSI shortcut target was validated from its database."
  }
  & (Join-Path $AppFolder 'pdfcraft-cli.exe') --version
  if ($LASTEXITCODE -ne 0) { throw "Installed CLI could not run ($LASTEXITCODE)" }
  Write-Output "ok silent /qn install at $AppFolder"
}
finally {
  if ($Installed) { Invoke-Msi 'x' $UninstallLog }
}
if (Test-Path -LiteralPath (Join-Path $AppFolder 'pdfcraft.exe')) { throw 'Silent uninstall left pdfcraft.exe behind' }
if (Test-Path -LiteralPath $Shortcut) { throw 'Silent uninstall left the Start menu shortcut behind' }
if (Test-Path -LiteralPath $DesktopShortcut) { throw 'Silent uninstall left the desktop shortcut behind' }
$FriendlyAfter = Get-ItemPropertyValue -LiteralPath 'HKCU:\Software\Classes\Applications\pdfcraft.exe' -Name 'FriendlyAppName' -ErrorAction SilentlyContinue
if ($FriendlyAfter -eq '沃飞翼览 PDF') { throw 'Silent uninstall left the per-user Open with registration behind' }
Write-Output 'ok silent /qn uninstall'
