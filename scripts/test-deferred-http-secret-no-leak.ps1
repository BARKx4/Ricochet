[CmdletBinding()]
param(
    [switch]$ByteBufferSelfTestOnly,
    [switch]$ArtifactSelectionSelfTestOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$runId = '{0}-{1}' -f ([DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')), ([Guid]::NewGuid().ToString('N'))
$evidenceRoot = Join-Path $root 'target\security-evidence'
$applicationEvidence = Join-Path $evidenceRoot ("deferred-http-{0}" -f $runId)
$selfTestEvidence = Join-Path $evidenceRoot ("deferred-http-scanner-self-test-{0}" -f $runId)
$utf8 = New-Object System.Text.UTF8Encoding($false)
$auditPipeEnvironment = 'RICOCHET_DEFERRED_HTTP_AUDIT_PIPE_HANDLE'
$secretBuffer = $null
$secretDigest = $null
$secretDigestTextLower = $null
$secretDigestTextUpper = $null
$selfTestRaw = $null
$selfTestDigest = $null
$selfTestDigestTextLower = $null
$selfTestDigestTextUpper = $null
$pipe = $null

function Clear-Bytes {
    param([byte[]]$Buffer)
    if ($null -ne $Buffer) {
        [Array]::Clear($Buffer, 0, $Buffer.Length)
    }
}

function Assert-ByteArray {
    param(
        [object]$Buffer,
        [string]$Label
    )
    if ($null -eq $Buffer -or $Buffer.GetType() -ne [byte[]]) {
        throw ("{0} must retain the System.Byte[] runtime type" -f $Label)
    }
}

function Assert-ClearedBytes {
    param(
        [object]$Buffer,
        [string]$Label
    )
    Assert-ByteArray $Buffer $Label
    if (@($Buffer | Where-Object { $_ -ne 0 }).Count -ne 0) {
        throw ("{0} was not zeroized in place" -f $Label)
    }
}

function Read-ExactBytes {
    param(
        [System.IO.Stream]$Stream,
        [int]$Count,
        [System.Exception]$InjectedFailure = $null,
        [hashtable]$ClearedFailureCapture = $null
    )
    $buffer = New-Object byte[] $Count
    $offset = 0
    $completed = $false
    try {
        while ($offset -lt $Count) {
            $read = $Stream.Read($buffer, $offset, $Count - $offset)
            if ($read -le 0) {
                throw 'anonymous audit pipe closed before the checked frame was complete'
            }
            $offset += $read
        }
        if ($null -ne $InjectedFailure) {
            throw $InjectedFailure
        }
        $completed = $true
        return ,$buffer
    }
    finally {
        if (-not $completed) {
            Clear-Bytes $buffer
            if ($null -ne $ClearedFailureCapture) {
                $ClearedFailureCapture.Buffer = [object]$buffer
            }
        }
    }
}

function Start-CapturedProcess {
    param(
        [string]$FileName,
        [string]$Arguments,
        [int]$TimeoutMilliseconds = 900000,
        [hashtable]$Environment = @{}
    )
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $FileName
    $start.Arguments = $Arguments
    $start.WorkingDirectory = $root
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($entry in $Environment.GetEnumerator()) {
        $start.EnvironmentVariables[$entry.Key] = [string]$entry.Value
    }
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $start
    if (-not $process.Start()) {
        throw "failed to start $FileName"
    }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($TimeoutMilliseconds)) {
        $process.Kill()
        throw "process timed out: $FileName $Arguments"
    }
    $stdout = $stdoutTask.Result
    $stderr = $stderrTask.Result
    $exitCode = $process.ExitCode
    $process.Dispose()
    return [pscustomobject]@{
        ExitCode = $exitCode
        Stdout = $stdout
        Stderr = $stderr
    }
}

function Test-JsonProperty {
    param(
        [object]$Object,
        [string]$Name
    )
    return $null -ne $Object -and $null -ne $Object.PSObject.Properties[$Name]
}

function Test-SamePath {
    param(
        [string]$Left,
        [string]$Right
    )
    if ([string]::IsNullOrWhiteSpace($Left) -or [string]::IsNullOrWhiteSpace($Right)) {
        return $false
    }
    $comparison = if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [System.Runtime.InteropServices.OSPlatform]::Windows
    )) {
        [System.StringComparison]::OrdinalIgnoreCase
    }
    else {
        [System.StringComparison]::Ordinal
    }
    return [string]::Equals(
        [System.IO.Path]::GetFullPath($Left),
        [System.IO.Path]::GetFullPath($Right),
        $comparison
    )
}

function Resolve-CargoTargetRootFromMetadataJson {
    param([string]$MetadataJson)
    try {
        $metadata = $MetadataJson | ConvertFrom-Json -ErrorAction Stop
    }
    catch {
        throw 'Cargo metadata output was not valid JSON'
    }
    if (-not (Test-JsonProperty $metadata 'target_directory') -or
        [string]::IsNullOrWhiteSpace([string]$metadata.target_directory)) {
        throw 'Cargo metadata did not provide target_directory'
    }
    return [System.IO.Path]::GetFullPath([string]$metadata.target_directory)
}

function Get-CargoTargetRoot {
    $rtkCommand = Get-Command rtk -ErrorAction Stop
    $metadata = Start-CapturedProcess $rtkCommand.Source 'proxy cargo metadata --format-version=1 --no-deps' 120000
    if ($metadata.ExitCode -ne 0) {
        throw 'Cargo metadata failed while resolving the configured target directory'
    }
    return Resolve-CargoTargetRootFromMetadataJson $metadata.Stdout
}

function Test-CargoArtifactLayout {
    param(
        [string]$Executable,
        [string]$TargetRoot,
        [string]$ProfileDirectoryName,
        [bool]$InDepsDirectory
    )
    $artifactDirectory = [System.IO.Path]::GetDirectoryName(
        [System.IO.Path]::GetFullPath($Executable)
    )
    if ($InDepsDirectory) {
        if ([System.IO.Path]::GetFileName($artifactDirectory) -cne 'deps') {
            return $false
        }
        $profileDirectory = [System.IO.Path]::GetDirectoryName($artifactDirectory)
    }
    else {
        $profileDirectory = $artifactDirectory
    }
    if ([System.IO.Path]::GetFileName($profileDirectory) -cne $ProfileDirectoryName) {
        return $false
    }
    $layoutRoot = [System.IO.Path]::GetDirectoryName($profileDirectory)
    if (Test-SamePath $layoutRoot $TargetRoot) {
        return $true
    }
    return Test-SamePath ([System.IO.Path]::GetDirectoryName($layoutRoot)) $TargetRoot
}

function Resolve-CargoTestExecutableFromJson {
    param(
        [string]$JsonLines,
        [string]$ExpectedTargetName,
        [string]$ExpectedManifestPath,
        [string]$ExpectedSourcePath,
        [string]$TargetRoot
    )
    $matches = @()
    foreach ($line in $JsonLines -split "`r?`n") {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        try {
            $record = $line | ConvertFrom-Json -ErrorAction Stop
        }
        catch {
            throw 'Cargo emitted non-JSON stdout while selecting the deferred HTTP audit artifact'
        }
        if (-not (Test-JsonProperty $record 'reason') -or $record.reason -ne 'compiler-artifact') {
            continue
        }
        if (-not (Test-JsonProperty $record 'manifest_path') -or
            -not (Test-SamePath ([string]$record.manifest_path) $ExpectedManifestPath) -or
            -not (Test-JsonProperty $record 'target') -or
            -not (Test-JsonProperty $record.target 'name') -or
            [string]$record.target.name -cne $ExpectedTargetName -or
            -not (Test-JsonProperty $record.target 'src_path') -or
            -not (Test-SamePath ([string]$record.target.src_path) $ExpectedSourcePath)) {
            continue
        }
        $targetKinds = @()
        if (Test-JsonProperty $record.target 'kind') {
            $targetKinds = @($record.target.kind)
        }
        $crateTypes = @()
        if (Test-JsonProperty $record.target 'crate_types') {
            $crateTypes = @($record.target.crate_types)
        }
        $features = @('__missing__')
        if (Test-JsonProperty $record 'features') {
            $features = @($record.features)
        }
        if ($targetKinds.Count -ne 1 -or [string]$targetKinds[0] -cne 'test' -or
            $crateTypes.Count -ne 1 -or [string]$crateTypes[0] -cne 'bin' -or
            -not (Test-JsonProperty $record.target 'test') -or -not [bool]$record.target.test -or
            -not (Test-JsonProperty $record 'profile') -or
            -not (Test-JsonProperty $record.profile 'test') -or -not [bool]$record.profile.test -or
            $features.Count -ne 0 -or
            -not (Test-JsonProperty $record 'executable') -or
            [string]::IsNullOrWhiteSpace([string]$record.executable)) {
            continue
        }
        $matches += ,$record
    }
    if ($matches.Count -eq 0) {
        throw 'Cargo did not emit the exact deferred HTTP audit test artifact'
    }
    if ($matches.Count -ne 1) {
        throw 'Cargo emitted ambiguous deferred HTTP audit test artifacts'
    }

    $artifact = $matches[0]
    $executable = [System.IO.Path]::GetFullPath([string]$artifact.executable)
    if (-not (Test-CargoArtifactLayout $executable $TargetRoot 'debug' $true)) {
        throw 'Cargo selected a deferred HTTP audit artifact outside the exact target directory'
    }
    $expectedFileName = if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [System.Runtime.InteropServices.OSPlatform]::Windows
    )) {
        '^deferred_http_secret_no_leak-[0-9a-f]{16}\.exe$'
    }
    else {
        '^deferred_http_secret_no_leak-[0-9a-f]{16}$'
    }
    if ([System.IO.Path]::GetFileName($executable) -cnotmatch $expectedFileName) {
        throw 'Cargo selected a sidecar or unexpected deferred HTTP audit artifact'
    }
    if (-not (Test-JsonProperty $artifact 'filenames') -or
        @($artifact.filenames | Where-Object { Test-SamePath ([string]$_) $executable }).Count -ne 1) {
        throw 'Cargo executable was not present exactly once in its artifact filename set'
    }
    if (-not [System.IO.File]::Exists($executable)) {
        throw 'Cargo selected deferred HTTP audit executable does not exist'
    }
    return $executable
}

function Resolve-CargoReleaseExecutableFromJson {
    param(
        [string]$JsonLines,
        [string]$ExpectedManifestPath,
        [string]$TargetRoot
    )
    $matches = @()
    foreach ($line in $JsonLines -split "`r?`n") {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        try {
            $record = $line | ConvertFrom-Json -ErrorAction Stop
        }
        catch {
            throw 'Cargo emitted non-JSON stdout while selecting the release package artifact'
        }
        if (-not (Test-JsonProperty $record 'reason') -or $record.reason -ne 'compiler-artifact' -or
            -not (Test-JsonProperty $record 'manifest_path') -or
            -not (Test-SamePath ([string]$record.manifest_path) $ExpectedManifestPath) -or
            -not (Test-JsonProperty $record 'target') -or
            -not (Test-JsonProperty $record.target 'name') -or [string]$record.target.name -cne 'rco') {
            continue
        }
        $targetKinds = @()
        if (Test-JsonProperty $record.target 'kind') {
            $targetKinds = @($record.target.kind)
        }
        $crateTypes = @()
        if (Test-JsonProperty $record.target 'crate_types') {
            $crateTypes = @($record.target.crate_types)
        }
        $features = @('__missing__')
        if (Test-JsonProperty $record 'features') {
            $features = @($record.features)
        }
        if ($targetKinds.Count -ne 1 -or [string]$targetKinds[0] -cne 'bin' -or
            $crateTypes.Count -ne 1 -or [string]$crateTypes[0] -cne 'bin' -or
            -not (Test-JsonProperty $record.target 'test') -or
            -not (Test-JsonProperty $record 'profile') -or
            -not (Test-JsonProperty $record.profile 'test') -or [bool]$record.profile.test -or
            $features.Count -ne 0 -or
            -not (Test-JsonProperty $record 'executable') -or
            [string]::IsNullOrWhiteSpace([string]$record.executable)) {
            continue
        }
        $matches += ,$record
    }
    if ($matches.Count -eq 0) {
        throw 'Cargo did not emit the exact release package artifact'
    }
    if ($matches.Count -ne 1) {
        throw 'Cargo emitted ambiguous release package artifacts'
    }

    $artifact = $matches[0]
    $executable = [System.IO.Path]::GetFullPath([string]$artifact.executable)
    if (-not (Test-CargoArtifactLayout $executable $TargetRoot 'release' $false)) {
        throw 'Cargo selected a release package artifact outside the exact target directory'
    }
    $expectedFileName = if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [System.Runtime.InteropServices.OSPlatform]::Windows
    )) {
        '^rco\.exe$'
    }
    else {
        '^rco$'
    }
    if ([System.IO.Path]::GetFileName($executable) -cnotmatch $expectedFileName) {
        throw 'Cargo selected an unexpected release package artifact'
    }
    if (-not (Test-JsonProperty $artifact 'filenames') -or
        @($artifact.filenames | Where-Object { Test-SamePath ([string]$_) $executable }).Count -ne 1) {
        throw 'Cargo release executable was not present exactly once in its artifact filename set'
    }
    if (-not [System.IO.File]::Exists($executable)) {
        throw 'Cargo selected release package executable does not exist'
    }
    return $executable
}

function Assert-ArtifactSelectionRejects {
    param(
        [scriptblock]$Action,
        [string]$Label
    )
    try {
        & $Action
    }
    catch {
        return
    }
    throw ("artifact selection self-test accepted {0}" -f $Label)
}

function Invoke-ArtifactSelectionSelfCheck {
    $rtkCommand = Get-Command rtk -ErrorAction Stop
    $compile = Start-CapturedProcess $rtkCommand.Source 'proxy cargo test -p ricochet_cli --test deferred_http_secret_no_leak --no-run --message-format=json'
    if ($compile.ExitCode -ne 0) {
        throw ("artifact selection self-test compilation failed: {0}" -f $compile.Stderr.Trim())
    }
    $targetRoot = Get-CargoTargetRoot
    $metadataTargetRoot = Resolve-CargoTargetRootFromMetadataJson (
        @{ target_directory = $targetRoot } | ConvertTo-Json -Compress
    )
    if (-not (Test-SamePath $metadataTargetRoot $targetRoot)) {
        throw 'artifact selection self-test did not preserve Cargo metadata target_directory'
    }
    $targetName = 'deferred_http_secret_no_leak'
    $manifestPath = Join-Path $root 'crates\ricochet_cli\Cargo.toml'
    $sourcePath = Join-Path $root 'crates\ricochet_cli\tests\deferred_http_secret_no_leak.rs'
    $executable = Resolve-CargoTestExecutableFromJson $compile.Stdout $targetName $manifestPath $sourcePath $targetRoot
    $syntheticTripleTargetRoot = [System.IO.Path]::GetDirectoryName($targetRoot)
    $tripleExecutable = Resolve-CargoTestExecutableFromJson $compile.Stdout $targetName $manifestPath $sourcePath $syntheticTripleTargetRoot
    if (-not (Test-SamePath $tripleExecutable $executable)) {
        throw 'artifact selection self-test changed the Cargo executable in a target-triple layout'
    }
    $releaseFileName = if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [System.Runtime.InteropServices.OSPlatform]::Windows
    )) { 'rco.exe' } else { 'rco' }
    $directReleasePath = Join-Path (Join-Path $targetRoot 'release') $releaseFileName
    $tripleReleasePath = Join-Path (Join-Path (Join-Path $targetRoot 'synthetic-target-triple') 'release') $releaseFileName
    if (-not (Test-CargoArtifactLayout $directReleasePath $targetRoot 'release' $false) -or
        -not (Test-CargoArtifactLayout $tripleReleasePath $targetRoot 'release' $false)) {
        throw 'artifact selection self-test rejected a release package layout'
    }

    $exactLines = @()
    foreach ($line in $compile.Stdout -split "`r?`n") {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        $record = $line | ConvertFrom-Json -ErrorAction Stop
        if ((Test-JsonProperty $record 'reason') -and $record.reason -eq 'compiler-artifact' -and
            (Test-JsonProperty $record 'manifest_path') -and
            (Test-SamePath ([string]$record.manifest_path) $manifestPath) -and
            (Test-JsonProperty $record 'target') -and
            (Test-JsonProperty $record.target 'name') -and
            [string]$record.target.name -ceq $targetName -and
            (Test-JsonProperty $record.target 'src_path') -and
            (Test-SamePath ([string]$record.target.src_path) $sourcePath)) {
            $exactLines += ,$line
        }
    }
    if ($exactLines.Count -ne 1) {
        throw 'artifact selection self-test could not isolate one exact Cargo record'
    }
    $exactLine = $exactLines[0]

    Assert-ArtifactSelectionRejects {
        Resolve-CargoTestExecutableFromJson '{"reason":"build-finished","success":true}' $targetName $manifestPath $sourcePath $targetRoot
    } 'a missing artifact'
    Assert-ArtifactSelectionRejects {
        Resolve-CargoTestExecutableFromJson ("{0}`n{0}" -f $exactLine) $targetName $manifestPath $sourcePath $targetRoot
    } 'ambiguous artifacts'

    $wrongTargetRecord = $exactLine | ConvertFrom-Json -ErrorAction Stop
    $wrongTargetRecord.target.name = 'wrong_deferred_http_target'
    $wrongTargetJson = $wrongTargetRecord | ConvertTo-Json -Compress -Depth 20
    Assert-ArtifactSelectionRejects {
        Resolve-CargoTestExecutableFromJson $wrongTargetJson $targetName $manifestPath $sourcePath $targetRoot
    } 'a wrong target'

    $sidecarRecord = $exactLine | ConvertFrom-Json -ErrorAction Stop
    $sidecarPath = Join-Path (Join-Path $targetRoot 'debug\deps') 'deferred_http_secret_no_leak-0000000000000000.pdb'
    $sidecarRecord.executable = $sidecarPath
    $sidecarRecord.filenames = @($sidecarPath)
    $sidecarJson = $sidecarRecord | ConvertTo-Json -Compress -Depth 20
    Assert-ArtifactSelectionRejects {
        Resolve-CargoTestExecutableFromJson $sidecarJson $targetName $manifestPath $sourcePath $targetRoot
    } 'a sidecar artifact'

    return [System.IO.Path]::GetFileName($executable)
}

function Write-EvidenceText {
    param(
        [string]$Path,
        [string]$Value
    )
    [System.IO.File]::WriteAllText($Path, $Value, $utf8)
}

function Count-Pattern {
    param(
        [byte[]]$Haystack,
        [byte[]]$Needle
    )
    if ($Needle.Length -eq 0 -or $Haystack.Length -lt $Needle.Length) {
        return 0
    }
    $count = 0
    for ($index = 0; $index -le $Haystack.Length - $Needle.Length; $index++) {
        $matched = $true
        for ($needleIndex = 0; $needleIndex -lt $Needle.Length; $needleIndex++) {
            if ($Haystack[$index + $needleIndex] -ne $Needle[$needleIndex]) {
                $matched = $false
                break
            }
        }
        if ($matched) {
            $count++
            $index += $Needle.Length - 1
        }
    }
    return $count
}

function ConvertTo-HexBytes {
    param(
        [byte[]]$Buffer,
        [bool]$Uppercase,
        [System.Exception]$InjectedFailure = $null,
        [hashtable]$ClearedFailureCapture = $null
    )
    $letterBase = if ($Uppercase) { 55 } else { 87 }
    $hex = New-Object byte[] ($Buffer.Length * 2)
    $completed = $false
    try {
        for ($index = 0; $index -lt $Buffer.Length; $index++) {
            $high = ($Buffer[$index] -shr 4) -band 0x0f
            $low = $Buffer[$index] -band 0x0f
            $hex[$index * 2] = if ($high -lt 10) { 48 + $high } else { $letterBase + $high }
            $hex[($index * 2) + 1] = if ($low -lt 10) { 48 + $low } else { $letterBase + $low }
        }
        if ($null -ne $InjectedFailure) {
            throw $InjectedFailure
        }
        $completed = $true
        return ,$hex
    }
    finally {
        if (-not $completed) {
            Clear-Bytes $hex
            if ($null -ne $ClearedFailureCapture) {
                $ClearedFailureCapture.Buffer = [object]$hex
            }
        }
    }
}

function Invoke-ByteBufferSelfCheck {
    $source = [byte[]](0x11, 0x22, 0x33, 0x44)
    $stream = New-Object System.IO.MemoryStream(, $source)
    $raw = $null
    $digest = $null
    $lower = $null
    $upper = $null
    try {
        $raw = Read-ExactBytes $stream $source.Length
        Assert-ByteArray $raw 'read buffer self-check'
        $sha = [System.Security.Cryptography.SHA256]::Create()
        try {
            $digest = $sha.ComputeHash($raw)
        }
        finally {
            $sha.Dispose()
        }
        $lower = ConvertTo-HexBytes $digest $false
        $upper = ConvertTo-HexBytes $digest $true
        Assert-ByteArray $lower 'lowercase digest self-check'
        Assert-ByteArray $upper 'uppercase digest self-check'
    }
    finally {
        $stream.Dispose()
        Clear-Bytes $source
        Clear-Bytes $raw
        Clear-Bytes $digest
        Clear-Bytes $lower
        Clear-Bytes $upper
    }
    Assert-ClearedBytes $source 'source self-check'
    Assert-ClearedBytes $raw 'read buffer self-check'
    Assert-ClearedBytes $digest 'digest self-check'
    Assert-ClearedBytes $lower 'lowercase digest self-check'
    Assert-ClearedBytes $upper 'uppercase digest self-check'

    foreach ($case in @(
        @{ Exception = New-Object System.InvalidOperationException('injected failure'); Label = 'failure' },
        @{ Exception = New-Object System.OperationCanceledException('injected cancellation'); Label = 'cancellation' }
    )) {
        $readSource = [byte[]](0xaa, 0xbb, 0xcc, 0xdd)
        $failureStream = New-Object System.IO.MemoryStream(, $readSource)
        $observedReadBuffer = @{ Buffer = $null }
        $readThrew = $false
        try {
            $null = Read-ExactBytes `
                -Stream $failureStream `
                -Count $readSource.Length `
                -InjectedFailure $case.Exception `
                -ClearedFailureCapture $observedReadBuffer
        }
        catch [System.InvalidOperationException] {
            $readThrew = $true
        }
        catch [System.OperationCanceledException] {
            $readThrew = $true
        }
        finally {
            $failureStream.Dispose()
            Clear-Bytes $readSource
        }
        if (-not $readThrew) {
            throw ("read helper did not execute its injected {0} path" -f $case.Label)
        }
        Assert-ClearedBytes $observedReadBuffer.Buffer ("read helper {0}-path buffer" -f $case.Label)

        $hexSource = [byte[]](0x01, 0x23, 0x45, 0x67)
        $observedHexBuffer = @{ Buffer = $null }
        $hexThrew = $false
        try {
            $null = ConvertTo-HexBytes `
                -Buffer $hexSource `
                -Uppercase $false `
                -InjectedFailure $case.Exception `
                -ClearedFailureCapture $observedHexBuffer
        }
        catch [System.InvalidOperationException] {
            $hexThrew = $true
        }
        catch [System.OperationCanceledException] {
            $hexThrew = $true
        }
        finally {
            Clear-Bytes $hexSource
        }
        if (-not $hexThrew) {
            throw ("hex helper did not execute its injected {0} path" -f $case.Label)
        }
        Assert-ClearedBytes $observedHexBuffer.Buffer ("hex helper {0}-path buffer" -f $case.Label)
    }
}

function Scan-Files {
    param(
        [string[]]$Paths,
        [byte[]]$RawPattern,
        [byte[]]$DigestPattern,
        [byte[]]$DigestTextLowerPattern,
        [byte[]]$DigestTextUpperPattern
    )
    $rawHits = 0
    $digestHits = 0
    $filesScanned = 0
    foreach ($path in $Paths | Sort-Object -Unique) {
        if (-not [System.IO.File]::Exists($path)) {
            continue
        }
        $contents = $null
        try {
            $contents = [System.IO.File]::ReadAllBytes($path)
            $filesScanned++
            $rawHits += Count-Pattern $contents $RawPattern
            $digestHits += Count-Pattern $contents $DigestPattern
            $digestHits += Count-Pattern $contents $DigestTextLowerPattern
            $digestHits += Count-Pattern $contents $DigestTextUpperPattern
        }
        finally {
            Clear-Bytes $contents
        }
    }
    return [pscustomobject]@{
        Files = $filesScanned
        RawHits = $rawHits
        DigestHits = $digestHits
    }
}

if ($ByteBufferSelfTestOnly) {
    Invoke-ByteBufferSelfCheck
    Write-Output 'byte_buffer_self_check=success;runtime_type=System.Byte[];success_zeroized=5;failure_zeroized=2;cancellation_zeroized=2'
    return
}

if ($ArtifactSelectionSelfTestOnly) {
    $selectedArtifact = Invoke-ArtifactSelectionSelfCheck
    Write-Output ("artifact_selection_self_check=success;exact_artifact={0};metadata_target_directory=1;test_target_triple_layout=1;release_target_triple_layout=1;missing_rejected=1;ambiguous_rejected=1;wrong_target_rejected=1;sidecar_rejected=1" -f $selectedArtifact)
    return
}

try {
    [System.IO.Directory]::CreateDirectory($applicationEvidence) | Out-Null
    [System.IO.Directory]::CreateDirectory($selfTestEvidence) | Out-Null
    Invoke-ByteBufferSelfCheck

    $rtkCommand = Get-Command rtk -ErrorAction Stop
    $rtk = $rtkCommand.Source
    $compile = Start-CapturedProcess $rtk 'proxy cargo test -p ricochet_cli --test deferred_http_secret_no_leak --no-run --message-format=json'
    Write-EvidenceText (Join-Path $applicationEvidence 'test-build.stdout.txt') $compile.Stdout
    Write-EvidenceText (Join-Path $applicationEvidence 'test-build.stderr.txt') $compile.Stderr
    if ($compile.ExitCode -ne 0) {
        throw 'deferred HTTP audit child compilation failed; retained output contains diagnostics'
    }

    $targetRoot = Get-CargoTargetRoot
    $testBinary = Resolve-CargoTestExecutableFromJson $compile.Stdout `
        'deferred_http_secret_no_leak' `
        (Join-Path $root 'crates\ricochet_cli\Cargo.toml') `
        (Join-Path $root 'crates\ricochet_cli\tests\deferred_http_secret_no_leak.rs') `
        $targetRoot

    $pipe = New-Object System.IO.Pipes.AnonymousPipeServerStream(
        [System.IO.Pipes.PipeDirection]::In,
        [System.IO.HandleInheritability]::Inheritable
    )
    $clientHandle = $pipe.GetClientHandleAsString()
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $testBinary
    $start.Arguments = 'scanner_child_writes_secret_only_to_inherited_anonymous_pipe --ignored --exact --nocapture'
    $start.WorkingDirectory = $root
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.EnvironmentVariables[$auditPipeEnvironment] = $clientHandle
    $child = New-Object System.Diagnostics.Process
    $child.StartInfo = $start
    if (-not $child.Start()) {
        throw 'failed to start the deferred HTTP audit child'
    }
    $childStdoutTask = $child.StandardOutput.ReadToEndAsync()
    $childStderrTask = $child.StandardError.ReadToEndAsync()
    $pipe.DisposeLocalCopyOfClientHandle()
    $lengthPrefix = Read-ExactBytes $pipe 4
    try {
        $length = [BitConverter]::ToUInt32($lengthPrefix, 0)
    }
    finally {
        Clear-Bytes $lengthPrefix
    }
    if ($length -lt 32 -or $length -gt 4096) {
        throw 'anonymous audit pipe supplied an invalid checked frame length'
    }
    $secretBuffer = Read-ExactBytes $pipe ([int]$length)
    if (-not $child.WaitForExit(120000)) {
        $child.Kill()
        throw 'deferred HTTP audit child timed out'
    }
    $childStdout = $childStdoutTask.Result
    $childStderr = $childStderrTask.Result
    $childExit = $child.ExitCode
    $child.Dispose()
    Write-EvidenceText (Join-Path $applicationEvidence 'audit-child.stdout.txt') $childStdout
    Write-EvidenceText (Join-Path $applicationEvidence 'audit-child.stderr.txt') $childStderr
    if ($childExit -ne 0) {
        throw 'deferred HTTP audit child failed; retained output contains sanitized diagnostics'
    }

    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $secretDigest = $sha256.ComputeHash($secretBuffer)
        $secretDigestTextLower = ConvertTo-HexBytes $secretDigest $false
        $secretDigestTextUpper = ConvertTo-HexBytes $secretDigest $true
    }
    finally {
        $sha256.Dispose()
    }

    $releaseBuild = Start-CapturedProcess $rtk 'proxy cargo build --locked --release -p ricochet_cli --bin rco --message-format=json'
    Write-EvidenceText (Join-Path $applicationEvidence 'release-build.stdout.txt') $releaseBuild.Stdout
    Write-EvidenceText (Join-Path $applicationEvidence 'release-build.stderr.txt') $releaseBuild.Stderr
    if ($releaseBuild.ExitCode -ne 0) {
        throw 'release package build failed; retained output contains diagnostics'
    }
    $rco = Resolve-CargoReleaseExecutableFromJson $releaseBuild.Stdout `
        (Join-Path $root 'crates\ricochet_cli\Cargo.toml') `
        $targetRoot
    $releaseSmoke = Start-CapturedProcess $rco '--version' 120000
    Write-EvidenceText (Join-Path $applicationEvidence 'release-package-smoke.stdout.txt') $releaseSmoke.Stdout
    Write-EvidenceText (Join-Path $applicationEvidence 'release-package-smoke.stderr.txt') $releaseSmoke.Stderr
    if ($releaseSmoke.ExitCode -ne 0) {
        throw 'release package smoke failed; retained output contains diagnostics'
    }

    $listed = Start-CapturedProcess $rtk 'proxy git ls-files --cached --others --exclude-standard' 120000
    Write-EvidenceText (Join-Path $applicationEvidence 'workspace-list.stdout.txt') $listed.Stdout
    Write-EvidenceText (Join-Path $applicationEvidence 'workspace-list.stderr.txt') $listed.Stderr
    if ($listed.ExitCode -ne 0) {
        throw 'workspace file enumeration failed'
    }
    $workspaceFiles = @(
        $listed.Stdout -split "`r?`n" |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            ForEach-Object { [System.IO.Path]::GetFullPath((Join-Path $root $_)) }
    )
    $evidenceFiles = @(Get-ChildItem -LiteralPath $applicationEvidence -File | ForEach-Object FullName)
    $applicationFiles = @($workspaceFiles + $evidenceFiles + $rco)
    $applicationScan = Scan-Files $applicationFiles $secretBuffer $secretDigest $secretDigestTextLower $secretDigestTextUpper
    if ($applicationScan.RawHits -ne 0 -or $applicationScan.DigestHits -ne 0) {
        throw 'deferred HTTP application evidence contains audited secret material'
    }

    $selfPrefix = $utf8.GetBytes('ricochet-scanner-self-test-')
    $selfRandom = New-Object byte[] 32
    $selfRng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $selfRng.GetBytes($selfRandom)
    }
    finally {
        $selfRng.Dispose()
    }
    $selfTestRaw = New-Object byte[] ($selfPrefix.Length + $selfRandom.Length)
    [Array]::Copy($selfPrefix, 0, $selfTestRaw, 0, $selfPrefix.Length)
    [Array]::Copy($selfRandom, 0, $selfTestRaw, $selfPrefix.Length, $selfRandom.Length)
    Clear-Bytes $selfPrefix
    Clear-Bytes $selfRandom
    $selfSha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $selfTestDigest = $selfSha.ComputeHash($selfTestRaw)
        $selfTestDigestTextLower = ConvertTo-HexBytes $selfTestDigest $false
        $selfTestDigestTextUpper = ConvertTo-HexBytes $selfTestDigest $true
    }
    finally {
        $selfSha.Dispose()
    }
    $selfRawPath = Join-Path $selfTestEvidence 'deliberate-raw.fixture.bin'
    $selfDigestPath = Join-Path $selfTestEvidence 'deliberate-sha256.fixture.txt'
    [System.IO.File]::WriteAllBytes($selfRawPath, $selfTestRaw)
    [System.IO.File]::WriteAllBytes($selfDigestPath, $selfTestDigestTextUpper)
    $selfTestScan = Scan-Files @($selfRawPath, $selfDigestPath) $selfTestRaw $selfTestDigest $selfTestDigestTextLower $selfTestDigestTextUpper
    if ($selfTestScan.RawHits -lt 1 -or $selfTestScan.DigestHits -lt 1) {
        throw 'scanner self-test did not detect both deliberate raw and digest fixtures'
    }

    Write-Output ("application_evidence={0}" -f $applicationEvidence)
    Write-Output ("application_files_scanned={0};raw_hits={1};digest_hits={2}" -f $applicationScan.Files, $applicationScan.RawHits, $applicationScan.DigestHits)
    Write-Output ("scanner_self_test_evidence={0}" -f $selfTestEvidence)
    Write-Output ("scanner_self_test_files_scanned={0};raw_hits={1};digest_hits={2}" -f $selfTestScan.Files, $selfTestScan.RawHits, $selfTestScan.DigestHits)
}
finally {
    if ($null -ne $pipe) {
        $pipe.Dispose()
    }
    Clear-Bytes $secretBuffer
    Clear-Bytes $secretDigest
    Clear-Bytes $secretDigestTextLower
    Clear-Bytes $secretDigestTextUpper
    Clear-Bytes $selfTestRaw
    Clear-Bytes $selfTestDigest
    Clear-Bytes $selfTestDigestTextLower
    Clear-Bytes $selfTestDigestTextUpper
    foreach ($bufferCheck in @(
        @{ Buffer = $secretBuffer; Label = 'application raw buffer' },
        @{ Buffer = $secretDigest; Label = 'application digest buffer' },
        @{ Buffer = $secretDigestTextLower; Label = 'application lowercase digest buffer' },
        @{ Buffer = $secretDigestTextUpper; Label = 'application uppercase digest buffer' },
        @{ Buffer = $selfTestRaw; Label = 'self-test raw buffer' },
        @{ Buffer = $selfTestDigest; Label = 'self-test digest buffer' },
        @{ Buffer = $selfTestDigestTextLower; Label = 'self-test lowercase digest buffer' },
        @{ Buffer = $selfTestDigestTextUpper; Label = 'self-test uppercase digest buffer' }
    )) {
        if ($null -ne $bufferCheck.Buffer) {
            Assert-ClearedBytes $bufferCheck.Buffer $bufferCheck.Label
        }
    }
}
