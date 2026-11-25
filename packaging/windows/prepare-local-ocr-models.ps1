param(
  [Parameter(Mandatory = $true)] [string] $Models
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Models = [System.IO.Path]::GetFullPath($Models)

New-Item -ItemType Directory -Force -Path $Models | Out-Null

# The S3 model origin can return HTTP 403 in some runner regions. Seed the exact
# v0.22.0 official model bundle from the public GitHub release, then let xtask
# verify each manifest SHA-256 and licence before packaging.
if (-not (Test-Path (Join-Path $Models 'text-detection.onnx')) -or -not (Test-Path (Join-Path $Models 'text-recognition.onnx'))) {
  $ModelTemp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
  $ModelArchive = Join-Path $ModelTemp 'pdfcraft-0.22.0-windows-x64-portable.zip'
  $ModelExtract = Join-Path $ModelTemp 'pdfcraft-0.22.0-model-bundle'
  if (-not (Test-Path $ModelArchive)) {
    $ModelUrl = 'https://github.com/storytold/pdfcraft/releases/download/v0.22.0/pdfcraft-0.22.0-windows-x64-portable.zip'
    Invoke-WebRequest -Uri $ModelUrl -OutFile $ModelArchive -MaximumRedirection 10
    if (-not (Test-Path $ModelArchive)) { throw 'Could not download the official v0.22.0 model bundle' }
  }
  if (-not (Test-Path $ModelExtract)) {
    Expand-Archive -LiteralPath $ModelArchive -DestinationPath $ModelExtract -Force
  }
  $ModelSource = Get-ChildItem -Path $ModelExtract -Directory -Recurse |
    Where-Object { $_.Name -eq 'models' } | Select-Object -First 1
  if (-not $ModelSource -or -not (Test-Path (Join-Path $ModelSource.FullName 'text-detection.onnx'))) {
    throw 'The official v0.22.0 portable archive did not contain its verified models directory'
  }
  Copy-Item -Path (Join-Path $ModelSource.FullName '*') -Destination $Models -Recurse -Force
}
Push-Location $Root
try {
  cargo xtask models $Models
  if ($LASTEXITCODE -ne 0) { throw "cargo xtask models failed with exit code $LASTEXITCODE" }
} finally { Pop-Location }
Remove-Item -Force (Join-Path $Models '*.part') -ErrorAction SilentlyContinue
if (-not (Test-Path (Join-Path $Models 'ATTRIBUTION.txt')) -or -not (Get-ChildItem (Join-Path $Models '*.LICENCE.txt'))) {
  throw "no OCR models in $Models after cargo xtask models"
}
foreach ($ModelName in @('text-detection.onnx', 'text-recognition.onnx')) {
  if (-not (Test-Path (Join-Path $Models $ModelName))) {
    throw "required verified OCR model is missing: $ModelName"
  }
}
