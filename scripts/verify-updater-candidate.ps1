param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-f]{40}$')]
  [string]$TargetCommit,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^v\d+\.\d+\.\d+$')]
  [string]$TargetTag,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^\d+\.\d+\.\d+$')]
  [string]$TargetVersion,

  [Parameter(Mandatory = $true)]
  [string]$ArchivePath,

  [Parameter(Mandatory = $true)]
  [string]$SignaturePath,

  [Parameter(Mandatory = $true)]
  [string]$PriorInstallerPath,

  [Parameter(Mandatory = $true)]
  [string]$EvidenceDirectory,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^v\d+\.\d+\.\d+$')]
  [string]$PriorTag,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^\d+\.\d+\.\d+$')]
  [string]$PriorVersion,

  [ValidateSet('github-actions', 'local-windows')]
  [string]$ExecutionEnvironment = 'github-actions',

  [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$')]
  [string]$ExecutionId = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-File {
  param([string]$Path, [string]$Label)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw "$Label was not found: $Path"
  }
  if ((Get-Item -LiteralPath $Path).Length -le 0) {
    throw "$Label is empty: $Path"
  }
}

function Get-Sha256 {
  param([string]$Path)
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-DirectoryInventorySha256 {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
    throw "Model directory was not found: $Path"
  }
  $files = @(
    Get-ChildItem -LiteralPath $Path -File -Recurse |
      Sort-Object FullName
  )
  if ($files.Count -eq 0) {
    throw "Model directory is empty: $Path"
  }
  $lines = foreach ($file in $files) {
    $relativePath = [System.IO.Path]::GetRelativePath($Path, $file.FullName)
    $relativePath = $relativePath.Replace('\', '/')
    "$relativePath`t$($file.Length)`t$(Get-Sha256 -Path $file.FullName)"
  }
  $inventory = ($lines -join "`n") + "`n"
  $hasher = [System.Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($inventory)
    return -join ($hasher.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') })
  } finally {
    $hasher.Dispose()
  }
}

function Assert-AudioBudSignature {
  param([string]$Path, [string]$Label)
  $signature = Get-AuthenticodeSignature -LiteralPath $Path
  if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
    throw "$Label signature is invalid: $($signature.StatusMessage)"
  }
  if ($signature.SignerCertificate.Subject -notlike '*CN=Joseph Amditis*') {
    throw "$Label has an unexpected signer: $($signature.SignerCertificate.Subject)"
  }
  if (-not $signature.TimeStamperCertificate) {
    throw "$Label has no Authenticode timestamp certificate"
  }
  return $signature
}

function Stop-AudioBudProcesses {
  Get-Process -Name AudioBud -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 1
}

function Wait-ForUpdaterQuiescence {
  param([string]$TargetVersion)
  $installerProcessName = "AudioBud_${TargetVersion}_x64-setup"
  $deadline = (Get-Date).AddMinutes(3)
  $quietSince = $null
  while ((Get-Date) -lt $deadline) {
    $audioBudProcesses = @(Get-Process -Name AudioBud -ErrorAction SilentlyContinue)
    if ($audioBudProcesses.Count -gt 0) {
      $audioBudProcesses | Stop-Process -Force -ErrorAction SilentlyContinue
      $quietSince = $null
    }
    $installerProcesses = @(
      Get-Process -Name $installerProcessName -ErrorAction SilentlyContinue
    )
    if ($installerProcesses.Count -eq 0 -and $audioBudProcesses.Count -eq 0) {
      if ($null -eq $quietSince) {
        $quietSince = Get-Date
      } elseif (((Get-Date) - $quietSince).TotalSeconds -ge 5) {
        return
      }
    } else {
      $quietSince = $null
    }
    Start-Sleep -Milliseconds 500
  }
  throw "Updater processes did not become quiet after installing $TargetVersion"
}

function Wait-ForPriorState {
  param(
    [System.Diagnostics.Process]$Process,
    [string]$SettingsPath,
    [string]$ModelsPath
  )
  $deadline = (Get-Date).AddSeconds(90)
  while ((Get-Date) -lt $deadline) {
    if ($Process.HasExited) {
      throw "AudioBud exited before it initialized user data: $($Process.ExitCode)"
    }
    if (
      (Test-Path -LiteralPath $SettingsPath -PathType Leaf) -and
      (Test-Path -LiteralPath $ModelsPath -PathType Container)
    ) {
      Start-Sleep -Seconds 3
      return
    }
    Start-Sleep -Seconds 2
  }
  throw "AudioBud did not initialize settings and models within 90 seconds"
}

function Read-AudioFeedbackSetting {
  param([string]$SettingsPath)
  $store = Get-Content -LiteralPath $SettingsPath -Raw | ConvertFrom-Json
  if ($store.PSObject.Properties.Name -notcontains 'settings') {
    throw "Settings store has no settings object"
  }
  if ($store.settings.PSObject.Properties.Name -notcontains 'audio_feedback') {
    throw "Settings store has no audio_feedback value"
  }
  return [bool]$store.settings.audio_feedback
}

function Get-OptionalRegistryStringValue {
  param([psobject]$Values, [string]$Name)
  if ($null -eq $Values) {
    return ''
  }
  $property = $Values.PSObject.Properties[$Name]
  if ($null -eq $property) {
    return ''
  }
  return [string]$property.Value
}

function Normalize-AudioBudInstallDirectory {
  param([string]$Path)
  return $Path.Trim().Trim('"').TrimEnd('\')
}

function Get-AudioBudUninstallRegistryPaths {
  param([string]$InstallDirectory)
  $normalizedInstallDirectory = Normalize-AudioBudInstallDirectory `
    -Path $InstallDirectory
  $roots = @(
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
  )
  return @(
    foreach ($root in $roots) {
      if (-not (Test-Path -LiteralPath $root -PathType Container)) { continue }
      foreach ($key in Get-ChildItem -LiteralPath $root -ErrorAction Stop) {
        $values = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction Stop
        $displayName = Get-OptionalRegistryStringValue `
          -Values $values `
          -Name 'DisplayName'
        $installLocation = Get-OptionalRegistryStringValue `
          -Values $values `
          -Name 'InstallLocation'
        $uninstallString = Get-OptionalRegistryStringValue `
          -Values $values `
          -Name 'UninstallString'
        if (
          $displayName -ceq 'AudioBud' -or
          (Normalize-AudioBudInstallDirectory -Path $installLocation) -ieq `
            $normalizedInstallDirectory -or
          $uninstallString -like "*$InstallDirectory*"
        ) {
          $key.PSPath
        }
      }
    }
  )
}

function Assert-AudioBudUninstallRegistration {
  param(
    [string]$InstallDirectory,
    [string]$ExpectedVersion,
    [string]$Label
  )
  $normalizedInstallDirectory = Normalize-AudioBudInstallDirectory `
    -Path $InstallDirectory
  $registryPaths = @(
    foreach ($registryPath in @(
      Get-AudioBudUninstallRegistryPaths -InstallDirectory $InstallDirectory
    )) {
      $candidateValues = Get-ItemProperty -LiteralPath $registryPath -ErrorAction Stop
      $displayName = Get-OptionalRegistryStringValue `
        -Values $candidateValues `
        -Name 'DisplayName'
      $installLocation = Get-OptionalRegistryStringValue `
        -Values $candidateValues `
        -Name 'InstallLocation'
      if (
        $displayName -ceq 'AudioBud' -and
        (Normalize-AudioBudInstallDirectory -Path $installLocation) -ieq `
          $normalizedInstallDirectory
      ) {
        $registryPath
      }
    }
  )
  if ($registryPaths.Count -eq 0) {
    throw "$Label created no AudioBud uninstall registration"
  }
  if ($registryPaths.Count -ne 1) {
    throw "$Label created multiple uninstall registrations for ${InstallDirectory}: $($registryPaths -join ', ')"
  }

  $values = Get-ItemProperty -LiteralPath $registryPaths[0] -ErrorAction Stop
  $displayName = Get-OptionalRegistryStringValue `
    -Values $values `
    -Name 'DisplayName'
  $displayVersion = Get-OptionalRegistryStringValue `
    -Values $values `
    -Name 'DisplayVersion'
  $installLocation = Get-OptionalRegistryStringValue `
    -Values $values `
    -Name 'InstallLocation'
  $uninstallString = Get-OptionalRegistryStringValue `
    -Values $values `
    -Name 'UninstallString'

  if ($displayName -cne 'AudioBud') {
    throw "$Label registered an unexpected display name: $displayName"
  }
  if ($displayVersion -cne $ExpectedVersion) {
    throw "$Label registered DisplayVersion $displayVersion instead of $ExpectedVersion"
  }
  if (
    (Normalize-AudioBudInstallDirectory -Path $installLocation) -ine `
      $normalizedInstallDirectory
  ) {
    throw "$Label registered an unexpected install location: $installLocation"
  }
  if ($uninstallString -notlike "*$InstallDirectory*uninstall.exe*") {
    throw "$Label registered an unexpected uninstall command: $uninstallString"
  }

  return $registryPaths[0]
}

function Get-AudioBudUpdaterDirectories {
  param([string]$TempRoot, [string]$TargetVersion)
  $directoryPrefix = "AudioBud-${TargetVersion}-updater-"
  return @(
    Get-ChildItem -LiteralPath $TempRoot -Directory -ErrorAction Stop |
      Where-Object {
        $_.Name.StartsWith(
          $directoryPrefix,
          [System.StringComparison]::Ordinal
        )
      } |
      ForEach-Object { $_.FullName }
  )
}

function Get-NewAudioBudUpdaterDirectories {
  param(
    [string]$TempRoot,
    [string]$TargetVersion,
    [string[]]$BaselinePaths
  )
  return @(
    Get-AudioBudUpdaterDirectories `
      -TempRoot $TempRoot `
      -TargetVersion $TargetVersion |
      Where-Object { $BaselinePaths -notcontains $_ }
  )
}

Assert-File -Path $ArchivePath -Label 'Updater archive'
Assert-File -Path $SignaturePath -Label 'Updater signature'
Assert-File -Path $PriorInstallerPath -Label 'Prior installer'

$expectedArchiveName = "AudioBud_${TargetVersion}_x64-setup.nsis.zip"
if ((Split-Path -Leaf $ArchivePath) -cne $expectedArchiveName) {
  throw "Updater archive name does not match ${TargetVersion}: $ArchivePath"
}
$expectedPriorInstaller = "AudioBud_${PriorVersion}_x64-setup.exe"
if ((Split-Path -Leaf $PriorInstallerPath) -cne $expectedPriorInstaller) {
  throw "Prior installer name does not match ${PriorVersion}: $PriorInstallerPath"
}
if ($TargetTag -cne "v$TargetVersion") {
  throw "Target tag $TargetTag does not match version $TargetVersion"
}
if ($PriorTag -cne "v$PriorVersion") {
  throw "Prior tag $PriorTag does not match version $PriorVersion"
}

$signatureValue = (Get-Content -LiteralPath $SignaturePath -Raw).Trim()
if ($signatureValue -notmatch '^[A-Za-z0-9+/]+={0,2}$') {
  throw 'Updater signature must be one base64 line'
}

if ($ExecutionEnvironment -eq 'github-actions') {
  if ($env:GITHUB_ACTIONS -cne 'true') {
    throw 'GitHub Actions execution requires GITHUB_ACTIONS=true'
  }
  foreach ($environmentVariable in @(
    'GITHUB_RUN_ID',
    'GITHUB_RUN_ATTEMPT',
    'GITHUB_SERVER_URL',
    'GITHUB_REPOSITORY',
    'RUNNER_TEMP'
  )) {
    if (-not [Environment]::GetEnvironmentVariable($environmentVariable)) {
      throw "GitHub Actions execution requires $environmentVariable"
    }
  }
  $executionIdValue = "github-$env:GITHUB_RUN_ID-$env:GITHUB_RUN_ATTEMPT"
  $temporaryRoot = $env:RUNNER_TEMP
  $workflowRunUrl = "$env:GITHUB_SERVER_URL/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID"
  $workflowRunAttempt = [int]$env:GITHUB_RUN_ATTEMPT
} else {
  if (-not $ExecutionId) {
    throw 'Local Windows execution requires ExecutionId'
  }
  $executionIdValue = $ExecutionId
  $temporaryRoot = [System.IO.Path]::GetTempPath()
  $workflowRunUrl = $null
  $workflowRunAttempt = $null
}
$verifierScriptSha256 = Get-Sha256 -Path $PSCommandPath
$hostName = $env:COMPUTERNAME
$windowsVersion = [Environment]::OSVersion.VersionString
$hostArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()

if (Test-Path -LiteralPath $EvidenceDirectory) {
  throw "Evidence directory must not already exist: $EvidenceDirectory"
}
New-Item -ItemType Directory -Path $EvidenceDirectory | Out-Null
$evidencePath = Join-Path $EvidenceDirectory 'updater-prepublication-evidence.json'
$diagnosticDirectory = Join-Path $EvidenceDirectory 'diagnostics'
New-Item -ItemType Directory -Path $diagnosticDirectory -Force | Out-Null
$stagePath = Join-Path $diagnosticDirectory 'updater-verification-stage.log'
$errorPath = Join-Path $diagnosticDirectory 'updater-verification-error.log'
$failurePath = Join-Path $EvidenceDirectory 'updater-prepublication-failure.json'
$failureStage = ''

function Write-VerificationStage {
  param([string]$Message)
  $script:failureStage = $Message
  $entry = "$([DateTime]::UtcNow.ToString('o'))`t$Message"
  Write-Output $entry
  Add-Content -LiteralPath $stagePath -Value $entry -Encoding utf8
}

Write-VerificationStage -Message 'Verifier initialized'

$installDirectory = Join-Path $temporaryRoot "audiobud-update-$executionIdValue"
$appDataDirectory = Join-Path $env:APPDATA 'tech.amditis.audiobud'
$settingsPath = Join-Path $appDataDirectory 'settings_store.json'
$modelsDirectory = Join-Path $appDataDirectory 'models'
$modelName = 'moonshine-tiny-streaming-en'
$modelArchiveName = 'moonshine-tiny-streaming-en.tar.gz'
$modelArchivePath = Join-Path $temporaryRoot "$executionIdValue-$modelArchiveName"
$modelDirectory = Join-Path $modelsDirectory $modelName
$modelAssetUrl = "https://github.com/jamditis/audiobud/releases/download/model-assets-v1/$modelArchiveName"
$modelArchiveSha256 = '465addcfca9e86117415677dfdc98b21edc53537210333a3ecdb58509a80abaf'
$readyPath = Join-Path $temporaryRoot "$executionIdValue-candidate-server-ready.json"
$pfxPath = Join-Path $temporaryRoot "$executionIdValue-candidate-localhost.pfx"
$cerPath = Join-Path $temporaryRoot "$executionIdValue-candidate-localhost.cer"
$serverStdout = Join-Path $diagnosticDirectory 'candidate-server.stdout.log'
$serverStderr = Join-Path $diagnosticDirectory 'candidate-server.stderr.log'
$updaterStdout = Join-Path $diagnosticDirectory 'updater.stdout.log'
$updaterStderr = Join-Path $diagnosticDirectory 'updater.stderr.log'
$serverScript = Join-Path $PSScriptRoot 'serve-updater-candidate.mjs'

$certificate = $null
$serverProcess = $null
$verificationError = $null
$cleanupErrors = [System.Collections.Generic.List[string]]::new()
$updaterDirectoriesBefore = @()
$updaterDirectoryBaselineRecorded = $false
$updaterExtractionDirectories = [System.Collections.Generic.List[string]]::new()

try {
  if (Test-Path -LiteralPath $installDirectory) {
    throw "Updater install directory is not clean: $installDirectory"
  }
  if (Test-Path -LiteralPath $appDataDirectory) {
    throw "Updater app-data directory is not clean: $appDataDirectory"
  }
  Remove-Item -LiteralPath $readyPath, $pfxPath, $cerPath -Force -ErrorAction SilentlyContinue

  $certificate = New-SelfSignedCertificate `
    -DnsName 'localhost' `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -FriendlyName 'AudioBud disposable updater verification' `
    -KeyAlgorithm RSA `
    -KeyExportPolicy Exportable `
    -KeyLength 2048 `
    -NotAfter (Get-Date).AddHours(2) `
    -Type SSLServerAuthentication
  $pfxPasswordText = [Guid]::NewGuid().ToString('N')
  $pfxPassword = ConvertTo-SecureString -String $pfxPasswordText -AsPlainText -Force
  Export-PfxCertificate -Cert $certificate -FilePath $pfxPath -Password $pfxPassword | Out-Null
  Export-Certificate -Cert $certificate -FilePath $cerPath -Type CERT | Out-Null
  $rootStore = [System.Security.Cryptography.X509Certificates.X509Store]::new(
    [System.Security.Cryptography.X509Certificates.StoreName]::Root,
    [System.Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser
  )
  try {
    $rootStore.Open(
      [System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite
    )
    $rootStore.Add($certificate)
  } finally {
    $rootStore.Close()
  }
  Get-Item `
    -LiteralPath "Cert:\CurrentUser\Root\$($certificate.Thumbprint)" `
    -ErrorAction Stop | Out-Null

  $env:AUDIOBUD_CANDIDATE_PFX_PASSWORD = $pfxPasswordText
  $pubDate = (Get-Date).ToUniversalTime().ToString('o')
  $serverProcess = Start-Process `
    -FilePath 'node' `
    -ArgumentList @(
      $serverScript,
      '--archive', $ArchivePath,
      '--signature', $SignaturePath,
      '--pfx', $pfxPath,
      '--version', $TargetVersion,
      '--pub-date', $pubDate,
      '--ready', $readyPath
    ) `
    -RedirectStandardOutput $serverStdout `
    -RedirectStandardError $serverStderr `
    -PassThru

  $readyDeadline = (Get-Date).AddSeconds(30)
  while ((Get-Date) -lt $readyDeadline) {
    if ($serverProcess.HasExited) {
      $serverError = if (Test-Path -LiteralPath $serverStderr) {
        Get-Content -LiteralPath $serverStderr -Raw
      } else {
        'No server error output was captured.'
      }
      throw "Candidate server exited early: $serverError"
    }
    if (Test-Path -LiteralPath $readyPath -PathType Leaf) { break }
    Start-Sleep -Milliseconds 500
  }
  Assert-File -Path $readyPath -Label 'Candidate server readiness file'
  $ready = Get-Content -LiteralPath $readyPath -Raw | ConvertFrom-Json
  $manifestUrl = [string]$ready.manifest_url
  if ($manifestUrl -notmatch '^https://localhost:\d+/latest-candidate\.json$') {
    throw "Candidate manifest escaped localhost: $manifestUrl"
  }
  if ([string]$ready.archive_sha256 -cne (Get-Sha256 -Path $ArchivePath)) {
    throw 'Candidate server selected the wrong updater archive bytes'
  }
  $manifest = Invoke-RestMethod `
    -Uri $manifestUrl `
    -ConnectionTimeoutSeconds 30 `
    -OperationTimeoutSeconds 30
  if ($manifest.version -cne $TargetVersion) {
    throw "Candidate manifest describes $($manifest.version), expected $TargetVersion"
  }
  $expectedLocalArchive = "https://localhost:$($ready.port)/$expectedArchiveName"
  if ($manifest.platforms.'windows-x86_64'.url -cne $expectedLocalArchive) {
    throw "Candidate manifest selected an unexpected archive URL"
  }
  Write-VerificationStage -Message 'Private candidate endpoint verified'

  $priorSignature = Assert-AudioBudSignature `
    -Path $PriorInstallerPath `
    -Label "$PriorTag installer"
  Write-VerificationStage -Message "Starting $PriorTag installer"
  $installProcess = Start-Process `
    -FilePath $PriorInstallerPath `
    -ArgumentList @('/S', "/D=$installDirectory") `
    -PassThru
  if (-not $installProcess.WaitForExit(300000)) {
    try {
      $installProcess.Kill($true)
      $null = $installProcess.WaitForExit(30000)
    } catch {
      throw "$PriorTag installer timed out and its process tree could not be stopped: $($_.Exception.Message)"
    }
    throw "$PriorTag installer did not exit within 5 minutes"
  }
  if ($installProcess.ExitCode -ne 0) {
    throw "$PriorTag installation failed with exit code $($installProcess.ExitCode)"
  }
  Write-VerificationStage -Message "$PriorTag installer exited successfully"

  $executable = Join-Path $installDirectory 'AudioBud.exe'
  Assert-File -Path $executable -Label "Installed $PriorTag executable"
  $installedPriorVersion = (Get-Item -LiteralPath $executable).VersionInfo.ProductVersion
  if ($installedPriorVersion -notmatch "^$([regex]::Escape($PriorVersion))(\.0)?$") {
    throw "Expected installed version $PriorVersion, found $installedPriorVersion"
  }

  $priorProcess = Start-Process `
    -FilePath $executable `
    -ArgumentList @('--start-hidden') `
    -PassThru
  Wait-ForPriorState `
    -Process $priorProcess `
    -SettingsPath $settingsPath `
    -ModelsPath $modelsDirectory
  Stop-AudioBudProcesses
  Write-VerificationStage -Message "$PriorTag initialized user data"

  $settingsStore = Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json
  if ($settingsStore.PSObject.Properties.Name -notcontains 'settings') {
    throw "$PriorTag settings store has no settings object"
  }
  if ($settingsStore.settings.PSObject.Properties.Name -notcontains 'audio_feedback') {
    throw "$PriorTag settings store has no audio_feedback value"
  }
  $settingsStore.settings.audio_feedback = $true
  $settingsStore | ConvertTo-Json -Depth 100 -Compress |
    Set-Content -LiteralPath $settingsPath -Encoding utf8 -NoNewline

  Write-VerificationStage -Message 'Downloading preservation model'
  Invoke-WebRequest `
    -Uri $modelAssetUrl `
    -OutFile $modelArchivePath `
    -ConnectionTimeoutSeconds 30 `
    -OperationTimeoutSeconds 60 `
    -MaximumRetryCount 3 `
    -RetryIntervalSec 5
  if ((Get-Sha256 -Path $modelArchivePath) -cne $modelArchiveSha256) {
    throw "Downloaded model archive hash does not match $modelArchiveSha256"
  }
  & "$env:SystemRoot\System32\tar.exe" `
    -xzf $modelArchivePath `
    -C $modelsDirectory
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to extract the pinned $modelName model"
  }
  $modelFileCount = @(
    Get-ChildItem -LiteralPath $modelDirectory -File -Recurse
  ).Count
  if ($modelFileCount -ne 7) {
    throw "Expected seven files in $modelName, found $modelFileCount"
  }
  $modelSha256Before = Get-DirectoryInventorySha256 -Path $modelDirectory
  Write-VerificationStage -Message 'Preservation model prepared'

  $installedRegistryPaths = @(
    Get-AudioBudUninstallRegistryPaths -InstallDirectory $installDirectory
  )
  if ($installedRegistryPaths.Count -eq 0) {
    throw "$PriorTag installation created no AudioBud uninstall registration"
  }
  Assert-AudioBudUninstallRegistration `
    -InstallDirectory $installDirectory `
    -ExpectedVersion $PriorVersion `
    -Label "$PriorTag installation" | Out-Null

  $priorRestart = Start-Process `
    -FilePath $executable `
    -ArgumentList @('--start-hidden') `
    -PassThru
  Start-Sleep -Seconds 10
  if ($priorRestart.HasExited) {
    throw "$PriorTag did not restart with prepared state: $($priorRestart.ExitCode)"
  }
  Stop-AudioBudProcesses
  $settingsValueBefore = Read-AudioFeedbackSetting -SettingsPath $settingsPath
  if (-not $settingsValueBefore) {
    throw "$PriorTag did not preserve the non-default audio_feedback setting"
  }
  if ((Get-DirectoryInventorySha256 -Path $modelDirectory) -cne $modelSha256Before) {
    throw "$PriorTag changed the pinned $modelName model before the update"
  }
  Write-VerificationStage -Message "$PriorTag preservation state prepared"

  $updaterDirectoriesBefore = @(
    Get-AudioBudUpdaterDirectories `
      -TempRoot $env:TEMP `
      -TargetVersion $TargetVersion
  )
  $updaterDirectoryBaselineRecorded = $true

  Write-VerificationStage -Message "Starting update to $TargetTag"
  $updateProcess = Start-Process `
    -FilePath $executable `
    -ArgumentList @(
      '--install-update',
      '--install-update-endpoint', $manifestUrl,
      '--start-hidden',
      '--no-tray'
    ) `
    -RedirectStandardOutput $updaterStdout `
    -RedirectStandardError $updaterStderr `
    -PassThru
  if (-not $updateProcess.WaitForExit(480000)) {
    try {
      $updateProcess.Kill($true)
      $null = $updateProcess.WaitForExit(30000)
    } catch {
      throw "Updater process timed out and its process tree could not be stopped: $($_.Exception.Message)"
    }
    throw 'Updater process did not exit within 8 minutes'
  }
  if ($updateProcess.ExitCode -ne 0) {
    throw "Updater process failed with exit code $($updateProcess.ExitCode)"
  }
  Write-VerificationStage -Message 'Updater process exited successfully'

  $versionDeadline = (Get-Date).AddMinutes(8)
  $installedTargetVersion = ''
  while ((Get-Date) -lt $versionDeadline) {
    if (Test-Path -LiteralPath $executable -PathType Leaf) {
      $installedTargetVersion = (Get-Item -LiteralPath $executable).VersionInfo.ProductVersion
      if ($installedTargetVersion -match "^$([regex]::Escape($TargetVersion))(\.0)?$") {
        break
      }
    }
    Start-Sleep -Seconds 5
  }
  if ($installedTargetVersion -notmatch "^$([regex]::Escape($TargetVersion))(\.0)?$") {
    throw "Updater did not install $TargetVersion; found $installedTargetVersion"
  }
  $targetSignature = Assert-AudioBudSignature `
    -Path $executable `
    -Label "Installed $TargetVersion executable"
  Write-VerificationStage -Message "$TargetTag installation verified"

  Wait-ForUpdaterQuiescence -TargetVersion $TargetVersion
  $newUpdaterDirectories = @(
    Get-NewAudioBudUpdaterDirectories `
      -TempRoot $env:TEMP `
      -TargetVersion $TargetVersion `
      -BaselinePaths $updaterDirectoriesBefore
  )
  foreach ($updaterDirectory in $newUpdaterDirectories) {
    if (-not $updaterExtractionDirectories.Contains($updaterDirectory)) {
      $updaterExtractionDirectories.Add($updaterDirectory)
    }
  }
  if ($newUpdaterDirectories.Count -ne 1) {
    throw "Expected one updater extraction directory, found $($newUpdaterDirectories.Count)"
  }

  $targetProcess = Start-Process `
    -FilePath $executable `
    -ArgumentList @('--start-hidden') `
    -PassThru
  Start-Sleep -Seconds 15
  if ($targetProcess.HasExited) {
    throw "$TargetVersion did not launch after update: $($targetProcess.ExitCode)"
  }
  Stop-AudioBudProcesses
  Write-VerificationStage -Message "$TargetTag launched successfully"

  $settingsValueAfter = Read-AudioFeedbackSetting -SettingsPath $settingsPath
  if (-not $settingsValueAfter) {
    throw "$TargetVersion replaced the prepared audio_feedback setting"
  }
  $modelSha256After = Get-DirectoryInventorySha256 -Path $modelDirectory
  if ($modelSha256After -cne $modelSha256Before) {
    throw "$TargetVersion changed the pinned $modelName model"
  }

  $targetRegistryPaths = @(
    Get-AudioBudUninstallRegistryPaths -InstallDirectory $installDirectory
  )
  if ($targetRegistryPaths.Count -eq 0) {
    throw 'Updated installation created no AudioBud uninstall registration'
  }
  Assert-AudioBudUninstallRegistration `
    -InstallDirectory $installDirectory `
    -ExpectedVersion $TargetVersion `
    -Label 'Updated installation' | Out-Null

  $uninstaller = Join-Path $installDirectory 'uninstall.exe'
  Assert-File -Path $uninstaller -Label 'Updated NSIS uninstaller'
  Write-VerificationStage -Message "Starting $TargetTag uninstaller"
  $uninstallProcess = Start-Process `
    -FilePath $uninstaller `
    -ArgumentList @('/S') `
    -PassThru
  if (-not $uninstallProcess.WaitForExit(300000)) {
    try {
      $uninstallProcess.Kill($true)
      $null = $uninstallProcess.WaitForExit(30000)
    } catch {
      throw "$TargetTag uninstaller timed out and its process tree could not be stopped: $($_.Exception.Message)"
    }
    throw "$TargetTag uninstaller did not exit within 5 minutes"
  }
  if ($uninstallProcess.ExitCode -ne 0) {
    throw "Updated uninstall failed with exit code $($uninstallProcess.ExitCode)"
  }
  Write-VerificationStage -Message "$TargetTag uninstaller exited successfully"
  $uninstallDeadline = (Get-Date).AddSeconds(60)
  while ((Get-Date) -lt $uninstallDeadline -and (Test-Path -LiteralPath $installDirectory)) {
    Start-Sleep -Seconds 2
  }
  if (Test-Path -LiteralPath $installDirectory) {
    throw 'Updated uninstall left the install directory'
  }
  $remainingRegistryPaths = @(
    Get-AudioBudUninstallRegistryPaths -InstallDirectory $installDirectory
  )
  if ($remainingRegistryPaths.Count -gt 0) {
    throw "Updated uninstall left AudioBud registration keys: $($remainingRegistryPaths -join ', ')"
  }
  $shortcutRoots = @(
    [Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory),
    [Environment]::GetFolderPath([Environment+SpecialFolder]::Programs),
    [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonDesktopDirectory),
    [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonPrograms)
  ) | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Container) }
  $remainingShortcuts = @(
    foreach ($shortcutRoot in $shortcutRoots) {
      Get-ChildItem -LiteralPath $shortcutRoot -Filter 'AudioBud.lnk' -File -Recurse
    }
  )
  if ($remainingShortcuts.Count -gt 0) {
    throw "Updated uninstall left AudioBud shortcuts: $($remainingShortcuts.FullName -join ', ')"
  }
  if ((Read-AudioFeedbackSetting -SettingsPath $settingsPath) -ne $settingsValueAfter) {
    throw 'Updated uninstall changed the preserved audio_feedback setting'
  }
  if ((Get-DirectoryInventorySha256 -Path $modelDirectory) -cne $modelSha256After) {
    throw "Updated uninstall changed the preserved $modelName model"
  }

  $evidence = [ordered]@{
    schema_version = 1
    execution_environment = $ExecutionEnvironment
    execution_id = $executionIdValue
    verifier_script_sha256 = $verifierScriptSha256
    host_name = $hostName
    windows_version = $windowsVersion
    host_architecture = $hostArchitecture
    target_tag = $TargetTag
    target_commit = $TargetCommit
    target_version = $TargetVersion
    workflow_run_url = $workflowRunUrl
    workflow_run_attempt = $workflowRunAttempt
    prior_tag = $PriorTag
    prior_version = $PriorVersion
    prior_installer = (Split-Path -Leaf $PriorInstallerPath)
    prior_installer_sha256 = Get-Sha256 -Path $PriorInstallerPath
    prior_installer_signer = $priorSignature.SignerCertificate.Subject
    updater_archive = (Split-Path -Leaf $ArchivePath)
    updater_archive_sha256 = Get-Sha256 -Path $ArchivePath
    settings_value_before = $settingsValueBefore
    settings_value_after = $settingsValueAfter
    model_name = $modelName
    model_file_count = $modelFileCount
    model_sha256_before = $modelSha256Before
    model_sha256_after = $modelSha256After
    installed_version = $installedTargetVersion
    installed_signer = $targetSignature.SignerCertificate.Subject
    uninstall_passed = $true
  }
  $evidence | ConvertTo-Json -Depth 10 |
    Set-Content -LiteralPath $evidencePath -Encoding utf8 -NoNewline
  Assert-File -Path $evidencePath -Label 'Updater prepublication evidence'
  Write-VerificationStage -Message 'Updater prepublication evidence written'
} catch {
  $verificationError = $_
  try {
    $failureEvidence = [ordered]@{
      schema_version = 1
      result = 'failed'
      execution_environment = $ExecutionEnvironment
      execution_id = $executionIdValue
      verifier_script_sha256 = $verifierScriptSha256
      host_name = $hostName
      windows_version = $windowsVersion
      host_architecture = $hostArchitecture
      failure_stage = $failureStage
      error = $verificationError.Exception.Message
      target_tag = $TargetTag
      target_commit = $TargetCommit
      target_version = $TargetVersion
      prior_tag = $PriorTag
      prior_version = $PriorVersion
      workflow_run_url = $workflowRunUrl
      workflow_run_attempt = $workflowRunAttempt
      prior_installer = (Split-Path -Leaf $PriorInstallerPath)
      prior_installer_sha256 = Get-Sha256 -Path $PriorInstallerPath
      updater_archive = (Split-Path -Leaf $ArchivePath)
      updater_archive_sha256 = Get-Sha256 -Path $ArchivePath
    }
    $failureEvidence | ConvertTo-Json -Depth 10 |
      Set-Content -LiteralPath $failurePath -Encoding utf8 -NoNewline
  } catch {
    $cleanupErrors.Add("Failure evidence write failed: $($_.Exception.Message)")
  }
  try {
    $verificationError | Out-String |
      Set-Content -LiteralPath $errorPath -Encoding utf8 -NoNewline
    Write-VerificationStage `
      -Message "Verifier failed: $($verificationError.Exception.Message)"
  } catch {
    $cleanupErrors.Add("Failure diagnostic write failed: $($_.Exception.Message)")
  }
  foreach ($logPath in @($serverStdout, $serverStderr, $updaterStdout, $updaterStderr)) {
    try {
      Write-Host "::group::$logPath"
      if (Test-Path -LiteralPath $logPath -PathType Leaf) {
        Get-Content -LiteralPath $logPath -Raw
      } else {
        Write-Host 'No log was written.'
      }
    } catch {
      $cleanupErrors.Add("Failure log read failed for ${logPath}: $($_.Exception.Message)")
    } finally {
      Write-Host '::endgroup::'
    }
  }
} finally {
  try {
    Write-VerificationStage -Message 'Verifier cleanup started'
  } catch {
    $cleanupErrors.Add("Cleanup start logging failed: $($_.Exception.Message)")
  }
  try {
    Stop-AudioBudProcesses
    if (@(Get-Process -Name AudioBud -ErrorAction SilentlyContinue).Count -gt 0) {
      throw 'AudioBud processes remain after cleanup'
    }
  } catch {
    $cleanupErrors.Add("AudioBud process cleanup failed: $($_.Exception.Message)")
  }

  try {
    if ($serverProcess -and -not $serverProcess.HasExited) {
      Stop-Process -Id $serverProcess.Id -Force -ErrorAction Stop
      Wait-Process -Id $serverProcess.Id -Timeout 10 -ErrorAction SilentlyContinue
    }
    if (
      $serverProcess -and
      (Get-Process -Id $serverProcess.Id -ErrorAction SilentlyContinue)
    ) {
      throw 'Candidate server process remains after cleanup'
    }
  } catch {
    $cleanupErrors.Add("Candidate server cleanup failed: $($_.Exception.Message)")
  }

  if ($updaterDirectoryBaselineRecorded) {
    try {
      $newUpdaterDirectories = @(
        Get-NewAudioBudUpdaterDirectories `
          -TempRoot $env:TEMP `
          -TargetVersion $TargetVersion `
          -BaselinePaths $updaterDirectoriesBefore
      )
      foreach ($updaterDirectory in $newUpdaterDirectories) {
        if (-not $updaterExtractionDirectories.Contains($updaterDirectory)) {
          $updaterExtractionDirectories.Add($updaterDirectory)
        }
      }
    } catch {
      $cleanupErrors.Add("Updater extraction directory discovery failed: $($_.Exception.Message)")
    }
  }

  foreach ($updaterDirectory in @($updaterExtractionDirectories)) {
    try {
      $tempRootFullPath = [System.IO.Path]::GetFullPath($env:TEMP).TrimEnd('\')
      $updaterDirectoryFullPath = [System.IO.Path]::GetFullPath($updaterDirectory)
      $updaterDirectoryParent = [System.IO.Path]::GetDirectoryName(
        $updaterDirectoryFullPath
      ).TrimEnd('\')
      $updaterDirectoryName = [System.IO.Path]::GetFileName(
        $updaterDirectoryFullPath
      )
      if (
        $updaterDirectoryParent -ine $tempRootFullPath -or
        -not $updaterDirectoryName.StartsWith(
          "AudioBud-${TargetVersion}-updater-",
          [System.StringComparison]::Ordinal
        )
      ) {
        throw "Refusing to remove unexpected updater directory: $updaterDirectory"
      }
      if (Test-Path -LiteralPath $updaterDirectory -PathType Container) {
        Remove-Item -LiteralPath $updaterDirectory -Recurse -Force -ErrorAction Stop
      }
      if (Test-Path -LiteralPath $updaterDirectory) {
        throw "Updater extraction directory remains at $updaterDirectory"
      }
    } catch {
      $cleanupErrors.Add("Updater extraction directory cleanup failed: $($_.Exception.Message)")
    }
  }

  foreach ($trustedCertificate in @($certificate)) {
    if ($null -eq $trustedCertificate) { continue }
    $trustedPath = "Cert:\CurrentUser\Root\$($trustedCertificate.Thumbprint)"
    try {
      if (Test-Path -LiteralPath $trustedPath) {
        Remove-Item -LiteralPath $trustedPath -Force -ErrorAction Stop
      }
      if (Test-Path -LiteralPath $trustedPath) {
        throw "Certificate remains at $trustedPath"
      }
    } catch {
      $cleanupErrors.Add("Trusted root certificate cleanup failed: $($_.Exception.Message)")
    }
  }

  foreach ($personalCertificate in @($certificate)) {
    if ($null -eq $personalCertificate) { continue }
    $personalPath = "Cert:\CurrentUser\My\$($personalCertificate.Thumbprint)"
    try {
      if (Test-Path -LiteralPath $personalPath) {
        Remove-Item -LiteralPath $personalPath -Force -ErrorAction Stop
      }
      if (Test-Path -LiteralPath $personalPath) {
        throw "Certificate remains at $personalPath"
      }
    } catch {
      $cleanupErrors.Add("Personal certificate cleanup failed: $($_.Exception.Message)")
    }
  }

  try {
    Remove-Item Env:\AUDIOBUD_CANDIDATE_PFX_PASSWORD -ErrorAction Stop
    if (Test-Path Env:\AUDIOBUD_CANDIDATE_PFX_PASSWORD) {
      throw 'Candidate PFX password remains in the environment'
    }
  } catch {
    if (Test-Path Env:\AUDIOBUD_CANDIDATE_PFX_PASSWORD) {
      $cleanupErrors.Add("Candidate credential cleanup failed: $($_.Exception.Message)")
    }
  }

  foreach ($temporaryPath in @(
    $readyPath,
    $pfxPath,
    $cerPath,
    $modelArchivePath
  )) {
    try {
      if (Test-Path -LiteralPath $temporaryPath) {
        Remove-Item -LiteralPath $temporaryPath -Force -ErrorAction Stop
      }
      if (Test-Path -LiteralPath $temporaryPath) {
        throw "Temporary file remains at $temporaryPath"
      }
    } catch {
      $cleanupErrors.Add("Temporary file cleanup failed: $($_.Exception.Message)")
    }
  }
  try {
    Write-VerificationStage -Message 'Verifier cleanup finished'
  } catch {
    $cleanupErrors.Add("Cleanup finish logging failed: $($_.Exception.Message)")
  }
}

if ($cleanupErrors.Count -gt 0) {
  foreach ($cleanupError in $cleanupErrors) {
    Write-Error -Message $cleanupError -ErrorAction Continue
  }
  if (-not $verificationError) {
    throw "Updater cleanup failed: $($cleanupErrors -join '; ')"
  }
}
if ($verificationError) {
  throw $verificationError
}
