param(
    [string]$Root = (Split-Path -Parent $MyInvocation.MyCommand.Path)
)

$ErrorActionPreference = "Stop"

$requiredFiles = @(
    "index.html",
    "styles.css",
    "app.js",
    "README.md"
)

$failures = New-Object System.Collections.Generic.List[string]

foreach ($file in $requiredFiles) {
    $path = Join-Path $Root $file
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        $failures.Add("Missing required docs file: $file")
    }
}

if ($failures.Count -eq 0) {
    $index = Get-Content -LiteralPath (Join-Path $Root "index.html") -Raw
    $styles = Get-Content -LiteralPath (Join-Path $Root "styles.css") -Raw
    $app = Get-Content -LiteralPath (Join-Path $Root "app.js") -Raw
    $readme = Get-Content -LiteralPath (Join-Path $Root "README.md") -Raw

    $requiredIndexMarkers = @(
        "Ricochet Reference",
        "id=""syntax""",
        "id=""words""",
        "id=""oop""",
        "id=""mvc""",
        "id=""active-record""",
        "id=""turing-complete""",
        "id=""debugging""",
        "id=""cli""",
        "id=""limits"""
    )

    foreach ($marker in $requiredIndexMarkers) {
        if (-not $index.Contains($marker)) {
            $failures.Add("index.html is missing marker: $marker")
        }
    }

    $requiredWords = @(
        '"+"',
        '"add"',
        '"-"',
        '"subtract"',
        '"*"',
        '"/"',
        '"%"',
        '"negate"',
        '"abs"',
        '"min"',
        '"max"',
        '"clamp"',
        '"not"',
        '"and"',
        '"or"',
        '"equals"',
        '"not-equals?"',
        '"assert"',
        '"assert-true"',
        '"assert-false"',
        '"assert-equals"',
        '"assert-ok"',
        '"assert-error"',
        '"less-than?"',
        '"greater-than?"',
        '"less-or-equals?"',
        '"greater-or-equals?"',
        '"self"',
        '"get"',
        '"set"',
        '"var"',
        '"Field"',
        '"Table"',
        '"Subclass"',
        '"new"',
        '"swap"',
        '"dup"',
        '"drop"',
        '"over"',
        '"rot"',
        '"nip"',
        '"tuck"',
        '"pick"',
        '"roll"',
        '"depth"',
        '"clear"',
        '"call"',
        '"spawn"',
        '"await"',
        '"await-all"',
        '"release-task"',
        '"tasks"',
        '"while"',
        '"break"',
        '"continue"',
        '"send"',
        '"println"',
        '"view"',
        '"text"',
        '"json"',
        '"redirect"',
        '"status"',
        '"header"',
        '"value"',
        '"error"',
        '"array"',
        '"list"',
        '"map"',
        '"Set"',
        '"range"',
        '"push!"',
        '"put!"',
        '"push!"',
        '"put!"',
        '"insert!"',
        '"remove!"',
        '"remove-at!"',
        '"clear!"',
        '"count"',
        '"at"',
        '"first"',
        '"last"',
        '"take"',
        '"skip"',
        '"reverse"',
        '"has?"',
        '"keys"',
        '"values"',
        '"each"',
        '"transform"',
        '"select"',
        '"reduce"',
        '"find"',
        '"default-page"',
        '"limit"',
        '"any?"',
        '"all?"',
        '"join"',
        '"trim"',
        '"trim-start"',
        '"trim-end"',
        '"blank?"',
        '"slice"',
        '"index-of"',
        '"last-index-of"',
        '"repeat"',
        '"lines"',
        '"chars"',
        '"split"',
        '"replace"',
        '"contains?"',
        '"starts-with?"',
        '"ends-with?"',
        '"uppercase"',
        '"lowercase"',
        '"concat"',
        '"to-number"',
        '"to-string"',
        '"json-encode"',
        '"json-decode"',
        '"regex"',
        '"matches?"',
        '"captures"',
        '"Method"',
        '"ok?"',
        '"ok"',
        '"fail"',
        '"error?"',
        '"unwrap-or"',
        '"map-result"',
        '"and-then"',
        '"result_envelope"',
        '"nil?"',
        '"empty?"',
        '"print"',
        '"eprint"',
        '"read-line"',
        '"args"',
        '"env"',
        '"cwd"',
        '"runtime_capabilities"',
        '"info"',
        '"process_spawn"',
        '"process_spawn_task"',
        '"process_start"',
        '"process_jobs"',
        '"process_job"',
        '"process_cancel"',
        '"process_read"',
        '"pty_start"',
        '"pty_write"',
        '"pty_read"',
        '"pty_resize"',
        '"pty_stop"',
        '"pty_list"',
        '"pty_detail"',
        '"approval_create"',
        '"approval_claim"',
        '"approval_complete"',
        '"approval_reject"',
        '"approval_detail"',
        '"now"',
        '"sleep"',
        '"random"',
        '"exit"',
        '"fs_read_text"',
        '"fs_write_text"',
        '"fs_exists?"',
        '"fs_list"',
        '"fs_create_dir"',
        '"workspace_resolve"',
        '"workspace_contains?"',
        '"workspace_metadata"',
        '"workspace_list"',
        '"workspace_read_text"',
        '"workspace_write_text"',
        '"workspace_mkdir"',
        '"workspace_copy"',
        '"workspace_move"',
        '"http_get"',
        '"http_get_task"',
        '"http_request"',
        '"http_post_json"',
        '"http_post_json_task"',
        '"http_request_task"',
        '"tui_enter"',
        '"tui_leave"',
        '"tui_move_to"',
        '"tui_write"',
        '"tui_flush"',
        '"tui_size"',
        '"tui_poll_key"',
        '"tui_read_key"',
        '"webview_text"',
        '"webview_heading"',
        '"webview_button"',
        '"webview_input"',
        '"webview_link"',
        '"webview_container"',
        '"webview_window"',
        '"webview_document"',
        '"inspect"',
        '"debug"',
        '"type"',
        '"class-of"',
        '"instance-of?"',
        '"responds-to?"',
        '"id"',
        '"status"',
        '"pending?"',
        '"fields"',
        '"methods"',
        '"callable?"'
    )

    foreach ($word in $requiredWords) {
        if (-not $app.Contains($word)) {
            $failures.Add("app.js is missing reference entry for: $word")
        }
    }

    $requiredExamples = @(
        "User Model Subclass",
        "HomeController Controller Subclass",
        'GET "/" HomeController "index" route',
        "User all",
        "10 User limit",
        "User count",
        "User first",
        "1 User exists?",
        "users array",
        "settings map",
        "tags Set",
        "push!",
        "put!",
        "slice",
        "regex value",
        "[web.static]",
        "mount = `"/assets`"",
        "className get `"Object`" Subclass",
        "multiplier get 0 &gt; while",
        "rco run --debug --step app.rco",
        "rco debug --json app.rco",
        "rco run --trace-file trace.json app.rco",
        "step``, ``next``, ``out``, ``continue``",
        "``self``, or ``tasks``",
        "rco fmt [--check] [path]",
        "rco run [--debug] [--step] [--breakpoint LINE] [--trace-file PATH] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] &lt;path&gt; [args...]",
        "rco debug [--json] [--step] [--breakpoint LINE] [--trace-file PATH] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] &lt;path&gt; [args...]",
        "rco run-bytecode [--debug] [--trace-file PATH] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] &lt;path&gt; [args...]",
        "rco gui [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] &lt;path&gt; [args...]",
        "rco tui [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] &lt;path&gt; [args...]",
        "rco package [path] --output &lt;exe&gt; [--tui] [--gui] [--mvc] [--gui-launcher PATH] [--linux-package tar|deb] [--package-name NAME] [--package-version VERSION]",
        "rco add &lt;source&gt; [--name NAME] [--registry PATH] [--version REQ] [--no-fetch]",
        "rco publish [path] --registry PATH [--provenance-file PATH] [--signature-file PATH] [--signature-kind KIND] [--dry-run]",
        "rco verify [path]",
        "rco audit [path] [--json]",
        "rco doctor [--capabilities] [path]",
        "rco lsp-diagnostics [--pretty] &lt;path&gt;",
        "rco migrate status [path]",
        "rco migrate apply [path]",
        "`"lib/math`" import",
        "`"greeter/greeting`" import",
        "[dependencies.greeter]",
        "rco add registry:greeter --registry ../ricochet-registry --version `"^0.2.0`"",
        "--provenance-file provenance.json --signature-file greeter.sig --signature-kind minisign",
        "rco audit --json",
        "rco add github:BARKx4/ricochet_auth@v0.1.0 --no-fetch",
        "`"/dashboard`" redirect",
        "rco routes [path]",
        "rco serve [--host HOST] [--port PORT] [--debug] [--watch] [--allow-env] [--no-env] [--env-allow NAME] [--allow-process] [--process-root PATH] [--allow-pty] [--fs-root PATH] [--fs-readonly] [--http-allow-host HOST]",
        "rco test [--debug] [--filter PATTERN] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] [path]",
        "task get status",
        "task get info",
        "task get release-task",
        "tui_read_key value",
        "`"Counter`" 1 webview_heading",
        "`"Increment`" `"increment`" webview_button",
        "tasks count",
        "fs_read_text",
        "workspace_read_text",
        "workspace_list",
        "http_get",
        "http_get_task",
        "process_spawn",
        "process_start",
        "process_read",
        "pty_start",
        "pty_read",
        "approval_create",
        "approval_claim",
        "result_envelope",
        "runtime_capabilities",
        "tui_write",
        "{ `$user name.get }"
    )

    foreach ($example in $requiredExamples) {
        if (-not $index.Contains($example) -and -not $app.Contains($example)) {
            $failures.Add("Docs are missing example text: $example")
        }
    }

    $requiredCss = @(
        "--ink",
        ".word-grid",
        ".stack-rail",
        "@media"
    )

    foreach ($marker in $requiredCss) {
        if (-not $styles.Contains($marker)) {
            $failures.Add("styles.css is missing marker: $marker")
        }
    }

    if (-not $readme.Contains("Open index.html")) {
        $failures.Add("README.md does not explain how to open the static site")
    }
}

if ($failures.Count -gt 0) {
    foreach ($failure in $failures) {
        Write-Error $failure
    }
    exit 1
}

Write-Host "Ricochet reference docs validation passed."
