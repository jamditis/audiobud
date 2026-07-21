[CmdletBinding()]
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [ValidateNotNullOrEmpty()]
  [string] $Path,

  [switch] $TauriNsisUninstaller
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
  throw "Signing input not found: $Path"
}

$resolvedPath = (Get-Item -LiteralPath $Path).FullName
$fileName = [System.IO.Path]::GetFileName($resolvedPath)
$isApplication = $fileName -ieq "audiobud.exe"

if (-not $isApplication -and -not $TauriNsisUninstaller) {
  Write-Output "Skipping Tauri signing input: $fileName"
  exit 0
}

$requiredVariables = @(
  "ARTIFACT_SIGNING_ENDPOINT",
  "ARTIFACT_SIGNING_ACCOUNT_NAME",
  "ARTIFACT_SIGNING_CERTIFICATE_PROFILE_NAME"
)

foreach ($variable in $requiredVariables) {
  $value = [System.Environment]::GetEnvironmentVariable($variable)
  if ([string]::IsNullOrWhiteSpace($value)) {
    throw "Required signing variable is missing: $variable"
  }
}

Import-Module ArtifactSigning -RequiredVersion 0.1.8 -Force -ErrorAction Stop

Invoke-ArtifactSigning `
  -Endpoint $env:ARTIFACT_SIGNING_ENDPOINT `
  -CodeSigningAccountName $env:ARTIFACT_SIGNING_ACCOUNT_NAME `
  -CertificateProfileName $env:ARTIFACT_SIGNING_CERTIFICATE_PROFILE_NAME `
  -Files $resolvedPath `
  -FileDigest SHA256 `
  -TimestampRfc3161 "http://timestamp.acs.microsoft.com" `
  -TimestampDigest SHA256 `
  -Description "AudioBud" `
  -DescriptionUrl "https://audiobud.amditis.tech/" `
  -ExcludeEnvironmentCredential:$true `
  -ExcludeWorkloadIdentityCredential:$true `
  -ExcludeManagedIdentityCredential:$true `
  -ExcludeSharedTokenCacheCredential:$true `
  -ExcludeVisualStudioCredential:$true `
  -ExcludeVisualStudioCodeCredential:$true `
  -ExcludeAzureCliCredential:$false `
  -ExcludeAzurePowerShellCredential:$true `
  -ExcludeAzureDeveloperCliCredential:$true `
  -ExcludeInteractiveBrowserCredential:$true

$signature = Get-AuthenticodeSignature -LiteralPath $resolvedPath
if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
  throw "Invalid Authenticode signature for $resolvedPath`: $($signature.StatusMessage)"
}
if (-not $signature.TimeStamperCertificate) {
  throw "Timestamp certificate missing from $resolvedPath"
}
if ($signature.SignerCertificate.Subject -notlike "*CN=Joseph Amditis*") {
  throw "Unexpected signer for $resolvedPath`: $($signature.SignerCertificate.Subject)"
}

Write-Output "Signed Tauri input: $fileName"
