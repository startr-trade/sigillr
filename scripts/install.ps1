# sigillr installer for Windows x86_64.
# Usage: install.ps1 [-Version vX.Y.Z] [-Dir PATH]
# Env: SIGILLR_VERSION, SIGILLR_INSTALL_DIR, SIGILLR_RELEASE_BASE (test hook; file://DIR reads local files,
# and then SIGILLR_VERSION is required).
# Exit codes: 0 ok, 1 failure (including a checksum mismatch: nothing is installed), 2 usage or unsupported platform.
param(
  [string]$Version = $(if ($env:SIGILLR_VERSION) { $env:SIGILLR_VERSION } else { 'latest' }),
  [string]$Dir = $(if ($env:SIGILLR_INSTALL_DIR) { $env:SIGILLR_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\sigillr' })
)
$ErrorActionPreference = 'Stop'
$Base = if ($env:SIGILLR_RELEASE_BASE) { $env:SIGILLR_RELEASE_BASE } else { 'https://github.com/startr-trade/sigillr/releases' }
$Target = 'x86_64-pc-windows-msvc'

function Fail([int]$Code, [string]$Msg) { [Console]::Error.WriteLine("install.ps1: $Msg"); exit $Code }

if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -notin @('AMD64')) {
  Fail 2 "unsupported platform: $env:PROCESSOR_ARCHITECTURE"
}

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("sigillr-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Tmp | Out-Null

try {
  # "latest" follows the releases/latest redirect to its tag.
  if ($Version -eq 'latest') {
    if ($Base.StartsWith('file://')) { Fail 2 'SIGILLR_VERSION is required with a file:// release base' }
    $r = Invoke-WebRequest -Uri "$Base/latest" -UseBasicParsing
    $final = if ($r.BaseResponse.ResponseUri) { $r.BaseResponse.ResponseUri.AbsoluteUri } else { $r.BaseResponse.RequestMessage.RequestUri.AbsoluteUri }
    $Version = $final.TrimEnd('/').Split('/')[-1]
  }
  $Tag = 'v' + $Version.TrimStart('v')
  $Ver = $Tag.Substring(1)
  $Asset = "sigillr-$Ver-$Target.zip"

  function Fetch([string]$Name, [string]$Dest) {
    if ($Base.StartsWith('file://')) {
      Copy-Item -LiteralPath (Join-Path $Base.Substring(7) $Name) -Destination $Dest
    } else {
      $url = "$Base/download/$Tag/$Name"
      if (-not $url.StartsWith('https://')) { Fail 2 "only https downloads are allowed: $url" }
      Invoke-WebRequest -Uri $url -OutFile $Dest -UseBasicParsing
    }
  }

  $zip = Join-Path $Tmp $Asset
  Fetch $Asset $zip
  Fetch "$Asset.sha256" "$zip.sha256"
  $expected = ((Get-Content -LiteralPath "$zip.sha256" -TotalCount 1) -split '\s+')[0].ToLower()
  if (-not $expected) { Fail 1 "$Asset.sha256 is empty" }

  # Nothing is installed unless this passes.
  $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash.ToLower()
  if ($actual -ne $expected) {
    [Console]::Error.WriteLine("install.ps1: checksum mismatch for $Asset")
    [Console]::Error.WriteLine("  expected: $expected")
    [Console]::Error.WriteLine("  actual:   $actual")
    exit 1
  }

  $x = Join-Path $Tmp 'x'
  Expand-Archive -LiteralPath $zip -DestinationPath $x
  $exe = Join-Path $x "sigillr-$Ver-$Target\sigillr.exe"
  if (-not (Test-Path $exe)) { Fail 1 'archive does not contain sigillr.exe' }
  New-Item -ItemType Directory -Force -Path $Dir | Out-Null
  $installed = Join-Path $Dir 'sigillr.exe'
  Copy-Item -LiteralPath $exe -Destination $installed -Force
  Write-Host "installed $installed"
  if (-not (($env:PATH -split ';') -contains $Dir)) {
    Write-Host "note: $Dir is not on your PATH; add it in your user environment variables."
  }
} finally {
  if (Test-Path $Tmp) { Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue }
}
