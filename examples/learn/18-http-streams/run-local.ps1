$ErrorActionPreference = 'Stop'

$RepoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')

$Probe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$Probe.Start()
$Port = $Probe.LocalEndpoint.Port
$Probe.Stop()

$ServerScript = @"
`$ErrorActionPreference = 'Stop'
`$Port = $Port
`$Listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, `$Port)
`$Listener.Start()

try {
    while (`$true) {
        `$Client = `$Listener.AcceptTcpClient()
        try {
            `$Stream = `$Client.GetStream()
            `$Reader = [System.IO.StreamReader]::new(`$Stream, [System.Text.Encoding]::ASCII, `$false, 1024, `$true)
            `$RequestLine = `$Reader.ReadLine()
            if ([string]::IsNullOrWhiteSpace(`$RequestLine)) {
                continue
            }

            `$Headers = @{}
            while (`$true) {
                `$Line = `$Reader.ReadLine()
                if (`$null -eq `$Line -or `$Line.Length -eq 0) {
                    break
                }

                `$Parts = `$Line.Split(':', 2)
                if (`$Parts.Length -eq 2) {
                    `$Headers[`$Parts[0].Trim()] = `$Parts[1].Trim()
                }
            }

            if (`$Headers.ContainsKey('Content-Length')) {
                `$Length = [int]`$Headers['Content-Length']
                if (`$Length -gt 0) {
                    `$Buffer = New-Object char[] `$Length
                    [void]`$Reader.ReadBlock(`$Buffer, 0, `$Length)
                }
            }

            `$Target = `$RequestLine.Split(' ')[1]
            `$Status = '200 OK'
            `$ContentType = 'text/plain; charset=utf-8'
            `$Body = 'ok'

            switch (`$Target) {
                '/ping' {
                    `$Body = 'pong'
                }
                '/messages' {
                    `$Status = '201 Created'
                    `$Body = 'created'
                }
                '/structured' {
                    `$Status = '202 Accepted'
                    `$Body = 'structured'
                }
                '/task' {
                    `$Body = 'task-ok'
                }
                '/post-task' {
                    `$Status = '201 Created'
                    `$Body = 'task-created'
                }
                '/request-task' {
                    `$Body = 'request-ok'
                }
                '/stream' {
                    `$Body = 'alpha|beta|gamma'
                }
                default {
                    `$Status = '404 Not Found'
                    `$Body = 'missing'
                }
            }

            `$BodyBytes = [System.Text.Encoding]::UTF8.GetBytes(`$Body)
            `$Header = "HTTP/1.1 `$Status``r``nContent-Type: `$ContentType``r``nContent-Length: `$(`$BodyBytes.Length)``r``nConnection: close``r``nX-Learn-Path: `$Target``r``n``r``n"
            `$HeaderBytes = [System.Text.Encoding]::ASCII.GetBytes(`$Header)
            `$Stream.Write(`$HeaderBytes, 0, `$HeaderBytes.Length)
            `$Stream.Write(`$BodyBytes, 0, `$BodyBytes.Length)
            `$Stream.Flush()
        }
        finally {
            `$Client.Dispose()
        }
    }
}
finally {
    `$Listener.Stop()
}
"@

$EncodedServerScript = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($ServerScript))
$ServerProcess = Start-Process -FilePath powershell.exe -ArgumentList @('-NoProfile', '-EncodedCommand', $EncodedServerScript) -WindowStyle Hidden -PassThru

Start-Sleep -Milliseconds 250

$BaseUrl = "http://127.0.0.1:$Port"

try {
    Push-Location $RepoRoot
    & cargo run -q -p ricochet_cli --bin rco -- run --capability-profile trusted --http-allow-host 127.0.0.1 examples/learn/18-http-streams/api-client.rco $BaseUrl
    if ($LASTEXITCODE -ne 0) {
        throw "Ricochet HTTP example failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
    if ($ServerProcess -and -not $ServerProcess.HasExited) {
        Stop-Process -Id $ServerProcess.Id -Force
    }
}
