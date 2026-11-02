# Inspect the compiled LocalAppData MSI without changing machine state.
param([Parameter(Mandatory)] [string] $Path)
$ErrorActionPreference = 'Stop'
$Installer = New-Object -ComObject WindowsInstaller.Installer
$Database = $Installer.OpenDatabase((Resolve-Path -LiteralPath $Path).Path, 0)

function First([string] $Sql) {
  $view = $Database.OpenView($Sql)
  try {
    [void] $view.Execute()
    $record = $view.Fetch()
    if ($record) { return $record.StringData(1) }
    return $null
  } finally {
    [void] $view.Close()
  }
}
function Require([bool] $Pass, [string] $Message) {
  if (-not $Pass) { throw "MSI validation failed: $Message" }
}

$allUsers = First 'SELECT `Value` FROM `Property` WHERE `Property` = ''ALLUSERS'''
Require ($allUsers -ne '1') 'ALLUSERS must not select per-machine scope'
$parent = First 'SELECT `Directory_Parent` FROM `Directory` WHERE `Directory` = ''INSTALLFOLDER'''
Require ($parent -eq 'LocalAppDataFolder') "INSTALLFOLDER must be under LocalAppDataFolder, got '$parent'"
$name = First 'SELECT `DefaultDir` FROM `Directory` WHERE `Directory` = ''INSTALLFOLDER'''
Require ($name -match '(^|\|)pdfcraft$') "install directory must be pdfcraft, got '$name'"

# The ChatGPT-generated red PDF icon must remain the Add/Remove Programs and shortcut icon.
$arpIcon = First 'SELECT `Value` FROM `Property` WHERE `Property` = ''ARPPRODUCTICON'''
$iconRow = First 'SELECT `Name` FROM `Icon` WHERE `Name` = ''PdfcraftIcon.ico'''
$desktopIcon = First 'SELECT `Icon_` FROM `Shortcut` WHERE `Shortcut` = ''DesktopShortcut'''
$startIcon = First 'SELECT `Icon_` FROM `Shortcut` WHERE `Shortcut` = ''StartMenuShortcut'''
$desktopTarget = First 'SELECT `Target` FROM `Shortcut` WHERE `Shortcut` = ''DesktopShortcut'''
$startTarget = First 'SELECT `Target` FROM `Shortcut` WHERE `Shortcut` = ''StartMenuShortcut'''
Require ($arpIcon -eq 'PdfcraftIcon.ico' -and $iconRow -eq 'PdfcraftIcon.ico') 'MSI must embed the supplied red PDF application icon'
Require ($desktopIcon -eq 'PdfcraftIcon.ico' -and $startIcon -eq 'PdfcraftIcon.ico') 'Desktop and Start menu shortcuts must use the supplied red PDF application icon'
Require ($desktopTarget -match 'PdfcraftExe' -and $startTarget -match 'PdfcraftExe') 'Desktop and Start menu shortcuts must launch the installed PDF application'
$friendlyName = First 'SELECT `Value` FROM `Registry` WHERE `Root` = 1 AND `Key` = ''Software\Classes\Applications\pdfcraft.exe'' AND `Name` = ''FriendlyAppName'''
Require ($friendlyName -eq '沃飞翼览 PDF') "Open with label must be 沃飞翼览 PDF, got '$friendlyName'"
$openWithProgId = First 'SELECT `Name` FROM `Registry` WHERE `Root` = 1 AND `Key` = ''Software\Classes\.pdf\OpenWithProgids'' AND `Name` = ''PdfCraft.Document'''
Require ($openWithProgId -eq 'PdfCraft.Document') 'MSI must register PdfCraft.Document in the per-user PDF OpenWithProgids list'
$openCommand = First 'SELECT `Value` FROM `Registry` WHERE `Root` = 1 AND `Key` = ''Software\Classes\Applications\pdfcraft.exe\shell\open\command'''
Require ($openCommand -match 'PdfcraftExe' -and $openCommand -match '%1') 'Open with command must launch the installed executable with the selected PDF'
$registeredApp = First 'SELECT `Value` FROM `Registry` WHERE `Root` = 1 AND `Key` = ''Software\RegisteredApplications'' AND `Name` = ''沃飞翼览 PDF'''
Require ($registeredApp -eq 'Software\PdfCraft\Capabilities') 'Windows registered-app entry must use the custom display name'

$modelsParent = First 'SELECT `Directory_Parent` FROM `Directory` WHERE `Directory` = ''ModelsFolder'''
Require ($modelsParent -eq 'INSTALLFOLDER') 'OCR models must be beside the application'
Require (-not (First 'SELECT `Key` FROM `Registry` WHERE `Root` = 2')) 'HKLM writes are forbidden for a per-user MSI'
Require (-not (First 'SELECT `Name` FROM `_Tables` WHERE `Name` = ''Dialog''')) 'local MSI must have no wizard dialogs'

$payloadKey = First 'SELECT `Registry` FROM `Registry` WHERE `Root` = 1 AND `Name` = ''Payload'''
$componentKey = First 'SELECT `KeyPath` FROM `Component` WHERE `Component` = ''PdfcraftLocalApp'''
Require ($payloadKey -and $componentKey -eq $payloadKey) 'application must use an HKCU registry key path'
$modelsKey = First 'SELECT `Registry` FROM `Registry` WHERE `Root` = 1 AND `Name` = ''Models'''
$modelsComponent = First 'SELECT `KeyPath` FROM `Component` WHERE `Component` = ''PdfcraftLocalModels'''
Require ($modelsKey -and $modelsComponent -eq $modelsKey) 'models must use an HKCU registry key path'
$shortcut = First 'SELECT `Directory_` FROM `Shortcut` WHERE `Shortcut` = ''StartMenuShortcut'''
Require ($shortcut -eq 'ProgramMenuFolder') 'Start menu shortcut must belong to the current user'
$desktopShortcutDir = First 'SELECT `Directory_` FROM `Shortcut` WHERE `Shortcut` = ''DesktopShortcut'''
Require ($desktopShortcutDir -eq 'DesktopFolder') 'MSI must create a current-user desktop shortcut even with /qn'
$desktopReg = First 'SELECT `Registry` FROM `Registry` WHERE `Root` = 1 AND `Name` = ''DesktopShortcut'''
$desktopKey = First 'SELECT `KeyPath` FROM `Component` WHERE `Component` = ''PdfcraftLocalDesktopShortcut'''
Require ($desktopReg -and $desktopKey -eq $desktopReg) 'Desktop shortcut component must use HKCU keypath'

$hasMarker = $false
$modelCount = 0
$requiredLicenseFiles = @('LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE', 'LICENSE-document.txt', 'LICENSE-source.txt', 'LICENSE-lucide.txt', 'OFL-Inter.txt', 'OFL-JetBrainsMono.txt', 'OFL-DancingScript.txt', 'CRAFT-FONTS-ATTRIBUTION.md', 'OFL-ShipporiMincho.txt', 'OFL-BIZ-UDPGothic.txt', 'OFL-BIZ-UDMincho.txt', 'OFL-LXGW-WenKai-TC.txt', 'OFL-NotoSansArabic.txt', 'OFL-NotoSansThai.txt')
$foundLicenseFiles = @{}
$view = $Database.OpenView('SELECT `File`.`FileName`, `Component`.`Directory_` FROM `File`, `Component` WHERE `File`.`Component_` = `Component`.`Component`')
try {
  [void] $view.Execute()
  while ($record = $view.Fetch()) {
    $filename = $record.StringData(1)
    $directory = $record.StringData(2)
    if ($filename -match '(^|\|)portable\.txt$') { $hasMarker = $true }
    foreach ($licenseFile in $requiredLicenseFiles) {
      if ($filename -match ('(^|\|)' + [regex]::Escape($licenseFile) + '$')) { $foundLicenseFiles[$licenseFile] = $true }
    }
    if ($directory -eq 'ModelsFolder' -and $filename -match '\.rten$') { $modelCount++ }
  }
} finally { [void] $view.Close() }
Require $hasMarker 'portable.txt is required so settings/logs stay in LocalAppData'
Require ($modelCount -ge 2) "at least 2 OCR models are required, got $modelCount"
foreach ($licenseFile in $requiredLicenseFiles) {
  Require ($foundLicenseFiles.ContainsKey($licenseFile)) "MSI must package the required license/provenance file $licenseFile"
}
[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Database)
[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Installer)
Write-Output 'ok: no wizard, current-user Desktop and Start Menu shortcuts, HKCU registry, %LOCALAPPDATA%\pdfcraft, portable data, OCR models'
