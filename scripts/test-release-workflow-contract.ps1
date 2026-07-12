Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$workflowPath = Join-Path $root ".github\workflows\release.yml"
$workflow = [System.IO.File]::ReadAllText($workflowPath)
$windowsPackageScript = [System.IO.File]::ReadAllText((Join-Path $root "scripts\package-release.ps1"))
$releaseArtifactValidator = [System.IO.File]::ReadAllText((Join-Path $root "scripts\validate-release-artifacts.ps1"))
$failures = [System.Collections.Generic.List[string]]::new()

function Add-Failure {
    param([string]$Message)

    $script:failures.Add($Message) | Out-Null
}

function Get-JobText {
    param(
        [string]$Name,
        [AllowEmptyString()][string]$NextName
    )

    $startMatch = [regex]::Match(
        $workflow,
        "(?m)^  $([regex]::Escape($Name)):\r?$"
    )
    if (-not $startMatch.Success) {
        Add-Failure "Release workflow is missing job '$Name'."
        return ""
    }

    $endIndex = $workflow.Length
    if (-not [string]::IsNullOrEmpty($NextName)) {
        $nextMatch = [regex]::Match(
            $workflow.Substring($startMatch.Index + $startMatch.Length),
            "(?m)^  $([regex]::Escape($NextName)):\r?$"
        )
        if (-not $nextMatch.Success) {
            Add-Failure "Release workflow is missing job '$NextName' after '$Name'."
            return ""
        }
        $endIndex = $startMatch.Index + $startMatch.Length + $nextMatch.Index
    }

    return $workflow.Substring($startMatch.Index, $endIndex - $startMatch.Index)
}

function Get-StepText {
    param(
        [string]$JobText,
        [string]$Name
    )

    if ([string]::IsNullOrEmpty($JobText)) {
        return ""
    }

    $matches = [regex]::Matches(
        $JobText,
        "(?m)^      - name: $([regex]::Escape($Name))\r?$"
    )
    if ($matches.Count -ne 1) {
        Add-Failure "Expected exactly one '$Name' step, found $($matches.Count)."
        return ""
    }

    $startIndex = $matches[0].Index
    $remainingStart = $startIndex + $matches[0].Length
    $nextMatch = [regex]::Match(
        $JobText.Substring($remainingStart),
        '(?m)^      - name: .+\r?$'
    )
    $endIndex = if ($nextMatch.Success) {
        $remainingStart + $nextMatch.Index
    } else {
        $JobText.Length
    }

    return $JobText.Substring($startIndex, $endIndex - $startIndex)
}

function Require-Pattern {
    param(
        [string]$Text,
        [string]$Pattern,
        [string]$Description
    )

    if (-not [regex]::IsMatch($Text, $Pattern)) {
        Add-Failure $Description
    }
}

function Reject-Pattern {
    param(
        [string]$Text,
        [string]$Pattern,
        [string]$Description
    )

    if ([regex]::IsMatch($Text, $Pattern)) {
        Add-Failure $Description
    }
}

function Test-PatternSet {
    param(
        [string]$Text,
        [string[]]$Patterns
    )

    foreach ($pattern in $Patterns) {
        if (-not [regex]::IsMatch($Text, $pattern)) {
            return $false
        }
    }

    return $true
}

function Require-PatternSet {
    param(
        [string]$Text,
        [string[]]$Patterns,
        [string]$Description
    )

    if (-not (Test-PatternSet -Text $Text -Patterns $Patterns)) {
        Add-Failure $Description
    }
}

function Get-IndentedBlockText {
    param(
        [string]$Text,
        [string]$StartPattern,
        [string]$EndPattern,
        [string]$Description
    )

    if ([string]::IsNullOrEmpty($Text)) {
        return ""
    }

    $startMatches = [regex]::Matches($Text, "(?m)^$StartPattern\r?$")
    if ($startMatches.Count -ne 1) {
        Add-Failure "Expected exactly one $Description start, found $($startMatches.Count)."
        return ""
    }

    $startIndex = $startMatches[0].Index
    $remainingStart = $startIndex + $startMatches[0].Length
    $endMatch = [regex]::Match(
        $Text.Substring($remainingStart),
        "(?m)^$EndPattern\r?$"
    )
    if (-not $endMatch.Success) {
        Add-Failure "Could not find the end of $Description."
        return ""
    }

    $endIndex = $remainingStart + $endMatch.Index + $endMatch.Length
    return $Text.Substring($startIndex, $endIndex - $startIndex)
}

function Get-JobHeaderText {
    param(
        [string]$JobText,
        [string]$Name
    )

    if ([string]::IsNullOrEmpty($JobText)) {
        return ""
    }

    $stepsMatch = [regex]::Match($JobText, '(?m)^    steps:\r?$')
    if (-not $stepsMatch.Success) {
        Add-Failure "Job '$Name' has no steps section."
        return ""
    }

    return $JobText.Substring(0, $stepsMatch.Index)
}

$resolveJob = Get-JobText -Name "resolve-version" -NextName "package-windows"
$windowsJob = Get-JobText -Name "package-windows" -NextName "package-linux"
$linuxJob = Get-JobText -Name "package-linux" -NextName "package-macos"
$macosNextJob = if ($workflow -match '(?m)^  smoke-linux-deb:\r?$') { "smoke-linux-deb" } else { "publish-release" }
$macosJob = Get-JobText -Name "package-macos" -NextName $macosNextJob
$cleanLinuxJob = Get-JobText -Name "smoke-linux-deb" -NextName "publish-release"
$publishJob = Get-JobText -Name "publish-release" -NextName ""

$resolveVersionStep = Get-StepText -JobText $resolveJob -Name "Resolve package version"
$windowsPortableStep = Get-StepText -JobText $windowsJob -Name "Smoke-test package executable"
$windowsInstallerStep = Get-StepText -JobText $windowsJob -Name "Smoke-test Windows installer"
$linuxVersionGuardStep = Get-StepText -JobText $linuxJob -Name "Test Linux release version guard"
$linuxDependenciesStep = Get-StepText -JobText $linuxJob -Name "Install Linux GUI build dependencies"
$linuxSmokeStep = Get-StepText -JobText $linuxJob -Name "Smoke-test package executable"
$macosSmokeStep = Get-StepText -JobText $macosJob -Name "Smoke-test package executable"
$cleanLinuxSmokeStep = Get-StepText -JobText $cleanLinuxJob -Name "Install and smoke-test Debian package"
$publishUpdateStep = Get-StepText -JobText $publishJob -Name "Write update channel metadata"
$publishChecksumsStep = Get-StepText -JobText $publishJob -Name "Write checksums"
$publishRevalidationStep = Get-StepText -JobText $publishJob -Name "Revalidate finalized update channel"
$publishDraftCreateStep = Get-StepText -JobText $publishJob -Name "Create draft GitHub release"
$publishDraftAuditStep = Get-StepText -JobText $publishJob -Name "Audit draft GitHub release"
$publishAuditedReleaseStep = Get-StepText -JobText $publishJob -Name "Publish audited GitHub release"
$publishFinalVerificationStep = Get-StepText -JobText $publishJob -Name "Verify published GitHub release"

$resolveJobHeader = Get-JobHeaderText -JobText $resolveJob -Name "resolve-version"
$windowsJobHeader = Get-JobHeaderText -JobText $windowsJob -Name "package-windows"
$linuxJobHeader = Get-JobHeaderText -JobText $linuxJob -Name "package-linux"
$macosJobHeader = Get-JobHeaderText -JobText $macosJob -Name "package-macos"
$cleanLinuxJobHeader = Get-JobHeaderText -JobText $cleanLinuxJob -Name "smoke-linux-deb"
$publishJobHeader = Get-JobHeaderText -JobText $publishJob -Name "publish-release"

$triggerSectionMatch = [regex]::Match(
    $workflow,
    '(?ms)^on:\r?\n(?<section>.*?)^permissions:'
)
$triggerSection = if ($triggerSectionMatch.Success) {
    $triggerSectionMatch.Groups['section'].Value
} else {
    Add-Failure "Release workflow trigger section could not be isolated."
    ""
}

$permissionsSectionMatch = [regex]::Match(
    $workflow,
    '(?ms)^permissions:\r?\n(?<section>.*?)^jobs:'
)
$permissionsSection = if ($permissionsSectionMatch.Success) {
    $permissionsSectionMatch.Groups['section'].Value
} else {
    Add-Failure "Release workflow permissions section could not be isolated."
    ""
}

Require-Pattern $triggerSection '(?m)^  workflow_dispatch:\r?$' "Release workflow must support manual workflow_dispatch execution."
Require-Pattern $triggerSection '(?ms)^  push:\r?\n    tags:\r?\n      - "v\*\.\*\.\*"\r?$' "Release publication must be triggered by version-tag pushes."

Require-Pattern $permissionsSection '(?m)^  contents: read\r?$' "Release workflow must default every job to read-only repository contents."
Reject-Pattern $permissionsSection '(?m)^  contents: write\r?$' "Release workflow must not grant contents: write at workflow scope."
Require-Pattern $publishJobHeader '(?ms)^    permissions:\r?\n      contents: write\r?$' "Only the publish job may elevate repository contents permission to write."
foreach ($jobHeader in @($resolveJobHeader, $windowsJobHeader, $linuxJobHeader, $macosJobHeader, $cleanLinuxJobHeader)) {
    Reject-Pattern $jobHeader '(?m)^      contents: write\r?$' "Non-publish release jobs must not receive contents: write permission."
}

$checkoutUses = [regex]::Matches($workflow, '(?m)^        uses: actions/checkout@v6\r?$')
$nonPersistingCheckouts = [regex]::Matches(
    $workflow,
    '(?ms)^      - name: Check out\r?\n        uses: actions/checkout@v6\r?\n        with:\r?\n          persist-credentials: false\r?$'
)
if ($checkoutUses.Count -eq 0 -or $nonPersistingCheckouts.Count -ne $checkoutUses.Count) {
    Add-Failure "Every release checkout must disable persisted Git credentials."
}

Require-Pattern `
    -Text $resolveVersionStep `
    -Pattern '(?ms)if \[\[ "\$GITHUB_EVENT_NAME" == "schedule" \]\]; then\r?\n            version="\$workspace_version"\r?\n            is_nightly=true\r?\n            artifact_suffix="-nightly\.\$\{GITHUB_RUN_NUMBER\}"\r?\n          elif' `
    -Description "Scheduled packages must retain the exact compiled workspace version."
Reject-Pattern $resolveVersionStep 'version="\$\{workspace_version\}-nightly\.\$\{GITHUB_RUN_NUMBER\}"' "Scheduled runs must not claim an uncompiled nightly semantic version."
Require-Pattern $resolveVersionStep 'if \[\[ "\$version" != "\$workspace_version" \]\]; then' "All package labels must equal the compiled workspace version."
Require-Pattern $resolveVersionStep 'artifact_suffix="-nightly\.\$\{GITHUB_RUN_NUMBER\}"' "Scheduled run numbers may appear only in the Actions artifact label suffix."
foreach ($packageJob in @($windowsJob, $linuxJob, $macosJob)) {
    Require-Pattern `
        -Text $packageJob `
        -Pattern ([regex]::Escape('${{ needs.resolve-version.outputs.version }}${{ needs.resolve-version.outputs.artifact_suffix }}')) `
        -Description "Every uploaded package artifact label must include the non-semantic nightly run suffix."
}

$targetChecksumAssignment = [regex]::Escape('$ChecksumsPath = Join-Path $OutDirPath "SHA256SUMS-$Target.txt"')
Require-Pattern $windowsPackageScript $targetChecksumAssignment "Windows packaging must use a target-specific checksum filename."
Reject-Pattern $windowsPackageScript ([regex]::Escape('$ChecksumsPath = Join-Path $OutDirPath "SHA256SUMS.txt"')) "Windows packaging must not reserve the combined release checksum filename."
Require-Pattern $releaseArtifactValidator ([regex]::Escape('$artifact.name -eq "SHA256SUMS-$Target.txt"')) "Release artifact validation must require the target-specific Windows checksum filename."

Require-PatternSet `
    -Text $windowsJobHeader `
    -Patterns @(
        '(?m)^    needs: resolve-version\r?$',
        '(?m)^    runs-on: windows-latest\r?$'
    ) `
    -Description "Manual Windows packaging must run once on windows-latest after version resolution."
Require-Pattern $windowsJob 'package-release\.ps1[^\r\n]*-Target windows-x64' "Windows package job must build the windows-x64 logical target."
Reject-Pattern $windowsJobHeader '(?m)^    if:' "Manual workflow execution must not filter out the Windows package job."

Require-PatternSet `
    -Text $linuxJobHeader `
    -Patterns @(
        '(?m)^    needs: resolve-version\r?$',
        '(?m)^    runs-on: ubuntu-latest\r?$'
    ) `
    -Description "Manual Linux packaging must run once on ubuntu-latest after version resolution."
Require-Pattern $linuxJob 'args=\(--target linux-x64 ' "Linux package job must build the linux-x64 logical target."
Reject-Pattern $linuxJobHeader '(?m)^    if:' "Manual workflow execution must not filter out the Linux package job."
Require-Pattern $linuxVersionGuardStep '(?m)^        run: bash scripts/test-linux-release-version\.sh\r?$' "Linux release packaging must execute the malformed-version behavior test."
Require-PatternSet `
    -Text $linuxDependenciesStep `
    -Patterns @(
        '(?m)^          sudo apt-get update\r?$',
        '(?m)^          sudo apt-get install -y libwebkit2gtk-4\.1-dev libxdo-dev\r?$'
    ) `
    -Description "Linux release packaging must install the native WebKitGTK and libxdo build dependencies."
$linuxDependenciesIndex = $linuxJob.IndexOf("      - name: Install Linux GUI build dependencies", [System.StringComparison]::Ordinal)
$linuxVersionGuardIndex = $linuxJob.IndexOf("      - name: Test Linux release version guard", [System.StringComparison]::Ordinal)
$linuxBuildIndex = $linuxJob.IndexOf("      - name: Build release package", [System.StringComparison]::Ordinal)
if ($linuxVersionGuardIndex -lt 0 -or $linuxDependenciesIndex -le $linuxVersionGuardIndex -or $linuxBuildIndex -le $linuxDependenciesIndex) {
    Add-Failure "Linux version guards and GUI build dependencies must run in order before release packaging."
}

Require-PatternSet `
    -Text $macosJobHeader `
    -Patterns @(
        '(?m)^    needs: resolve-version\r?$',
        ([regex]::Escape('    runs-on: ${{ matrix.runner }}'))
    ) `
    -Description "Manual macOS packaging must run on each matrix runner after version resolution."
Require-Pattern $macosJob ([regex]::Escape('--target ''${{ matrix.target }}''')) "macOS package job must build its declared matrix target."
Require-Pattern $macosSmokeStep ([regex]::Escape('tar -xzf dist/ricochet-v*-${{ matrix.target }}.tar.gz -C "$tmp"')) "macOS smoke must inspect the package for its declared matrix target."
Reject-Pattern $macosJobHeader '(?m)^    if:' "Manual workflow execution must not filter out the macOS package matrix."

$actualMacosTargets = @(
    [regex]::Matches(
        $macosJobHeader,
        '(?m)^          - target: (?<target>[^\r\n]+)\r?$'
    ) | ForEach-Object { $_.Groups['target'].Value.Trim() }
)
$expectedLogicalTargets = @(
    "windows-x64",
    "linux-x64",
    "macos-x64",
    "macos-arm64"
)
$actualLogicalTargets = @("windows-x64", "linux-x64") + $actualMacosTargets
$logicalTargetDifferences = @(
    Compare-Object `
        -ReferenceObject $expectedLogicalTargets `
        -DifferenceObject $actualLogicalTargets
)
if ($actualLogicalTargets.Count -ne 4 -or $logicalTargetDifferences.Count -ne 0) {
    Add-Failure "Manual workflow execution must build exactly windows-x64, linux-x64, macos-x64, and macos-arm64; found $($actualLogicalTargets -join ', ')."
}

$publishConditionMatches = [regex]::Matches(
    $publishJobHeader,
    '(?m)^    if:\s*(?<condition>[^\r\n]+)\r?$'
)
$expectedPublishCondition = "github.event_name == 'push' && startsWith(github.ref, 'refs/tags/')"
if (
    $publishConditionMatches.Count -ne 1 -or
    $publishConditionMatches[0].Groups['condition'].Value.Trim() -cne $expectedPublishCondition
) {
    Add-Failure "Publish job must require a tag push explicitly so workflow_dispatch can never publish."
}

Require-PatternSet `
    -Text $cleanLinuxJobHeader `
    -Patterns @(
        '(?ms)^    needs:\r?\n      - resolve-version\r?\n      - package-linux\r?$',
        '(?m)^    runs-on: ubuntu-latest\r?$',
        '(?ms)^    container:\r?\n      image: ubuntu:24\.04\r?$'
    ) `
    -Description "Debian runtime smoke must run in a clean Ubuntu 24.04 container after Linux packaging."
Require-Pattern `
    -Text $cleanLinuxJob `
    -Pattern ([regex]::Escape('name: ricochet-${{ needs.resolve-version.outputs.version }}${{ needs.resolve-version.outputs.artifact_suffix }}-linux-x64')) `
    -Description "Debian runtime smoke must download the exact Linux artifact from this run."
Require-PatternSet `
    -Text $cleanLinuxSmokeStep `
    -Patterns @(
        '(?m)^          apt-get update\r?$',
        '(?m)^          apt-get install -y --no-install-recommends "\$deb"\r?$',
        '(?m)^          for binary in /usr/bin/rco /usr/bin/rco-gui /usr/bin/ricochet; do\r?$',
        '(?m)^            ldd "\$binary" > "\$ldd_report"\r?$',
        '(?m)^            if grep -Fq ''not found'' "\$ldd_report"; then\r?$',
        '(?m)^          rco run /usr/share/ricochet/examples/basic-oop\.rco\r?$',
        '(?m)^          RICOCHET_GUI_EXPORT_HTML="\$webview_export" rco gui /usr/share/ricochet/examples/webview_ui\.rco\r?$',
        '(?m)^          grep -Fq ''<title>Ricochet Desktop UI</title>'' "\$webview_export"\r?$'
    ) `
    -Description "Debian runtime smoke must install declared runtime dependencies, resolve every launcher, and exercise installed basic and WebView probes."
Require-Pattern $publishJobHeader '(?m)^      - smoke-linux-deb\r?$' "Tag publication must depend on the clean Debian runtime smoke."

$publishUpdateIndex = $publishJob.IndexOf("      - name: Write update channel metadata", [StringComparison]::Ordinal)
$publishChecksumsIndex = $publishJob.IndexOf("      - name: Write checksums", [StringComparison]::Ordinal)
$publishRevalidationIndex = $publishJob.IndexOf("      - name: Revalidate finalized update channel", [StringComparison]::Ordinal)
$publishCreateIndex = $publishJob.IndexOf("      - name: Create draft GitHub release", [StringComparison]::Ordinal)
$publishAuditIndex = $publishJob.IndexOf("      - name: Audit draft GitHub release", [StringComparison]::Ordinal)
$publishPromoteIndex = $publishJob.IndexOf("      - name: Publish audited GitHub release", [StringComparison]::Ordinal)
$publishVerifyIndex = $publishJob.IndexOf("      - name: Verify published GitHub release", [StringComparison]::Ordinal)
if (
    $publishUpdateIndex -lt 0 -or
    $publishChecksumsIndex -le $publishUpdateIndex -or
    $publishRevalidationIndex -le $publishChecksumsIndex -or
    $publishCreateIndex -le $publishRevalidationIndex -or
    $publishAuditIndex -le $publishCreateIndex -or
    $publishPromoteIndex -le $publishAuditIndex -or
    $publishVerifyIndex -le $publishPromoteIndex
) {
    Add-Failure "Publish must write the channel, finalize checksums, revalidate, create a draft, audit it, promote it, then verify public state."
}
Require-Pattern $publishChecksumsStep "! -name 'SHA256SUMS\.txt'" "Combined checksum generation must exclude only its own final output."
Require-Pattern $publishRevalidationStep 'validate-update-channel\.ps1' "The finalized release set must revalidate update-channel hashes after combined checksums are written."
Require-PatternSet `
    -Text $publishDraftCreateStep `
    -Patterns @(
        'gh release create',
        'git ls-remote',
        'refs/tags/\$GITHUB_REF_NAME\^\{\}',
        '\$GITHUB_SHA',
        '--verify-tag',
        '--draft',
        '--prerelease',
        '--latest=false'
    ) `
    -Description "Tag publication must stage an unpublished draft prerelease before auditing GitHub assets."
$remoteTagVerificationIndex = $publishDraftCreateStep.IndexOf('git ls-remote', [StringComparison]::Ordinal)
$draftCreationCommandIndex = $publishDraftCreateStep.IndexOf('gh release create', [StringComparison]::Ordinal)
if ($remoteTagVerificationIndex -lt 0 -or $draftCreationCommandIndex -le $remoteTagVerificationIndex) {
    Add-Failure "The remote tag must peel to the workflow SHA immediately before draft creation."
}
Require-PatternSet `
    -Text $publishDraftAuditStep `
    -Patterns @(
        'gh api',
        'gh release download',
        'validate-published-release-assets\.ps1',
        '-RequireDraft',
        '-RequireStable',
        'validate-update-channel\.ps1'
    ) `
    -Description "Draft audit must compare GitHub API/downloaded assets and revalidate the candidate channel before publication."
Require-PatternSet `
    -Text $publishAuditedReleaseStep `
    -Patterns @(
        'gh release edit',
        'git ls-remote',
        'refs/tags/\$GITHUB_REF_NAME\^\{\}',
        '\$GITHUB_SHA',
        '--draft=false',
        '--prerelease',
        '--latest=false'
    ) `
    -Description "Only an audited draft may be promoted to the public RC release."
$promotionTagVerificationIndex = $publishAuditedReleaseStep.IndexOf('git ls-remote', [StringComparison]::Ordinal)
$promotionCommandIndex = $publishAuditedReleaseStep.IndexOf('gh release edit', [StringComparison]::Ordinal)
if ($promotionTagVerificationIndex -lt 0 -or $promotionCommandIndex -le $promotionTagVerificationIndex) {
    Add-Failure "The remote tag must still peel to the workflow SHA immediately before draft promotion."
}
Require-PatternSet `
    -Text $publishFinalVerificationStep `
    -Patterns @(
        'gh api',
        'validate-published-release-assets\.ps1',
        '-RequirePublished',
        '-RequireStable'
    ) `
    -Description "Published release state must be re-queried and verified after promotion."

$portableIndex = $windowsJob.IndexOf("      - name: Smoke-test package executable", [StringComparison]::Ordinal)
$installerIndex = $windowsJob.IndexOf("      - name: Smoke-test Windows installer", [StringComparison]::Ordinal)
$manifestIndex = $windowsJob.IndexOf("      - name: Validate release artifact manifest", [StringComparison]::Ordinal)
if ($portableIndex -lt 0 -or $installerIndex -le $portableIndex -or $manifestIndex -le $installerIndex) {
    Add-Failure "Windows installer smoke must follow portable smoke and precede artifact validation."
}

$windowsPortableNoticeBlock = Get-IndentedBlockText `
    -Text $windowsPortableStep `
    -StartPattern '          foreach \(\$noticeName in @\(' `
    -EndPattern '          \}' `
    -Description "Windows portable notice loop"
$windowsInstallerSourceNoticeBlock = Get-IndentedBlockText `
    -Text $windowsInstallerStep `
    -StartPattern '          foreach \(\$noticeName in @\(' `
    -EndPattern '          \}' `
    -Description "Windows installer source notice loop"
$windowsInstallerInstalledNoticeBlock = Get-IndentedBlockText `
    -Text $windowsInstallerStep `
    -StartPattern '          foreach \(\$trackedFile in @\(' `
    -EndPattern '          \}' `
    -Description "Windows installed-file hash loop"
$windowsInstallerHashFunction = Get-IndentedBlockText `
    -Text $windowsInstallerStep `
    -StartPattern '          function Assert-Sha256Match \{' `
    -EndPattern '          \}' `
    -Description "Windows installer SHA-256 helper"
$linuxNoticeBlock = Get-IndentedBlockText `
    -Text $linuxSmokeStep `
    -StartPattern '          for notice in THIRD_PARTY_LICENSES\.html THIRD_PARTY_NOTICES\.txt; do' `
    -EndPattern '          done' `
    -Description "Linux tar and Debian notice loop"
$macosNoticeBlock = Get-IndentedBlockText `
    -Text $macosSmokeStep `
    -StartPattern '          for notice in THIRD_PARTY_LICENSES\.html THIRD_PARTY_NOTICES\.txt; do' `
    -EndPattern '          done' `
    -Description "macOS matrix notice loop"

$noticeNameLoopHeaderPattern = '(?ms)^          foreach \(\$noticeName in @\(\r?\n            ''THIRD_PARTY_LICENSES\.html'',\r?\n            ''THIRD_PARTY_NOTICES\.txt''\r?\n          \)\) \{'
$windowsPortableNoticePatterns = @(
    $noticeNameLoopHeaderPattern,
    '(?m)^            \$repositoryNotice = Join-Path \$env:GITHUB_WORKSPACE \$noticeName\r?$',
    '(?m)^            \$packagedNotice = Join-Path \$packageDir \$noticeName\r?$',
    '(?m)^            if \(\(Get-FileHash -LiteralPath \$packagedNotice -Algorithm SHA256\)\.Hash -cne \(Get-FileHash -LiteralPath \$repositoryNotice -Algorithm SHA256\)\.Hash\) \{\r?$'
)
$windowsInstallerSourceNoticePatterns = @(
    $noticeNameLoopHeaderPattern,
    '(?ms)^            Assert-Sha256Match `\r?\n              -Expected \(Join-Path \$repoRoot \$noticeName\) `\r?\n              -Actual \(Join-Path \$installerSource \$noticeName\) `\r?\n              -Label "Windows installer source \$noticeName"\r?$'
)
$windowsInstallerInstalledNoticePatterns = @(
    '(?ms)^          foreach \(\$trackedFile in @\(\r?\n            ''LICENSE'',\r?\n            ''THIRD_PARTY_LICENSES\.html'',\r?\n            ''THIRD_PARTY_NOTICES\.txt''\r?\n          \)\) \{',
    '(?ms)^            Assert-Sha256Match `\r?\n              -Expected \(Join-Path \$repoRoot \$trackedFile\) `\r?\n              -Actual \(Join-Path \$installDir \$trackedFile\) `\r?\n              -Label "Installed \$trackedFile"\r?$'
)
$windowsInstallerHashFunctionPatterns = @(
    '(?m)^            \$expectedHash = \(Get-FileHash -LiteralPath \$Expected -Algorithm SHA256\)\.Hash\r?$',
    '(?m)^            \$actualHash = \(Get-FileHash -LiteralPath \$Actual -Algorithm SHA256\)\.Hash\r?$',
    '(?m)^            if \(\$actualHash -cne \$expectedHash\) \{\r?$'
)
$linuxNoticePatterns = @(
    '(?m)^          for notice in THIRD_PARTY_LICENSES\.html THIRD_PARTY_NOTICES\.txt; do\r?$',
    '(?m)^            tar_notice="\$package_dir/\$notice"\r?$',
    '(?m)^            deb_notice="\$tmp/deb-root/usr/share/doc/ricochet/\$notice"\r?$',
    '(?m)^            repository_hash="\$\(notice_hash "\$notice"\)"\r?$',
    '(?m)^            tar_hash="\$\(notice_hash "\$tar_notice"\)"\r?$',
    '(?m)^            deb_hash="\$\(notice_hash "\$deb_notice"\)"\r?$',
    '(?m)^            if \[\[ "\$tar_hash" != "\$repository_hash" \]\]; then\r?$',
    '(?m)^            if \[\[ "\$deb_hash" != "\$repository_hash" \]\]; then\r?$'
)
$macosNoticePatterns = @(
    '(?m)^          for notice in THIRD_PARTY_LICENSES\.html THIRD_PARTY_NOTICES\.txt; do\r?$',
    '(?m)^            packaged_notice="\$package_dir/\$notice"\r?$',
    '(?m)^            repository_hash="\$\(notice_hash "\$notice"\)"\r?$',
    '(?m)^            packaged_hash="\$\(notice_hash "\$packaged_notice"\)"\r?$',
    '(?m)^            if \[\[ "\$packaged_hash" != "\$repository_hash" \]\]; then\r?$'
)

Require-PatternSet `
    -Text $windowsPortableNoticeBlock `
    -Patterns $windowsPortableNoticePatterns `
    -Description "Windows portable smoke must compare each named packaged notice hash with that named repository notice hash in one loop."
Require-PatternSet `
    -Text $windowsInstallerSourceNoticeBlock `
    -Patterns $windowsInstallerSourceNoticePatterns `
    -Description "Windows installer smoke must compare each named staged notice hash with that named repository notice hash."
Require-PatternSet `
    -Text $windowsInstallerInstalledNoticeBlock `
    -Patterns $windowsInstallerInstalledNoticePatterns `
    -Description "Windows installer smoke must compare each named installed notice hash with that named repository notice hash."
Require-PatternSet `
    -Text $windowsInstallerHashFunction `
    -Patterns $windowsInstallerHashFunctionPatterns `
    -Description "Windows installer SHA-256 helper must compare the hashes of its Expected and Actual paths."
Require-PatternSet `
    -Text $linuxNoticeBlock `
    -Patterns $linuxNoticePatterns `
    -Description "Linux smoke must compare each named tar and Debian notice hash with that named repository notice hash in one loop."
Require-PatternSet `
    -Text $macosNoticeBlock `
    -Patterns $macosNoticePatterns `
    -Description "macOS smoke must compare each named packaged notice hash with that named repository notice hash in one matrix loop."

$decoupledNoticeFixture = @'
          foreach ($noticeName in @(
            'THIRD_PARTY_LICENSES.html',
            'THIRD_PARTY_NOTICES.txt'
          )) {
            $repositoryNotice = Join-Path $env:GITHUB_WORKSPACE $noticeName
            $packagedNotice = Join-Path $packageDir $noticeName
            $unrelatedHash = (Get-FileHash -LiteralPath $env:GITHUB_WORKSPACE -Algorithm SHA256).Hash
          }
'@
$decoupledFixturePassedIndependentChecks =
    [regex]::IsMatch($decoupledNoticeFixture, [regex]::Escape("THIRD_PARTY_LICENSES.html")) -and
    [regex]::IsMatch($decoupledNoticeFixture, [regex]::Escape("THIRD_PARTY_NOTICES.txt")) -and
    [regex]::IsMatch($decoupledNoticeFixture, "Get-FileHash")
if (-not $decoupledFixturePassedIndependentChecks) {
    Add-Failure "The deliberately decoupled notice mutation fixture no longer exercises the legacy independent checks."
}
if (Test-PatternSet -Text $decoupledNoticeFixture -Patterns $windowsPortableNoticePatterns) {
    Add-Failure "Relationship-aware notice assertions accepted a deliberately unrelated hash mutation."
}

Require-Pattern $windowsPortableStep 'Get-FileHash' "Windows portable smoke must use SHA-256 file hashes."
Require-Pattern $windowsInstallerStep 'Get-FileHash' "Windows installer smoke must use SHA-256 file hashes."
Require-Pattern $linuxSmokeStep 'sha256sum' "Linux smoke must use sha256sum for notice integrity."
Require-Pattern $linuxSmokeStep '\$package_dir' "Linux smoke must compare notice hashes from the tar root."
Require-Pattern $linuxSmokeStep '\$tmp/deb-root/usr/share/doc/ricochet' "Linux smoke must compare notice hashes from the extracted Debian documentation directory."
Require-Pattern $macosSmokeStep 'shasum -a 256' "macOS smoke must use shasum -a 256 for notice integrity."
Require-Pattern $macosSmokeStep '\$package_dir' "macOS smoke must compare notice hashes from each matrix tar root."
Require-Pattern $windowsPortableStep ([regex]::Escape("& .\docs\reference\validate.ps1 -Root (Join-Path `$packageDir 'docs\reference') -PackageMode")) "Windows portable smoke must validate packaged offline HTML links."
Require-Pattern $windowsInstallerStep ([regex]::Escape("& .\docs\reference\validate.ps1 -Root (Join-Path `$installerSource 'docs\reference') -PackageMode")) "Windows installer source must validate packaged offline HTML links."
Require-Pattern $windowsInstallerStep ([regex]::Escape("& .\docs\reference\validate.ps1 -Root (Join-Path `$installDir 'docs\reference') -PackageMode")) "Installed Windows docs must validate offline HTML links."
Require-Pattern $linuxSmokeStep ([regex]::Escape('pwsh -NoProfile -File ./docs/reference/validate.ps1 -Root "$package_dir/docs/reference" -PackageMode')) "Linux portable smoke must validate packaged offline HTML links."
Require-Pattern $linuxSmokeStep ([regex]::Escape('pwsh -NoProfile -File ./docs/reference/validate.ps1 -Root "$tmp/deb-root/usr/share/doc/ricochet/reference" -PackageMode')) "Debian smoke must validate installed-layout offline HTML links."
Require-Pattern $macosSmokeStep ([regex]::Escape('pwsh -NoProfile -File ./docs/reference/validate.ps1 -Root "$package_dir/docs/reference" -PackageMode')) "macOS smoke must validate packaged offline HTML links."

$installerRequirements = [ordered]@{
    'ricochet-v\$version-windows-x64-setup\.exe' = "Installer smoke must resolve the version-specific setup executable."
    'ricochet-v\$version-windows-x64' = "Installer smoke must resolve the version-specific staged package directory."
    'installerCandidates\.Count -ne 1' = "Installer smoke must require exactly one setup executable."
    'sourceCandidates\.Count -ne 1' = "Installer smoke must require exactly one staged package directory."
    '\$env:RUNNER_TEMP' = "Installer smoke must use RUNNER_TEMP."
    'ricochet-installer-smoke-' = "Installer smoke must use a unique disposable root."
    'outside-checkout' = "Installer smoke must use an outside-checkout working directory."
    'NSIS smoke install path contains whitespace or quotes' = "Installer smoke must reject a /D path that needs quoting."
    'New-Item -ItemType Directory -Path \$installDir' = "Installer smoke must construct an owned upgrade fixture."
    'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Ricochet' = "Installer smoke must inspect the HKCU uninstall key."
    '\[Environment\+SpecialFolder\]::Programs' = "Installer smoke must resolve the current-user Programs folder."
    'Start-Process' = "Installer smoke must execute native installer and runtime processes with explicit waits."
    '"/S /D=\$installDir"' = "Installer smoke must silently install with final unquoted /D."
    'rco\.exe' = "Installer smoke must require installed rco.exe."
    'rco-gui\.exe' = "Installer smoke must require installed rco-gui.exe."
    'ricochet\.exe' = "Installer smoke must require installed ricochet.exe."
    'Ricochet Shell\.cmd' = "Installer smoke must require the installed shell launcher."
    'Uninstall\.exe' = "Installer smoke must require the uninstaller."
    'LICENSE' = "Installer smoke must require and hash-check LICENSE."
    'examples\\basic-oop\.rco' = "Installer smoke must require installed basic-oop."
    'examples\\webview_ui\.rco' = "Installer smoke must require the installed WebView example."
    'docs\\reference\\index\.html' = "Installer smoke must require the installed reference index."
    'foreach \(\$binaryName in @\(''rco\.exe'', ''ricochet\.exe''\)\)' = "Installer smoke must check only CLI aliases with --version."
    'rco \$version' = "Installer smoke must require exact rco <version> output."
    'Push-Location -LiteralPath \$outsideDir' = "Installer smoke must package the installed WebView outside the checkout."
    '& \$rco package \$webviewSource' = "Installer smoke must package WebView with installed rco."
    'RICOCHET_GUI_EXPORT_HTML' = "Installer smoke must export installed WebView HTML."
    '<title>Ricochet Desktop UI</title>' = "Installer smoke must verify the installed WebView title."
    'DisplayName' = "Installer smoke must verify DisplayName."
    'DisplayVersion' = "Installer smoke must verify DisplayVersion."
    'Publisher' = "Installer smoke must verify Publisher."
    'InstallLocation' = "Installer smoke must verify InstallLocation."
    'UninstallString' = "Installer smoke must verify UninstallString."
    'NoModify' = "Installer smoke must verify NoModify."
    'NoRepair' = "Installer smoke must verify NoRepair."
    'Ricochet Shell\.lnk' = "Installer smoke must verify the shell shortcut."
    'Reference Docs\.lnk' = "Installer smoke must verify the docs shortcut."
    'Third-Party Licenses\.lnk' = "Installer smoke must verify the third-party licenses shortcut."
    'Uninstall Ricochet\.lnk' = "Installer smoke must verify the uninstall shortcut."
    "-ArgumentList '/S'" = "Installer smoke must run the normal silent uninstaller."
    'AddSeconds\(30\)' = "Installer smoke must bound uninstall polling to 30 seconds."
    'Start-Sleep -Milliseconds 250' = "Installer smoke must poll without a blocking delay."
    'uninstallStubExitCode -ne 0' = "Installer smoke must require the uninstaller stub exit code to be zero."
}

foreach ($requirement in $installerRequirements.GetEnumerator()) {
    Require-Pattern $windowsInstallerStep $requirement.Key $requirement.Value
}

Reject-Pattern $windowsInstallerStep '(?i)\bRemove-Item\b' "Installer smoke must not manually delete installer-owned state."
Reject-Pattern $windowsInstallerStep '_\?=' "Installer smoke must not disable NSIS uninstaller self-copying."
Reject-Pattern $windowsInstallerStep '(?i)rco-gui(?:\.exe)?[^\r\n]*--version' "Installer smoke must not invoke rco-gui --version."

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "Release workflow contract tests failed:`n$($details -join "`n")"
}

Write-Host "Release workflow contract tests passed."
