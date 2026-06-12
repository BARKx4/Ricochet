param(
    [string]$Rco,
    [string]$Project,
    [string]$TempRoot,
    [int]$Port = 0,
    [int]$TimeoutSeconds = 15
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if ([string]::IsNullOrWhiteSpace($Rco)) {
    $Rco = Join-Path $Root "target\debug\rco.exe"
}

if (-not (Test-Path -LiteralPath $Rco -PathType Leaf)) {
    throw "Could not find rco at '$Rco'. Build it first with: cargo build -p ricochet_cli --bin rco"
}

function Get-FreeTcpPort {
    $address = [System.Net.IPAddress]::Parse("127.0.0.1")
    $listener = [System.Net.Sockets.TcpListener]::new($address, 0)
    try {
        $listener.Start()
        return $listener.LocalEndpoint.Port
    }
    finally {
        $listener.Stop()
    }
}

function Invoke-RcoChecked {
    param(
        [string]$Name,
        [string[]]$Arguments
    )

    Write-Host "==> $Name"
    & $Rco @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

if ([string]::IsNullOrWhiteSpace($Project)) {
    if ([string]::IsNullOrWhiteSpace($TempRoot)) {
        $TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ricochet-live-smoke-" + [System.Guid]::NewGuid().ToString("N"))
    }
    New-Item -ItemType Directory -Path $TempRoot -Force | Out-Null
    $Project = Join-Path $TempRoot "app"
    Invoke-RcoChecked "scaffold live-smoke app" @("new", $Project)
}

if (-not (Test-Path -LiteralPath (Join-Path $Project "ricochet.toml") -PathType Leaf)) {
    throw "Project does not look like a Ricochet app: $Project"
}

if ($Port -eq 0) {
    $Port = Get-FreeTcpPort
}

$url = "http://127.0.0.1:$Port/"
$process = $null
$response = $null

try {
    Write-Host "==> live server smoke $url"
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Rco
    $startInfo.Arguments = "serve --host 127.0.0.1 --port $Port"
    $startInfo.WorkingDirectory = $Project
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.CreateNoWindow = $true

    $process = [System.Diagnostics.Process]::Start($startInfo)
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)

    while ((Get-Date) -lt $deadline) {
        if ($process.HasExited) {
            $stdout = $process.StandardOutput.ReadToEnd()
            $stderr = $process.StandardError.ReadToEnd()
            throw "rco serve exited before responding with code $($process.ExitCode)`nstdout:`n$stdout`nstderr:`n$stderr"
        }

        try {
            $response = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 2
            if ($response.StatusCode -eq 200) {
                break
            }
        }
        catch {
            Start-Sleep -Milliseconds 250
        }
    }

    if ($null -eq $response) {
        throw "rco serve did not respond within $TimeoutSeconds seconds at $url"
    }

    if ($response.StatusCode -ne 200) {
        throw "Expected HTTP 200 from $url, got $($response.StatusCode)"
    }

    if (-not $response.Content.Contains("Hello Ricochet")) {
        throw "Expected scaffold home page to contain 'Hello Ricochet'; body was:`n$($response.Content)"
    }

    Write-Host "Ricochet live server smoke passed at $url"
}
finally {
    if ($null -ne $process) {
        if (-not $process.HasExited) {
            $process.Kill()
            $process.WaitForExit(5000) | Out-Null
        }
        $process.Dispose()
    }
}
