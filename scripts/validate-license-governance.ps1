param()

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$Failures = [System.Collections.Generic.List[string]]::new()

function Add-Failure {
    param([string]$Message)

    [void]$script:Failures.Add($Message)
}

function Get-RepoPath {
    param([string]$RelativePath)

    return Join-Path $Root ($RelativePath -replace '/', [System.IO.Path]::DirectorySeparatorChar)
}

function Read-RequiredFile {
    param([string]$RelativePath)

    $path = Get-RepoPath $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Add-Failure "Missing required file: $RelativePath"
        return $null
    }

    return [System.IO.File]::ReadAllText($path)
}

function Normalize-Text {
    param([string]$Text)

    $normalized = $Text.Replace("`r`n", "`n").Replace("`r", "`n")
    while ($normalized.EndsWith("`n", [System.StringComparison]::Ordinal)) {
        $normalized = $normalized.Substring(0, $normalized.Length - 1)
    }
    return $normalized + "`n"
}

function Get-Sha256 {
    param([string]$Text)

    $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($Text)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha.ComputeHash($bytes)
    }
    finally {
        $sha.Dispose()
    }
    return -join ($hash | ForEach-Object { $_.ToString("x2") })
}

function Require-Match {
    param(
        [string]$RelativePath,
        [AllowNull()]$Contents,
        [string]$Pattern,
        [string]$Description
    )

    if ($null -ne $Contents -and $Contents -notmatch $Pattern) {
        Add-Failure "$RelativePath must $Description"
    }
}

function Require-ApacheManifestLicense {
    param([string]$RelativePath)

    $contents = Read-RequiredFile $RelativePath
    if ($null -eq $contents) {
        return
    }

    $packageSection = [regex]::Match(
        $contents,
        '(?ms)^\[package\]\s*(.*?)(?=^\[|\z)'
    )
    if (-not $packageSection.Success) {
        Add-Failure "$RelativePath must contain a [package] section"
        return
    }

    $matches = [regex]::Matches(
        $packageSection.Groups[1].Value,
        '(?m)^\s*license\s*=\s*"([^"]+)"\s*$'
    )
    if ($matches.Count -ne 1) {
        Add-Failure "$RelativePath must contain exactly one package license declaration"
        return
    }

    if ($matches[0].Groups[1].Value -cne "Apache-2.0") {
        Add-Failure "$RelativePath package license must be Apache-2.0"
    }
}

$license = Read-RequiredFile "LICENSE"
if ($null -ne $license) {
    # Official source: https://www.apache.org/licenses/LICENSE-2.0.txt
    $expectedLicenseSha256 = "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"
    $actualLicenseSha256 = Get-Sha256 (Normalize-Text $license)
    if ($actualLicenseSha256 -cne $expectedLicenseSha256) {
        Add-Failure "LICENSE must be the canonical Apache License 2.0 text (normalized SHA-256 $expectedLicenseSha256; found $actualLicenseSha256)"
    }
}

$workspaceManifest = Read-RequiredFile "Cargo.toml"
$workspacePackageSection = if ($null -ne $workspaceManifest) {
    [regex]::Match($workspaceManifest, '(?ms)^\[workspace\.package\]\s*(.*?)(?=^\[|\z)')
} else {
    $null
}
if ($null -ne $workspacePackageSection -and -not $workspacePackageSection.Success) {
    Add-Failure "Cargo.toml must contain a [workspace.package] section"
} elseif ($null -ne $workspacePackageSection) {
    Require-Match "Cargo.toml [workspace.package]" $workspacePackageSection.Groups[1].Value '(?m)^license\s*=\s*"Apache-2\.0"\s*$' 'set license to Apache-2.0'
}
if ($null -ne $workspaceManifest -and $workspaceManifest -match 'MIT\s+OR\s+Apache-2\.0') {
    Add-Failure "Cargo.toml must not retain the former MIT OR Apache-2.0 declaration"
}

$crateManifests = @(
    "crates/ricochet_bytecode/Cargo.toml",
    "crates/ricochet_cli/Cargo.toml",
    "crates/ricochet_compiler/Cargo.toml",
    "crates/ricochet_syntax/Cargo.toml",
    "crates/ricochet_vm/Cargo.toml",
    "crates/ricochet_web/Cargo.toml"
)
foreach ($manifest in $crateManifests) {
    $contents = Read-RequiredFile $manifest
    if ($null -ne $contents) {
        $packageSection = [regex]::Match($contents, '(?ms)^\[package\]\s*(.*?)(?=^\[|\z)')
        if (-not $packageSection.Success) {
            Add-Failure "$manifest must contain a [package] section"
        } else {
            Require-Match "$manifest [package]" $packageSection.Groups[1].Value '(?m)^license\.workspace\s*=\s*true\s*$' 'inherit the workspace Apache-2.0 license'
        }
    }
}

$firstPartyPackageManifests = @(
    "packages/ricochet_ai/ricochet.toml",
    "packages/ricochet_auth/ricochet.toml",
    "packages/ricochet_forms/ricochet.toml",
    "packages/ricochet_python/ricochet.toml",
    "packages/ricochet_test_helpers/ricochet.toml",
    "examples/learn/27-auth-forms/login_flow/.ricochet/packages/auth/ricochet.toml",
    "examples/learn/27-auth-forms/login_flow/.ricochet/packages/forms/ricochet.toml",
    "examples/learn/28-ai/fake_provider_chat/.ricochet/packages/ai/ricochet.toml",
    "examples/showcase/ai_provider_probe/.ricochet/packages/ai/ricochet.toml",
    "examples/showcase/package_auth_forms/.ricochet/packages/auth/ricochet.toml",
    "examples/showcase/package_auth_forms/.ricochet/packages/forms/ricochet.toml"
)
foreach ($manifest in $firstPartyPackageManifests) {
    Require-ApacheManifestLicense $manifest
}

$trackedFiles = @(& git -C $Root ls-files --cached --others --exclude-standard)
if ($LASTEXITCODE -ne 0) {
    Add-Failure "git ls-files failed while scanning for stale license declarations"
} else {
    $textExtensions = @(".html", ".json", ".md", ".ps1", ".rs", ".sh", ".toml", ".txt", ".yaml", ".yml")
    foreach ($relativePath in $trackedFiles) {
        $normalizedPath = $relativePath.Replace("\", "/")
        if ($normalizedPath -eq "scripts/validate-license-governance.ps1" -or
            $normalizedPath -like "THIRD_PARTY_*" -or
            $normalizedPath.StartsWith("vendor/", [System.StringComparison]::Ordinal)) {
            continue
        }

        $extension = [System.IO.Path]::GetExtension($normalizedPath).ToLowerInvariant()
        if ($textExtensions -notcontains $extension -and [System.IO.Path]::GetFileName($normalizedPath) -ne "LICENSE") {
            continue
        }

        $contents = [System.IO.File]::ReadAllText((Get-RepoPath $normalizedPath))
        if ($contents -match 'GPL-3\.0|MIT\s+OR\s+Apache-2\.0') {
            Add-Failure "$normalizedPath contains a stale GPL-3.0 or MIT OR Apache-2.0 declaration"
        }
        if ($normalizedPath -ne "crates/ricochet_cli/src/lib.rs" -and $contents -match '<project_license>MIT</project_license>') {
            Add-Failure "$normalizedPath identifies a first-party AppStream project as MIT"
        }
    }
}

$approvedPublicMarkdownPaths = @(
    "README.md",
    "SECURITY.md",
    "SUPPORT.md",
    "docs/reference/README.md",
    "docs/wiki/README.md",
    "editors/vscode/README.md",
    "examples/learn/README.md",
    "examples/showcase/README.md",
    "packages/README.md",
    "packages/ricochet_ai/README.md",
    "packages/ricochet_auth/README.md",
    "packages/ricochet_forms/README.md",
    "packages/ricochet_python/README.md",
    "packages/ricochet_test_helpers/README.md"
)
$approvedPublicMarkdownSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($approvedMarkdownPath in $approvedPublicMarkdownPaths) {
    [void]$approvedPublicMarkdownSet.Add($approvedMarkdownPath)
}
$trackedMarkdownPaths = @(& git -C $Root ls-files --cached -- "*.md")
if ($LASTEXITCODE -ne 0) {
    Add-Failure "git ls-files failed while validating the public Markdown allowlist"
} else {
    $trackedMarkdownSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($trackedMarkdownPath in $trackedMarkdownPaths) {
        [void]$trackedMarkdownSet.Add($trackedMarkdownPath)
    }
    foreach ($trackedMarkdownPath in $trackedMarkdownPaths) {
        if (-not $approvedPublicMarkdownSet.Contains($trackedMarkdownPath)) {
            Add-Failure "tracked Markdown path is outside the approved public allowlist: $trackedMarkdownPath"
        }
    }
    foreach ($approvedMarkdownPath in $approvedPublicMarkdownPaths) {
        if (-not $trackedMarkdownSet.Contains($approvedMarkdownPath)) {
            Add-Failure "approved public Markdown path is not tracked: $approvedMarkdownPath"
        }
    }
}

$editorManifestPath = "editors/vscode/package.json"
$editorManifest = Read-RequiredFile $editorManifestPath
if ($null -ne $editorManifest) {
    try {
        $editorMetadata = $editorManifest | ConvertFrom-Json
        if ([string]$editorMetadata.license -cne "Apache-2.0") {
            Add-Failure "$editorManifestPath license must be Apache-2.0"
        }
    }
    catch {
        Add-Failure "$editorManifestPath must contain valid JSON: $($_.Exception.Message)"
    }
}

$linuxPackagerPath = "scripts/package-release-linux.sh"
$linuxPackager = Read-RequiredFile $linuxPackagerPath
Require-Match $linuxPackagerPath $linuxPackager '<metadata_license>CC0-1\.0</metadata_license>' 'retain the AppStream metadata license CC0-1.0'
Require-Match $linuxPackagerPath $linuxPackager '<project_license>Apache-2\.0</project_license>' 'identify official Ricochet packages as Apache-2.0'
if ($null -ne $linuxPackager -and $linuxPackager -match '<project_license>MIT</project_license>') {
    Add-Failure "$linuxPackagerPath must not identify official Ricochet packages as MIT"
}

$gitignore = Read-RequiredFile ".gitignore"
Require-Match ".gitignore" $gitignore '(?m)^!SECURITY\.md\s*$' 'allow the root security policy to be tracked'
Require-Match ".gitignore" $gitignore '(?m)^!SUPPORT\.md\s*$' 'allow the root support policy to be tracked'

$gitAttributesPath = ".gitattributes"
$gitAttributes = Read-RequiredFile $gitAttributesPath
Require-Match $gitAttributesPath $gitAttributes '(?m)^/licenses/third-party-licenses\.hbs\s+text\s+eol=lf\s*$' 'keep the cargo-about template byte-stable across checkouts'
Require-Match $gitAttributesPath $gitAttributes '(?m)^/THIRD_PARTY_LICENSES\.html\s+text\s+eol=lf\s+-whitespace\s*$' 'keep the license snapshot byte-stable while preserving upstream license text'
Require-Match $gitAttributesPath $gitAttributes '(?m)^/THIRD_PARTY_NOTICES\.txt\s+text\s+eol=lf\s+-whitespace\s*$' 'keep the notice snapshot byte-stable while preserving upstream notice text'

$securityPath = "SECURITY.md"
$security = Read-RequiredFile $securityPath
Require-Match $securityPath $security 'https://github\.com/BARKx4/Ricochet/security/advisories/new' 'link the enabled private vulnerability-reporting form'
Require-Match $securityPath $security '(?i)do not.*public GitHub issue' 'tell reporters not to disclose suspected vulnerabilities publicly'
Require-Match $securityPath $security '(?i)newest published release candidate' 'state the supported prerelease line'
Require-Match $securityPath $security '(?i)best-effort' 'avoid implying a response or remediation SLA'
Require-Match $securityPath $security '(?i)sandboxed' 'describe the opt-in untrusted-code capability boundary'

$supportPath = "SUPPORT.md"
$support = Read-RequiredFile $supportPath
Require-Match $supportPath $support 'https://github\.com/BARKx4/Ricochet/issues' 'route public questions and bugs to the enabled issue tracker'
Require-Match $supportPath $support '\[SECURITY\.md\]\(SECURITY\.md\)' 'route suspected vulnerabilities to the private security policy'
Require-Match $supportPath $support '(?i)Windows x64' 'name the official Windows artifact target'
Require-Match $supportPath $support '(?i)Linux x64' 'name the official Linux artifact target'
Require-Match $supportPath $support '(?i)macOS arm64 and x64' 'name both official macOS artifact targets'
Require-Match $supportPath $support '(?i)best-effort' 'state the prerelease support boundary without an SLA'

$readme = Read-RequiredFile "README.md"
Require-Match "README.md" $readme '\[Apache License 2\.0\]\(LICENSE\)' 'link the repository license'
Require-Match "README.md" $readme '(?is)Third-party components remain subject to their\s+own licenses' 'preserve third-party license boundaries'
Require-Match "README.md" $readme '\[third-party license report\]\(THIRD_PARTY_LICENSES\.html\)' 'link the tracked third-party license report'
Require-Match "README.md" $readme '\[supplemental notices\]\(THIRD_PARTY_NOTICES\.txt\)' 'link the tracked supplemental notices'
Require-Match "README.md" $readme '\[security policy\]\(https://github\.com/BARKx4/Ricochet/security/policy\)' 'link the security policy from both source and packaged copies'
Require-Match "README.md" $readme '\[support guide\]\(https://github\.com/BARKx4/Ricochet/blob/main/SUPPORT\.md\)' 'link the support guide from both source and packaged copies'

$acceptancePath = "scripts/acceptance.ps1"
$acceptance = Read-RequiredFile $acceptancePath
Require-Match $acceptancePath $acceptance 'validate-license-governance\.ps1' 'run this validator in the acceptance suite'
Require-Match $acceptancePath $acceptance 'validate-third-party-notices\.ps1' 'validate the deterministic third-party license and notice snapshots'

$aboutConfigPath = "about.toml"
$aboutConfig = Read-RequiredFile $aboutConfigPath
foreach ($licenseId in @(
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "MIT",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "Unicode-3.0",
    "CDLA-Permissive-2.0",
    "BSL-1.0",
    "MPL-2.0"
)) {
    Require-Match $aboutConfigPath $aboutConfig ([regex]::Escape('"' + $licenseId + '"')) "allow approved dependency license $licenseId"
}
Require-Match $aboutConfigPath $aboutConfig '(?s)accepted\s*=\s*\[\s*"Apache-2\.0"\s*,' 'prefer Apache-2.0 when satisfying dual-license expressions'
Require-Match $aboutConfigPath $aboutConfig '(?m)^ignore-build-dependencies\s*=\s*false\s*$' 'include build dependencies'
Require-Match $aboutConfigPath $aboutConfig '(?m)^ignore-dev-dependencies\s*=\s*true\s*$' 'exclude dev-only dependencies'
Require-Match $aboutConfigPath $aboutConfig '(?m)^ignore-transitive-dependencies\s*=\s*false\s*$' 'include transitive dependencies'
Require-Match $aboutConfigPath $aboutConfig '(?s)workarounds\s*=\s*\[[^\]]*"ring"[^\]]*"unicode-ident"' 'retain the approved ring and unicode-ident clarifications'
Require-Match $aboutConfigPath $aboutConfig '(?s)\[ring\][\s\S]*?accepted\s*=\s*\[\s*"OpenSSL"\s*\]' 'accept the ring OpenSSL clarification'
Require-Match $aboutConfigPath $aboutConfig '(?s)\[unicode-ident\][\s\S]*?accepted\s*=\s*\[\s*"Unicode-DFS-2016"\s*\]' 'accept the unicode-ident Unicode-DFS-2016 clarification'

$aboutTemplatePath = "licenses/third-party-licenses.hbs"
$aboutTemplate = Read-RequiredFile $aboutTemplatePath
Require-Match $aboutTemplatePath $aboutTemplate 'THIRD-PARTY LICENSES' 'identify the generated third-party license report'

$noticeGeneratorPath = "scripts/generate-third-party-notices.ps1"
$noticeGenerator = Read-RequiredFile $noticeGeneratorPath
foreach ($target in @(
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin"
)) {
    Require-Match $aboutConfigPath $aboutConfig ([regex]::Escape($target)) "include target $target in cargo-about generation"
    Require-Match $noticeGeneratorPath $noticeGenerator ([regex]::Escape($target)) "include target $target in the dependency union"
}
Require-Match $noticeGeneratorPath $noticeGenerator 'cargo\s+about\s+generate[\s\S]+?--locked[\s\S]+?--workspace' 'generate the locked workspace license report with cargo-about'
Require-Match $noticeGeneratorPath $noticeGenerator 'cargo\s+about\s+generate[^\r\n]*--fail(?:\s|$)' 'fail on unresolved cargo-about license or clarification errors'
Require-Match $noticeGeneratorPath $noticeGenerator 'Normalize-Text\s+\(\[System\.IO\.File\]::ReadAllText\(\$licensesOutput\)\)' 'normalize generated cargo-about HTML to LF before snapshot comparison'
Require-Match $noticeGeneratorPath $noticeGenerator 'cargo\s+metadata[\s\S]+?--locked[\s\S]+?--filter-platform' 'derive the union from locked target-specific Cargo metadata'
Require-Match $noticeGeneratorPath $noticeGenerator 'cargo\s+tree[\s\S]+?--locked[\s\S]+?--workspace[\s\S]+?--target[\s\S]+?--edges\s+"normal,build"' 'select the feature-aware active non-dev graph for each target'
Require-Match $noticeGeneratorPath $noticeGenerator 'NOTICE\*.*COPYRIGHT\*.*AUTHORS\*.*PATENTS\*' 'discover the approved supplemental notice file families'
Require-Match $noticeGeneratorPath $noticeGenerator 'SHA-256 \(normalized UTF-8\)' 'record normalized supplemental notice content hashes'
if ($null -ne $noticeGenerator -and $noticeGenerator -match '(?m)^\s*Remove-Item\b') {
    Add-Failure "$noticeGeneratorPath must retain every fresh generation directory"
}
if ($null -ne $noticeGenerator -and $noticeGenerator -match '(?m)^\s*[^#\r\n]*--(?:offline|frozen)\b') {
    Add-Failure "$noticeGeneratorPath authoritative generation must remain online-capable"
}

$noticeValidatorPath = "scripts/validate-third-party-notices.ps1"
$noticeValidator = Read-RequiredFile $noticeValidatorPath
Require-Match $noticeValidatorPath $noticeValidator '0\.9\.1' 'pin and require cargo-about 0.9.1'
Require-Match $noticeValidatorPath $noticeValidator 'THIRD_PARTY_LICENSES\.html' 'compare the generated license report with its tracked snapshot'
Require-Match $noticeValidatorPath $noticeValidator 'THIRD_PARTY_NOTICES\.txt' 'compare the generated supplemental notices with its tracked snapshot'
Require-Match $noticeValidatorPath $noticeValidator 'target[\\/]third-party-notices' 'retain fresh validation output under target/third-party-notices'
Require-Match $noticeValidatorPath $noticeValidator 'StructuralEqualityComparer' 'byte-compare regenerated files with tracked snapshots'
Require-Match $noticeValidatorPath $noticeValidator 'SHA256' 'hash-compare regenerated files with tracked snapshots'
foreach ($inactiveHttp3Crate in @("quinn", "quinn-proto", "quinn-udp", "lru-slab")) {
    Require-Match $noticeValidatorPath $noticeValidator ([regex]::Escape('"' + $inactiveHttp3Crate + '"')) "reject inactive reqwest HTTP/3 dependency $inactiveHttp3Crate"
}

[void](Read-RequiredFile "THIRD_PARTY_LICENSES.html")
[void](Read-RequiredFile "THIRD_PARTY_NOTICES.txt")

$editorValidatorPath = "scripts/validate-editor-assets.ps1"
$editorValidator = Read-RequiredFile $editorValidatorPath
Require-Match $editorValidatorPath $editorValidator 'package\.license\s+-cne\s+"Apache-2\.0"' 'enforce the VS Code extension license independently'

$storeValidatorPath = "scripts/validate-store-packaging.ps1"
$storeValidator = Read-RequiredFile $storeValidatorPath
Require-Match $storeValidatorPath $storeValidator '"\*/LICENSE"' 'require LICENSE in portable Unix archives'
Require-Match $storeValidatorPath $storeValidator 'usr/share/doc/ricochet/LICENSE\$' 'require LICENSE in Debian packages'
Require-Match $storeValidatorPath $storeValidator '(?s)"windows-x64".*?"THIRD_PARTY_LICENSES\.html".*?"THIRD_PARTY_NOTICES\.txt"' 'require both third-party snapshots in Windows ZIP roots'
Require-Match $storeValidatorPath $storeValidator '\$entry\s+-clike\s+\$pattern' 'match exact and wildcard archive entries case-sensitively'
Require-Match $storeValidatorPath $storeValidator '(?s)function Assert-EntriesContainRegex.*?\$entry\s+-cmatch\s+\$pattern' 'provide case-sensitive regex matching for exact Unix archive depth'
Require-Match $storeValidatorPath $storeValidator '(?s)"linux-x64".*?\^\[\^/\]\+/THIRD_PARTY_LICENSES\\\.html\$.*?\^\[\^/\]\+/THIRD_PARTY_NOTICES\\\.txt\$' 'require exact case-sensitive one-root third-party snapshots in Linux tar roots'
Require-Match $storeValidatorPath $storeValidator 'usr/share/doc/ricochet/THIRD_PARTY_LICENSES\\\.html\$' 'require the third-party license report in Debian packages'
Require-Match $storeValidatorPath $storeValidator 'usr/share/doc/ricochet/THIRD_PARTY_NOTICES\\\.txt\$' 'require supplemental notices in Debian packages'
Require-Match $storeValidatorPath $storeValidator '(?s)function Assert-DebContains.*?\$_\s+-cmatch\s+\$Pattern' 'match Debian package paths case-sensitively'
Require-Match $storeValidatorPath $storeValidator '(?s)macos-arm64.*?macos-x64.*?\^\[\^/\]\+/THIRD_PARTY_LICENSES\\\.html\$.*?\^\[\^/\]\+/THIRD_PARTY_NOTICES\\\.txt\$' 'require exact case-sensitive one-root third-party snapshots in macOS tar roots'

$storeEntryContractPath = "scripts/test-store-packaging-entry-contract.ps1"
[void](Read-RequiredFile $storeEntryContractPath)
try {
    & (Join-Path $Root $storeEntryContractPath)
} catch {
    $Failures.Add("${storeEntryContractPath}: $($_.Exception.Message)") | Out-Null
}

$debianVersionContractPath = "scripts/test-debian-version-contract.ps1"
[void](Read-RequiredFile $debianVersionContractPath)
try {
    & (Join-Path $Root $debianVersionContractPath)
} catch {
    $Failures.Add("${debianVersionContractPath}: $($_.Exception.Message)") | Out-Null
}

$releaseWorkflowPath = ".github/workflows/release.yml"
$releaseWorkflow = Read-RequiredFile $releaseWorkflowPath
Require-Match $releaseWorkflowPath $releaseWorkflow 'cmp LICENSE "\$package_dir/LICENSE"' 'compare the Linux archive license with the repository license'
Require-Match $releaseWorkflowPath $releaseWorkflow '<project_license>Apache-2\.0</project_license>' 'verify Ricochet AppStream license metadata'
Require-Match $releaseWorkflowPath $releaseWorkflow 'cmp LICENSE "\$tmp/deb-root/usr/share/doc/ricochet/LICENSE"' 'compare the Debian license with the repository license'
Require-Match $releaseWorkflowPath $releaseWorkflow 'Windows portable ZIP LICENSE did not match the repository license' 'compare the Windows archive license with the repository license'
Require-Match $releaseWorkflowPath $releaseWorkflow 'ricochet-v\*-\$\{\{ matrix\.target \}\}\.tar\.gz[\s\S]+?cmp LICENSE "\$package_dir/LICENSE"' 'compare each macOS archive license with the repository license'
Require-Match $releaseWorkflowPath $releaseWorkflow 'Smoke-test Windows installer' 'execute the disposable Windows installer smoke after portable package testing'
Require-Match $releaseWorkflowPath $releaseWorkflow 'ricochet-installer-smoke-' 'use a unique RUNNER_TEMP root for Windows installer testing'
Require-Match $releaseWorkflowPath $releaseWorkflow '"/S /D=\$installDir"' 'silently install NSIS into the generated unquoted destination'
Require-Match $releaseWorkflowPath $releaseWorkflow 'AddSeconds\(30\)' 'bound Windows uninstaller state polling to 30 seconds'
Require-Match $releaseWorkflowPath $releaseWorkflow 'sha256sum' 'hash-compare Linux tar and Debian notice copies'
Require-Match $releaseWorkflowPath $releaseWorkflow 'shasum -a 256' 'hash-compare both macOS notice copies'

$releaseWorkflowContractPath = "scripts/test-release-workflow-contract.ps1"
[void](Read-RequiredFile $releaseWorkflowContractPath)
try {
    & (Join-Path $Root $releaseWorkflowContractPath)
} catch {
    $Failures.Add("${releaseWorkflowContractPath}: $($_.Exception.Message)") | Out-Null
}

$windowsPackagerPath = "scripts/package-release.ps1"
$windowsPackager = Read-RequiredFile $windowsPackagerPath
Require-Match $windowsPackagerPath $windowsPackager 'Copy-Item[^\r\n]+"LICENSE"' 'copy the repository license into Windows packages'
Require-Match $windowsPackagerPath $windowsPackager 'Copy-Item[^\r\n]+"THIRD_PARTY_LICENSES\.html"[^\r\n]+\$PackageDir' 'copy the third-party license report into Windows packages'
Require-Match $windowsPackagerPath $windowsPackager 'Copy-Item[^\r\n]+"THIRD_PARTY_NOTICES\.txt"[^\r\n]+\$PackageDir' 'copy supplemental notices into Windows packages'

$windowsInstallerPath = "packaging/windows/ricochet.nsi"
$windowsInstaller = Read-RequiredFile $windowsInstallerPath
Require-Match $windowsInstallerPath $windowsInstaller 'Third-Party Licenses\.lnk" "\$INSTDIR\\THIRD_PARTY_LICENSES\.html"' 'create one Start Menu shortcut to the installed third-party license report'

$macosPackagerPath = "scripts/package-release-macos.sh"
$macosPackager = Read-RequiredFile $macosPackagerPath
Require-Match $macosPackagerPath $macosPackager 'cp "\$repo_root/LICENSE" "\$package_dir/LICENSE"' 'copy the repository license into macOS packages'
Require-Match $macosPackagerPath $macosPackager 'cp "\$repo_root/THIRD_PARTY_LICENSES\.html" "\$package_dir/THIRD_PARTY_LICENSES\.html"' 'copy the third-party license report into macOS archives'
Require-Match $macosPackagerPath $macosPackager 'cp "\$repo_root/THIRD_PARTY_NOTICES\.txt" "\$package_dir/THIRD_PARTY_NOTICES\.txt"' 'copy supplemental notices into macOS archives'
Require-Match $macosPackagerPath $macosPackager 'cp "\$script_dir/LICENSE" "\$doc_dir/LICENSE"' 'install the repository license under the macOS prefix documentation directory'
Require-Match $macosPackagerPath $macosPackager 'cp "\$script_dir/THIRD_PARTY_LICENSES\.html" "\$doc_dir/THIRD_PARTY_LICENSES\.html"' 'install the third-party license report under the macOS prefix documentation directory'
Require-Match $macosPackagerPath $macosPackager 'cp "\$script_dir/THIRD_PARTY_NOTICES\.txt" "\$doc_dir/THIRD_PARTY_NOTICES\.txt"' 'install supplemental notices under the macOS prefix documentation directory'

Require-Match $linuxPackagerPath $linuxPackager 'cp "\$repo_root/LICENSE" "\$package_dir/LICENSE"' 'copy the repository license into Linux archives'
Require-Match $linuxPackagerPath $linuxPackager 'cp "\$repo_root/LICENSE" "\$deb_root/usr/share/doc/ricochet/LICENSE"' 'copy the repository license into Debian packages'
Require-Match $linuxPackagerPath $linuxPackager 'cp "\$repo_root/THIRD_PARTY_LICENSES\.html" "\$package_dir/THIRD_PARTY_LICENSES\.html"' 'copy the third-party license report into Linux archives'
Require-Match $linuxPackagerPath $linuxPackager 'cp "\$repo_root/THIRD_PARTY_NOTICES\.txt" "\$package_dir/THIRD_PARTY_NOTICES\.txt"' 'copy supplemental notices into Linux archives'
Require-Match $linuxPackagerPath $linuxPackager 'cp "\$repo_root/THIRD_PARTY_LICENSES\.html" "\$deb_root/usr/share/doc/ricochet/THIRD_PARTY_LICENSES\.html"' 'copy the third-party license report into Debian packages'
Require-Match $linuxPackagerPath $linuxPackager 'cp "\$repo_root/THIRD_PARTY_NOTICES\.txt" "\$deb_root/usr/share/doc/ricochet/THIRD_PARTY_NOTICES\.txt"' 'copy supplemental notices into Debian packages'
Require-Match $linuxPackagerPath $linuxPackager 'cp "\$script_dir/LICENSE" "\$doc_dir/LICENSE"' 'install the repository license under the Linux prefix documentation directory'
Require-Match $linuxPackagerPath $linuxPackager 'cp "\$script_dir/THIRD_PARTY_LICENSES\.html" "\$doc_dir/THIRD_PARTY_LICENSES\.html"' 'install the third-party license report under the Linux prefix documentation directory'
Require-Match $linuxPackagerPath $linuxPackager 'cp "\$script_dir/THIRD_PARTY_NOTICES\.txt" "\$doc_dir/THIRD_PARTY_NOTICES\.txt"' 'install supplemental notices under the Linux prefix documentation directory'

foreach ($publicStoreGuidePath in @(
    "docs/reference/guides/store-packaging.html",
    "docs/wiki/store-packaging.html"
)) {
    $publicStoreGuide = Read-RequiredFile $publicStoreGuidePath
    Require-Match $publicStoreGuidePath $publicStoreGuide 'THIRD_PARTY_LICENSES\.html' 'name the bundled third-party license report'
    Require-Match $publicStoreGuidePath $publicStoreGuide 'THIRD_PARTY_NOTICES\.txt' 'name the bundled supplemental notices'
    Require-Match $publicStoreGuidePath $publicStoreGuide 'share/doc/ricochet' 'document the installed Unix disclosure path'
}

if ($Failures.Count -gt 0) {
    $details = $Failures | ForEach-Object { " - $_" }
    throw "License and governance validation failed:`n$($details -join "`n")"
}

Write-Host "License and governance validation passed."
