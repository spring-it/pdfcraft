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

$allUsers = First "SELECT `Value` FROM `Property` WHERE `Property` = 'ALLUSERS'"
Require ($allUsers -ne '1') 'ALLUSERS must not select per-machine scope'
$parent = First "SELECT `Directory_Parent` FROM `Directory` WHERE `Directory` = 'INSTALLFOLDER'"
Require ($parent -eq 'LocalAppDataFolder') "INSTALLFOLDER must be under LocalAppDataFolder, got '$parent'"
$name = First "SELECT `DefaultDir` FROM `Directory` WHERE `Directory` = 'INSTALLFOLDER'"
Require ($name -match '(^|\|)pdfcraft$') "install directory must be pdfcraft, got '$name'"
$modelsParent = First "SELECT `Directory_Parent` FROM `Directory` WHERE `Directory` = 'ModelsFolder'"
Require ($modelsParent -eq 'INSTALLFOLDER') 'OCR models must be beside the application'
Require (-not (First 'SELECT `Key` FROM `Registry` WHERE `Root` = 2')) 'HKLM writes are forbidden for a per-user MSI'
Require (-not (First "SELECT `Name` FROM `_Tables` WHERE `Name` = 'Dialog'")) 'local MSI must have no wizard dialogs'

$payloadKey = First "SELECT `Registry` FROM `Registry` WHERE `Root` = 1 AND `Name` = 'Payload'"
$componentKey = First "SELECT `KeyPath` FROM `Component` WHERE `Component` = 'PdfcraftLocalApp'"
Require ($payloadKey -and $componentKey -eq $payloadKey) 'application must use an HKCU registry key path'
$modelsKey = First "SELECT `Registry` FROM `Registry` WHERE `Root` = 1 AND `Name` = 'Models'"
$modelsComponent = First "SELECT `KeyPath` FROM `Component` WHERE `Component` = 'PdfcraftLocalModels'"
Require ($modelsKey -and $modelsComponent -eq $modelsKey) 'models must use an HKCU registry key path'
$shortcut = First "SELECT `Directory_` FROM `Shortcut` WHERE `Shortcut` = 'StartMenuShortcut'"
Require ($shortcut -eq 'ProgramMenuFolder') 'Start menu shortcut must belong to the current user'

$hasMarker = $false
$modelCount = 0
$view = $Database.OpenView('SELECT `File`.`FileName`, `Component`.`Directory_` FROM `File`, `Component` WHERE `File`.`Component_` = `Component`.`Component`')
try {
  [void] $view.Execute()
  while ($record = $view.Fetch()) {
    $filename = $record.StringData(1)
    $directory = $record.StringData(2)
    if ($filename -match '(^|\|)portable\.txt$') { $hasMarker = $true }
    if ($directory -eq 'ModelsFolder' -and $filename -match '\.rten$') { $modelCount++ }
  }
} finally { [void] $view.Close() }
Require $hasMarker 'portable.txt is required so settings/logs stay in LocalAppData'
Require ($modelCount -ge 2) "at least 2 OCR models are required, got $modelCount"
[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Database)
[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Installer)
Write-Output 'ok: no wizard, current-user HKCU registry, %LOCALAPPDATA%\pdfcraft, portable data, OCR models'
