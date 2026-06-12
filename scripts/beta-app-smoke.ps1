param(
    [string]$Rco,
    [string]$Project,
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

if ([string]::IsNullOrWhiteSpace($Project)) {
    throw "-Project is required"
}

if (-not (Test-Path -LiteralPath (Join-Path $Project "ricochet.toml") -PathType Leaf)) {
    throw "Project does not look like a Ricochet app: $Project"
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

function Assert-Contains {
    param(
        [string]$Name,
        [string]$Body,
        [string]$Expected
    )

    if (-not $Body.Contains($Expected)) {
        throw "Expected $Name to contain '$Expected'; body was:`n$Body"
    }
}

if ($Port -eq 0) {
    $Port = Get-FreeTcpPort
}

$baseUrl = "http://127.0.0.1:$Port"
$process = $null

try {
    Write-Host "==> beta app smoke $baseUrl"
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
    $ready = $false

    while ((Get-Date) -lt $deadline) {
        if ($process.HasExited) {
            $stdout = $process.StandardOutput.ReadToEnd()
            $stderr = $process.StandardError.ReadToEnd()
            throw "rco serve exited before responding with code $($process.ExitCode)`nstdout:`n$stdout`nstderr:`n$stderr"
        }

        try {
            $response = Invoke-WebRequest -Uri "$baseUrl/users" -UseBasicParsing -TimeoutSec 2
            if ($response.StatusCode -eq 200) {
                Assert-Contains "/users" $response.Content "ada@example.com"
                $ready = $true
                break
            }
        }
        catch {
            Start-Sleep -Milliseconds 250
        }
    }

    if (-not $ready) {
        throw "rco serve did not respond within $TimeoutSeconds seconds at $baseUrl"
    }

    $session = [Microsoft.PowerShell.Commands.WebRequestSession]::new()

    $meBefore = Invoke-WebRequest -Uri "$baseUrl/me" -UseBasicParsing -WebSession $session -TimeoutSec 2
    Assert-Contains "/me before login" $meBefore.Content "Not signed in"

    $login = Invoke-WebRequest `
        -Uri "$baseUrl/login" `
        -Method Post `
        -Body "email=ada%40example.com" `
        -ContentType "application/x-www-form-urlencoded" `
        -UseBasicParsing `
        -WebSession $session `
        -TimeoutSec 2
    Assert-Contains "POST /login" $login.Content "Signed in as ada@example.com"

    $meAfter = Invoke-WebRequest -Uri "$baseUrl/me" -UseBasicParsing -WebSession $session -TimeoutSec 2
    Assert-Contains "/me after login" $meAfter.Content "Signed in as ada@example.com"

    $logout = Invoke-WebRequest `
        -Uri "$baseUrl/logout" `
        -Method Post `
        -UseBasicParsing `
        -WebSession $session `
        -TimeoutSec 2
    Assert-Contains "POST /logout" $logout.Content "Sign in"

    $meAfterLogout = Invoke-WebRequest -Uri "$baseUrl/me" -UseBasicParsing -WebSession $session -TimeoutSec 2
    Assert-Contains "/me after logout" $meAfterLogout.Content "Not signed in"

    Write-Host "Ricochet beta app smoke passed at $baseUrl"
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
