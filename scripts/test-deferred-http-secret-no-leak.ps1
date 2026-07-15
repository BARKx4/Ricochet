[CmdletBinding()]
param()

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

function Read-ExactBytes {
    param(
        [System.IO.Stream]$Stream,
        [int]$Count
    )
    $buffer = New-Object byte[] $Count
    $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.Read($buffer, $offset, $Count - $offset)
        if ($read -le 0) {
            Clear-Bytes $buffer
            throw 'anonymous audit pipe closed before the checked frame was complete'
        }
        $offset += $read
    }
    return $buffer
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
        [bool]$Uppercase
    )
    $letterBase = if ($Uppercase) { 55 } else { 87 }
    $hex = New-Object byte[] ($Buffer.Length * 2)
    for ($index = 0; $index -lt $Buffer.Length; $index++) {
        $high = ($Buffer[$index] -shr 4) -band 0x0f
        $low = $Buffer[$index] -band 0x0f
        $hex[$index * 2] = if ($high -lt 10) { 48 + $high } else { $letterBase + $high }
        $hex[($index * 2) + 1] = if ($low -lt 10) { 48 + $low } else { $letterBase + $low }
    }
    return $hex
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

try {
    [System.IO.Directory]::CreateDirectory($applicationEvidence) | Out-Null
    [System.IO.Directory]::CreateDirectory($selfTestEvidence) | Out-Null

    $rtkCommand = Get-Command rtk -ErrorAction Stop
    $rtk = $rtkCommand.Source
    $compile = Start-CapturedProcess $rtk 'cargo test -p ricochet_cli --test deferred_http_secret_no_leak --no-run'
    Write-EvidenceText (Join-Path $applicationEvidence 'test-build.stdout.txt') $compile.Stdout
    Write-EvidenceText (Join-Path $applicationEvidence 'test-build.stderr.txt') $compile.Stderr
    if ($compile.ExitCode -ne 0) {
        throw 'deferred HTTP audit child compilation failed; retained output contains diagnostics'
    }

    $targetRoot = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        Join-Path $root 'target'
    }
    else {
        [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    }
    $testPattern = if ($env:OS -eq 'Windows_NT') {
        'deferred_http_secret_no_leak-*.exe'
    }
    else {
        'deferred_http_secret_no_leak-*'
    }
    $testBinary = Get-ChildItem -LiteralPath (Join-Path $targetRoot 'debug\deps') -Filter $testPattern -File |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if ($null -eq $testBinary) {
        throw 'compiled deferred HTTP audit child was not found'
    }

    $pipe = New-Object System.IO.Pipes.AnonymousPipeServerStream(
        [System.IO.Pipes.PipeDirection]::In,
        [System.IO.HandleInheritability]::Inheritable
    )
    $clientHandle = $pipe.GetClientHandleAsString()
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $testBinary.FullName
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

    $releaseBuild = Start-CapturedProcess $rtk 'cargo build --locked --release -p ricochet_cli --bin rco'
    Write-EvidenceText (Join-Path $applicationEvidence 'release-build.stdout.txt') $releaseBuild.Stdout
    Write-EvidenceText (Join-Path $applicationEvidence 'release-build.stderr.txt') $releaseBuild.Stderr
    if ($releaseBuild.ExitCode -ne 0) {
        throw 'release package build failed; retained output contains diagnostics'
    }
    $releaseExecutable = if ($env:OS -eq 'Windows_NT') {
        'release\rco.exe'
    }
    else {
        'release/rco'
    }
    $rco = Join-Path $targetRoot $releaseExecutable
    if (-not [System.IO.File]::Exists($rco)) {
        throw 'release package executable was not found'
    }
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
}
