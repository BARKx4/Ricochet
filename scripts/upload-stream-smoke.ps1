param(
    [string]$Rco,
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

if ([string]::IsNullOrWhiteSpace($TempRoot)) {
    $TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ricochet-upload-smoke-" + [System.Guid]::NewGuid().ToString("N"))
}

if ($Port -eq 0) {
    $Port = Get-FreeTcpPort
}

$project = Join-Path $TempRoot "upload_app"
New-Item -ItemType Directory -Path (Join-Path $project "config") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $project "app\Controllers") -Force | Out-Null

[System.IO.File]::WriteAllText(
    (Join-Path $project "ricochet.toml"),
@'
[package]
name = "upload_stream_smoke"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.uploads]
max_request_bytes = 6291456
max_file_bytes = 5242880
memory_threshold_bytes = 1024
max_retained_streams = 2
'@
)

[System.IO.File]::WriteAllText(
    (Join-Path $project "config\routes.rco"),
    'POST "/upload" UploadController "create" route'
)

[System.IO.File]::WriteAllText(
    (Join-Path $project "app\Controllers\UploadController.rco"),
@'
UploadController Controller Subclass
  ( file ) [
    file var

    map options var
    options get "max_bytes" 32 put! drop
    file get "stream_id" at options get upload_read value read var

    map response var
    response get "stream_count_before_release" upload_streams count put! drop
    response get "stream_id" file get "stream_id" at put! drop
    response get "filename" file get "filename" at put! drop
    response get "size" file get "size" at put! drop
    response get "text_is_nil" file get "text" at nil? put! drop
    response get "data_base64_is_nil" file get "data_base64" at nil? put! drop
    response get "read_len" read get "bytes_len" at put! drop
    response get "read_text" read get "text" at put! drop
    file get "stream_id" at upload_release value released var
    response get "released" released get put! drop
    response get "stream_count_after_release" upload_streams count put! drop
    response get json
  ] "create" Method
end
'@
)

$url = "http://127.0.0.1:$Port/upload"
$process = $null
$client = $null

try {
    Write-Host "==> upload stream smoke $url"
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Rco
    $startInfo.Arguments = "serve --host 127.0.0.1 --port $Port"
    $startInfo.WorkingDirectory = $project
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
            throw "rco serve exited before upload smoke with code $($process.ExitCode)`nstdout:`n$stdout`nstderr:`n$stderr"
        }

        $tcp = $null
        try {
            $tcp = [System.Net.Sockets.TcpClient]::new()
            $connect = $tcp.BeginConnect("127.0.0.1", $Port, $null, $null)
            if ($connect.AsyncWaitHandle.WaitOne(250)) {
                $tcp.EndConnect($connect)
                $ready = $true
                break
            }
        }
        catch {
        }
        finally {
            if ($null -ne $tcp) {
                $tcp.Dispose()
            }
        }

        Start-Sleep -Milliseconds 250
    }

    if (-not $ready) {
        if ($process.HasExited) {
            $stdout = $process.StandardOutput.ReadToEnd()
            $stderr = $process.StandardError.ReadToEnd()
            throw "rco serve exited before upload smoke with code $($process.ExitCode)`nstdout:`n$stdout`nstderr:`n$stderr"
        }

        try {
            $probe = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/" -UseBasicParsing -TimeoutSec 2
            if ($null -ne $probe) {
                $ready = $true
            }
        }
        catch {
            if ($null -ne $_.Exception.Response) {
                $ready = $true
            }
        }
    }

    if (-not $ready) {
        throw "rco serve did not start within $TimeoutSeconds seconds at $url"
    }

    Add-Type -AssemblyName System.Net.Http
    $client = [System.Net.Http.HttpClient]::new()
    $content = [System.Net.Http.MultipartFormDataContent]::new()
    $fileBytes = [byte[]]::new(3145728)
    for ($i = 0; $i -lt $fileBytes.Length; $i++) {
        $fileBytes[$i] = [byte][char]'a'
    }
    $fileContent = [System.Net.Http.ByteArrayContent]::new($fileBytes)
    $fileContent.Headers.ContentType = [System.Net.Http.Headers.MediaTypeHeaderValue]::Parse("text/plain")
    $content.Add($fileContent, "file", "large.txt")

    $response = $client.PostAsync($url, $content).GetAwaiter().GetResult()
    $body = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
    if (-not $response.IsSuccessStatusCode) {
        throw "Expected upload smoke HTTP success, got $([int]$response.StatusCode): $body"
    }

    $json = $body | ConvertFrom-Json
    if ($json.stream_count_before_release -ne 1) {
        throw "Expected one retained upload stream before release, got: $body"
    }
    if ($json.stream_count_after_release -ne 0) {
        throw "Expected no retained upload streams after release, got: $body"
    }
    if ($json.text_is_nil -ne $true -or $json.data_base64_is_nil -ne $true) {
        throw "Expected large upload to omit in-memory compatibility bytes, got: $body"
    }
    if ($json.read_len -ne 32 -or $json.read_text -ne ("a" * 32)) {
        throw "Expected upload_read to return first 32 bytes, got: $body"
    }
    if ($json.released -ne $true) {
        throw "Expected upload_release to report true, got: $body"
    }

    Write-Host "Ricochet upload stream smoke passed at $url"
}
finally {
    if ($null -ne $client) {
        $client.Dispose()
    }
    if ($null -ne $process) {
        if (-not $process.HasExited) {
            $process.Kill()
            $process.WaitForExit(5000) | Out-Null
        }
        $process.Dispose()
    }
}
