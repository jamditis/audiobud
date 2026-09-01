param(
  [Parameter(Mandatory = $true)]
  [string]$PfxPath,

  [Parameter(Mandatory = $true)]
  [string]$CerPath,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$')]
  [string]$ExecutionId
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module Microsoft.PowerShell.Security -ErrorAction Stop
Import-Module PKI -ErrorAction Stop
if (-not (Get-PSDrive -Name Cert -ErrorAction SilentlyContinue)) {
  throw 'The Cert PowerShell drive is unavailable'
}

$pfxPasswordText = $env:AUDIOBUD_CANDIDATE_PFX_PASSWORD
if (-not $pfxPasswordText) {
  throw 'AUDIOBUD_CANDIDATE_PFX_PASSWORD is required'
}
foreach ($outputPath in @($PfxPath, $CerPath)) {
  if (Test-Path -LiteralPath $outputPath) {
    throw "Certificate output already exists: $outputPath"
  }
}

$certificate = $null
try {
  $certificate = New-SelfSignedCertificate `
    -DnsName 'localhost' `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -FriendlyName "AudioBud disposable updater verification $ExecutionId" `
    -KeyAlgorithm RSA `
    -KeyExportPolicy Exportable `
    -KeyLength 2048 `
    -NotAfter (Get-Date).AddHours(2) `
    -Type SSLServerAuthentication
  $pfxPassword = ConvertTo-SecureString `
    -String $pfxPasswordText `
    -AsPlainText `
    -Force
  Export-PfxCertificate `
    -Cert $certificate `
    -FilePath $PfxPath `
    -Password $pfxPassword | Out-Null
  Export-Certificate `
    -Cert $certificate `
    -FilePath $CerPath `
    -Type CERT | Out-Null
  foreach ($outputPath in @($PfxPath, $CerPath)) {
    if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
      throw "Certificate output was not written: $outputPath"
    }
    if ((Get-Item -LiteralPath $outputPath).Length -le 0) {
      throw "Certificate output is empty: $outputPath"
    }
  }
} finally {
  if ($certificate) {
    $personalPath = "Cert:\CurrentUser\My\$($certificate.Thumbprint)"
    if (Test-Path -LiteralPath $personalPath) {
      Remove-Item -LiteralPath $personalPath -Force -ErrorAction Stop
    }
    if (Test-Path -LiteralPath $personalPath) {
      throw "Certificate remains at $personalPath"
    }
  }
}
