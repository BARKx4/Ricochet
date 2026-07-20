param(
    [string]$Root = (Split-Path -Parent $MyInvocation.MyCommand.Path),
    [switch]$PackageMode
)

$ErrorActionPreference = "Stop"

$docsRoot = Split-Path -Parent $Root
$docsRootFull = [System.IO.Path]::GetFullPath($docsRoot)

function Get-DocsRelativePath {
    param([string]$Path)

    $target = [System.IO.Path]::GetFullPath($Path)
    if ([System.IO.Path].GetMethods().Name -contains "GetRelativePath") {
        return [System.IO.Path]::GetRelativePath($docsRootFull, $target).Replace("\", "/")
    }

    $base = $docsRootFull
    if (-not $base.EndsWith([System.IO.Path]::DirectorySeparatorChar)) {
        $base = $base + [System.IO.Path]::DirectorySeparatorChar
    }
    $baseUri = [System.Uri]$base
    $targetUri = [System.Uri]$target
    return [System.Uri]::UnescapeDataString($baseUri.MakeRelativeUri($targetUri).ToString()).Replace("\", "/")
}

$requiredFiles = @(
    "index.html",
    "styles.css",
    "app.js",
    "README.html",
    "learn/index.html",
    "guides/index.html",
    "guides/features.html",
    "guides/getting-started.html",
    "guides/language-runtime.html",
    "guides/images-source.html",
    "guides/macros.html",
    "guides/web-and-data.html",
    "guides/host-capabilities.html",
    "guides/packages.html",
    "guides/hosted-registry-protocol.html",
    "guides/editor-debugging.html",
    "guides/development-release.html",
    "guides/store-packaging.html",
    "guides/updater-workflow.html"
)

$failures = New-Object System.Collections.Generic.List[string]

foreach ($file in $requiredFiles) {
    $path = Join-Path $Root $file
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        $failures.Add("Missing required docs file: $file")
    }
}

$noJekyllPath = Join-Path $docsRoot ".nojekyll"
if (-not $PackageMode -and -not (Test-Path -LiteralPath $noJekyllPath -PathType Leaf)) {
    $failures.Add("docs/.nojekyll is missing; GitHub Pages should serve the pre-rendered static HTML without Jekyll")
}

$repoRoot = Split-Path -Parent $docsRoot
$publicMarkdownFiles = @()
$gitListSucceeded = [bool]$PackageMode
if (-not $PackageMode) {
    try {
        $trackedMarkdown = @(& git -C $repoRoot ls-files -- "docs/*.md" "docs/**/*.md" 2>$null)
        if ($LASTEXITCODE -eq 0) {
            $gitListSucceeded = $true
            $publicMarkdownFiles = @(
                $trackedMarkdown |
                    Where-Object {
                        $_ -and $_ -ne "docs/feature-map.md" -and -not $_.StartsWith("docs/superpowers/")
                    } |
                    ForEach-Object {
                        Get-Item -LiteralPath (Join-Path $repoRoot $_)
                    }
            )
        }
    } catch {
        $publicMarkdownFiles = @()
    }
}

if (-not $gitListSucceeded) {
    $publicMarkdownFiles = @(
        Get-ChildItem -LiteralPath $docsRoot -Recurse -Filter "*.md" -File -ErrorAction SilentlyContinue |
            Where-Object {
                $relative = Get-DocsRelativePath -Path $_.FullName
                $relative -ne "feature-map.md" -and -not $relative.StartsWith("superpowers/")
            }
    )
}

foreach ($markdownFile in $publicMarkdownFiles) {
    $htmlPath = [System.IO.Path]::ChangeExtension($markdownFile.FullName, ".html")
    if (-not (Test-Path -LiteralPath $htmlPath -PathType Leaf)) {
        $relative = Get-DocsRelativePath -Path $markdownFile.FullName
        $failures.Add("Public Markdown docs file is missing an HTML sibling: $relative")
    }
}

$publicDocsMarkdownPattern = [regex]"docs/(?:wiki|learn|benchmarks|releases|feature-map|adding-words|debugger-integrations|reference/README|superpowers/[^<`"\s]+)\.md"
$htmlLinkPattern = [regex]'(?i)\b(?:href|src)="([^"]+)"'
$candidateProductionValidationPattern = [regex]'(?is)validate-update-channel\.ps1(?:(?!</code>).)*-Channel\s+candidate(?:(?!</code>).)*-RequireProduction'

foreach ($htmlFile in Get-ChildItem -LiteralPath $docsRoot -Recurse -Filter "*.html" -File -ErrorAction SilentlyContinue) {
    $relativeHtml = Get-DocsRelativePath -Path $htmlFile.FullName
    $htmlText = Get-Content -LiteralPath $htmlFile.FullName -Raw

    $markdownPathMatch = $publicDocsMarkdownPattern.Match($htmlText)
    if ($markdownPathMatch.Success) {
        $failures.Add("Public HTML references a docs Markdown path in ${relativeHtml}: $($markdownPathMatch.Value)")
    }

    if ($candidateProductionValidationPattern.IsMatch($htmlText)) {
        $failures.Add("Candidate update-channel command incorrectly requires production signatures in ${relativeHtml}")
    }

    foreach ($match in $htmlLinkPattern.Matches($htmlText)) {
        $ref = $match.Groups[1].Value
        if ($ref -match '(?i)^(?:[a-z][a-z0-9+.-]*:|#|mailto:|javascript:|data:)') {
            continue
        }

        $target = ($ref -split '#', 2)[0]
        $target = ($target -split '\?', 2)[0]
        if ([string]::IsNullOrWhiteSpace($target)) {
            continue
        }

        $target = [System.Uri]::UnescapeDataString($target)
        $resolved = [System.IO.Path]::GetFullPath((Join-Path $htmlFile.DirectoryName $target))
        if (-not $resolved.StartsWith($docsRootFull, [System.StringComparison]::OrdinalIgnoreCase)) {
            continue
        }
        if ((Test-Path -LiteralPath $resolved -PathType Container)) {
            $resolved = Join-Path $resolved "index.html"
        }
        if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
            $failures.Add("Broken local HTML link in ${relativeHtml}: $ref")
        }
    }
}

if ($failures.Count -eq 0) {
    $index = Get-Content -LiteralPath (Join-Path $Root "index.html") -Raw
    $styles = Get-Content -LiteralPath (Join-Path $Root "styles.css") -Raw
    $app = Get-Content -LiteralPath (Join-Path $Root "app.js") -Raw
    $readme = Get-Content -LiteralPath (Join-Path $Root "README.html") -Raw
    $learnIndex = Get-Content -LiteralPath (Join-Path $Root "learn/index.html") -Raw
    $publishedLearnIndexPath = Join-Path (Split-Path -Parent $Root) "learn/index.html"
    $guidesIndex = Get-Content -LiteralPath (Join-Path $Root "guides/index.html") -Raw
    $editorDebuggingGuide = Get-Content -LiteralPath (Join-Path $Root "guides/editor-debugging.html") -Raw
    $webAndDataGuide = Get-Content -LiteralPath (Join-Path $Root "guides/web-and-data.html") -Raw

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
        "id=""limits""",
        "../learn/",
        "guides/index.html"
    )

    foreach ($marker in $requiredIndexMarkers) {
        if (-not $index.Contains($marker)) {
            $failures.Add("index.html is missing marker: $marker")
        }
    }

    $requiredGuideMarkers = @(
        "Ricochet Guides",
        "Feature Overview",
        "Getting Started",
        "Language And Runtime",
        "Images And Source Emission",
        "Compile-Time Macros",
        "Web And Data",
        "Host Capabilities And Safety",
        "Packages And Registries",
        "Hosted Registry Protocol",
        "Editor And Debugging",
        "Development And Release",
        "Store Packaging",
        "Updater Workflow"
    )

    foreach ($marker in $requiredGuideMarkers) {
        if (-not $guidesIndex.Contains($marker)) {
            $failures.Add("guides/index.html is missing marker: $marker")
        }
    }

    $requiredLearnMarkers = @(
        "Learn Ricochet",
        "http-equiv=""refresh""",
        "../../learn/",
        "rel=""canonical"""
    )

    foreach ($marker in $requiredLearnMarkers) {
        if (-not $learnIndex.Contains($marker)) {
            $failures.Add("learn/index.html is missing marker: $marker")
        }
    }

    if (-not (Test-Path -LiteralPath $publishedLearnIndexPath -PathType Leaf)) {
        $failures.Add("docs/learn/index.html is missing; the retired reference Learn page must redirect to a published manual landing page")
    } else {
        $publishedLearnIndex = Get-Content -LiteralPath $publishedLearnIndexPath -Raw
        foreach ($marker in @("Learn Ricochet", "manual-map.html", "chapters/00-orientation.html", "chapters/38-capstone-packaged-gui-app.html")) {
            if (-not $publishedLearnIndex.Contains($marker)) {
                $failures.Add("docs/learn/index.html is missing marker: $marker")
            }
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
        '"not_equals?"',
        '"assert"',
        '"assert_true"',
        '"assert_false"',
        '"assert_equals"',
        '"assert_ok"',
        '"assert_error"',
        '"less_than?"',
        '"greater_than?"',
        '"less_or_equals?"',
        '"greater_or_equals?"',
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
        '"await_all"',
        '"release_task"',
        '"tasks"',
        '"task_status"',
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
        '"push"',
        '"put"',
        '"push"',
        '"put"',
        '"insert_at"',
        '"remove"',
        '"remove_at"',
        '"clear_items"',
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
        '"find_record"',
        '"default_page"',
        '"limit"',
        '"count_records"',
        '"first_record"',
        '"transaction"',
        '"begin"',
        '"commit"',
        '"rollback"',
        '"savepoint"',
        '"any?"',
        '"all?"',
        '"join"',
        '"trim"',
        '"trim_start"',
        '"trim_end"',
        '"blank?"',
        '"slice"',
        '"index_of"',
        '"last_index_of"',
        '"repeat"',
        '"lines"',
        '"chars"',
        '"split"',
        '"replace"',
        '"contains?"',
        '"starts_with?"',
        '"ends_with?"',
        '"uppercase"',
        '"lowercase"',
        '"concat"',
        '"to_number"',
        '"to_integer"',
        '"to_bigint"',
        '"to_int"',
        '"to_mediumint"',
        '"to_smallint"',
        '"to_tinyint"',
        '"to_bit"',
        '"to_unsigned_int"',
        '"to_unsigned_mediumint"',
        '"to_unsigned_smallint"',
        '"to_unsigned_tinyint"',
        '"to_unsigned_bigint"',
        '"to_float"',
        '"to_float32"',
        '"to_float64"',
        '"to_double"',
        '"to_real"',
        '"to_string"',
        '"json_encode"',
        '"json_decode"',
        '"regex"',
        '"matches?"',
        '"regex_find"',
        '"regex_replace"',
        '"captures"',
        '"Method"',
        '"ok?"',
        '"ok"',
        '"fail"',
        '"error?"',
        '"unwrap_or"',
        '"map_result"',
        '"and_then"',
        '"result_envelope"',
        '"nil?"',
        '"empty?"',
        '"print"',
        '"eprint"',
        '"read_line"',
        '"args"',
        '"env_get"',
        '"env"',
        '"env_set"',
        '"secret_env"',
        '"secret_literal"',
        '"secret_resolve"',
        '"password_hash"',
        '"password_verify"',
        '"upload_streams"',
        '"upload_stream"',
        '"upload_read"',
        '"upload_release"',
        '"config_get"',
        '"cwd"',
        '"runtime_capabilities"',
        '"info"',
        '"process_spawn"',
        '"process_spawn_task"',
        '"process_start"',
        '"process_jobs"',
        '"process_job"',
        '"process_cancel"',
        '"process_release"',
        '"process_write"',
        '"process_read"',
        '"process_env_put"',
        '"pty_start"',
        '"pty_write"',
        '"pty_read"',
        '"pty_resize"',
        '"pty_stop"',
        '"pty_release"',
        '"pty_list"',
        '"pty_detail"',
        '"approval_create"',
        '"approval_claim"',
        '"approval_complete"',
        '"approval_reject"',
        '"approval_detail"',
        '"approval_release"',
        '"now"',
        '"timestamp_now"',
        '"timestamp_parse"',
        '"timestamp_format"',
        '"timestamp_format_pattern"',
        '"timestamp_parts"',
        '"timestamp_from_parts"',
        '"timestamp_add"',
        '"timestamp_diff"',
        '"date_from_timestamp"',
        '"date_to_timestamp"',
        '"date_parse"',
        '"date_format"',
        '"date_add_days"',
        '"date_diff_days"',
        '"duration_millis"',
        '"duration_seconds"',
        '"duration_minutes"',
        '"duration_hours"',
        '"duration_days"',
        '"duration_weeks"',
        '"duration_parts"',
        '"sleep"',
        '"random"',
        '"exit"',
        '"fs_read_text"',
        '"fs_write_text"',
        '"fs_exists?"',
        '"fs_list"',
        '"fs_create_dir"',
        '"fs_delete"',
        '"workspace_resolve"',
        '"workspace_contains?"',
        '"workspace_metadata"',
        '"workspace_list"',
        '"workspace_read_text"',
        '"workspace_write_text"',
        '"workspace_mkdir"',
        '"workspace_delete"',
        '"workspace_copy"',
        '"workspace_move"',
        '"http_request_new"',
        '"http_header_put"',
        '"http_bearer_auth"',
        '"http_json_body"',
        '"http_timeout"',
        '"http_get"',
        '"http_get_task"',
        '"http_request"',
        '"http_post_json"',
        '"http_post_json_task"',
        '"http_request_task"',
        '"tcp_listen"',
        '"tcp_listeners"',
        '"tcp_listener"',
        '"tcp_accept"',
        '"tcp_listener_close"',
        '"tcp_listener_release"',
        '"tcp_connect"',
        '"tcp_connections"',
        '"tcp_connection"',
        '"tcp_write"',
        '"tcp_read"',
        '"tcp_close"',
        '"tcp_release"',
        '"ws_listen"',
        '"ws_listeners"',
        '"ws_listener"',
        '"ws_accept"',
        '"ws_listener_close"',
        '"ws_listener_release"',
        '"ws_connect"',
        '"ws_connections"',
        '"ws_connection"',
        '"ws_send"',
        '"ws_read"',
        '"ws_close"',
        '"ws_release"',
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
        '"web_command"',
        '"web_command_button"',
        '"webview_action"',
        '"webview_input"',
        '"webview_link"',
        '"webview_container"',
        '"web_toolbar"',
        '"web_sidebar"',
        '"web_tabs"',
        '"web_split_pane"',
        '"web_table"',
        '"web_form_row"',
        '"web_status_bar"',
        '"web_menu"',
        '"web_menu_bar"',
        '"webview_window"',
        '"webview_window_state"',
        '"webview_window_app"',
        '"webview_open_file"',
        '"webview_save_file"',
        '"webview_choose_folder"',
        '"webview_clipboard_read"',
        '"webview_clipboard_write"',
        '"webview_open_url"',
        '"webview_document"',
        '"inspect"',
        '"debug"',
        '"type"',
        '"class_of"',
        '"instance_of?"',
        '"responds_to?"',
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
        "42 User find_record",
        "10 User limit",
        "User count_records",
        "User first_record",
        "1 User exists?",
        "] `$db transaction",
        "`$db savepoint",
        "users array",
        "settings map",
        "tags Set",
        "push",
        "put",
        "slice",
        "regex value",
        "[web.static]",
        "mount = `"/assets`"",
        "User Model Subclass",
        "`$count 10 < while",
        "rco run --debug --step app.rco",
        "rco debug --json app.rco",
        "rco debug-tui --smoke app.rco",
        "rco debug-tui --command step --command continue app.rco",
        "break &lt;line&gt;",
        "clear &lt;line&gt;",
        "clear_breakpoints",
        "breakpoint_add",
        "breakpoint_remove",
        "breakpoint_clear",
        "grouped panes for source/current instruction",
        "globals, ``self``, tasks",
        "keyboard shortcuts for step, next, out, continue, and abort",
        "rco debug-web --smoke app.rco",
        "rco debug-web app.rco",
        "rco debug-adapter",
        "rco run --trace-file trace.json app.rco",
        "step``, ``next``, ``out``, ``continue``",
        "``self``, or ``tasks``",
        "rco fmt [--check] [path]",
        "rco lint [--json] [path]",
        "rco run [--debug] [--step] [--breakpoint LINE] [--trace-file PATH] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] [--allow-sockets] [--socket-allow-host HOST] &lt;path&gt; [args...]",
        "rco debug [--json] [--step] [--breakpoint LINE] [--trace-file PATH] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] [--allow-sockets] [--socket-allow-host HOST] &lt;path&gt; [args...]",
        "rco debug-tui [--smoke] [--command ACTION]... [--step] [--breakpoint LINE] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] [--allow-sockets] [--socket-allow-host HOST] &lt;path&gt; [args...]",
        "rco debug-web [--smoke] [--host IP] [--port PORT] [--step] [--breakpoint LINE] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] [--allow-sockets] [--socket-allow-host HOST] &lt;path&gt; [args...]",
        "rco debug-adapter",
        "rco run-bytecode [--debug] [--trace-file PATH] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] [--allow-sockets] [--socket-allow-host HOST] &lt;path&gt; [args...]",
        "rco gui [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] [--allow-sockets] [--socket-allow-host HOST] &lt;path&gt; [args...]",
        "rco tui [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] [--allow-sockets] [--socket-allow-host HOST] &lt;path&gt; [args...]",
        "rco package [path] --output &lt;exe&gt; [--tui] [--gui] [--mvc] [--gui-launcher PATH] [--linux-package tar|deb] [--package-name NAME] [--package-version VERSION] [--package-license SPDX]",
        "rco add &lt;source&gt; [--name NAME|--as ALIAS] [--registry PATH|--registry-url URL] [--version REQ] [--no-fetch]",
        "rco publish [path] --registry PATH [--provenance-file PATH] [--signature-file PATH] [--signature-kind KIND] [--dry-run]",
        "rco registry rebuild PATH",
        "rco registry check PATH",
        "rco search QUERY [--registry PATH|--registry-url URL]",
        "rco verify [path]",
        "rco audit [path] [--json]",
        "rco doctor [--capabilities] [path]",
        "rco words [--json] [--check] [--docs-app PATH] [--grammar PATH]",
        "rco bench [--iterations N] [--smoke] [--json]",
        "rco lsp-diagnostics [--pretty] &lt;path&gt;",
        "rco migrate new NAME [--dsl] [path]",
        "rco migrate status [path]",
        "rco migrate apply [path]",
        "rco migrate rollback [path] [--steps N]",
        "rco migrate dump [path] [--output PATH]",
        "rco seed [path]",
        "&quot;notes&quot; table_create",
        "&quot;id&quot; &quot;integer&quot; column primary_key",
        "&quot;status&quot; &quot;text&quot; column not_null &quot;writer's draft&quot; default",
        "&quot;notes&quot; &quot;archived_at&quot; &quot;text&quot; column_add",
        "&quot;notes&quot; &quot;archived_at&quot; &quot;archived_on&quot; column_rename",
        "&quot;idx_notes_status&quot; &quot;notes&quot; &quot;status&quot; index_create",
        "&quot;uq_notes_body&quot; &quot;notes&quot; &quot;body&quot; unique_index_create",
        "&quot;idx_notes_status&quot; &quot;notes&quot; index_drop",
        "&quot;uq_notes_body&quot; &quot;notes&quot; index_drop",
        "&quot;notes&quot; &quot;archived_on&quot; column_drop",
        "&quot;notes&quot; table_drop",
        "`"lib/math`" import",
        "`"forms/validation`" import",
        "[dependencies.forms]",
        "rco add registry:@ricochet/forms --registry ../ricochet-registry --as forms --version `"^0.1.0`"",
        "rco add registry:@ricochet/forms --registry-url file:///E:/path/to/ricochet-registry/index.toml --as forms --version `"^0.1.0`"",
        "rco registry rebuild ../ricochet-registry",
        "rco registry check ../ricochet-registry",
        "rco search forms --registry-url file:///E:/path/to/ricochet-registry/index.toml",
        "--provenance-file provenance.json --signature-file forms.sig --signature-kind minisign",
        "rco audit --json",
        "rco add github:BARKx4/ricochet_auth@v0.1.0 --no-fetch",
        "`"/dashboard`" redirect",
        "rco routes [path]",
        "rco serve [--host HOST] [--port PORT] [--debug] [--watch] [--max-controller-instructions N] [--allow-env] [--no-env] [--env-allow NAME] [--allow-process] [--process-root PATH] [--allow-pty] [--fs-root PATH] [--fs-readonly] [--http-allow-host HOST]",
        "rco test [--debug] [--filter PATTERN] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--allow-process] [--process-root PATH] [--allow-pty] [--no-tui] [--allow-tui] [--no-webview] [--allow-webview] [--no-env] [--env-allow NAME] [--no-sleep] [--http-allow-host HOST] [--allow-sockets] [--socket-allow-host HOST] [path]",
        "`$task task_status",
        "`$task info",
        "`$task release_task",
        "tui_read_key value",
        "`$body `$state `$actions webview_window_state value",
        "`"Increment`" `"increment`" webview_button",
        "`"increment`" `"Increment`" `"Ctrl+I`" web_command",
        "`"Increment`" `"increment`" web_command_button",
        "`"Actions`" commands get web_menu",
        "menus get web_menu_bar",
        "webview_window_app value",
        "`"Increment`" `"increment`" `"increment_counter`" webview_action",
        "webview_window_state",
        "tasks count",
        "fs_read_text",
        "fs_delete",
        "workspace_read_text",
        "workspace_list",
        "workspace_delete",
        "http_get",
        "http_get_task",
        "tcp_listen",
        "tcp_accept",
        "tcp_listener_release",
        "tcp_connect",
        "tcp_read",
        "tcp_release",
        "ws_listen",
        "ws_accept",
        "ws_listener_release",
        "ws_connect",
        "ws_read",
        "ws_release",
        "process_spawn",
        "process_start",
        "process_write",
        "process_read",
        "process_release",
        "pty_start",
        "pty_read",
        "pty_release",
        "timestamp_parse",
        "timestamp_format",
        "date_parse",
        "date_format",
        "duration_hours",
        "approval_create",
        "approval_claim",
        "approval_release",
        "result_envelope",
        "runtime_capabilities",
        "tui_write",
        "{ `$user name.get }",
        "{&#37; show get if &#37;}",
        "{&#37; users get &quot;user&quot; each &#37;}",
        "{&#37; &quot;Featured users&quot; &quot;heading&quot; var do &#37;}"
    )

    foreach ($example in $requiredExamples) {
        if (-not $index.Contains($example) -and -not $app.Contains($example) -and -not $webAndDataGuide.Contains($example) -and -not $editorDebuggingGuide.Contains($example)) {
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
        $failures.Add("README.html does not explain how to open the static site")
    }
}

if ($failures.Count -gt 0) {
    foreach ($failure in $failures) {
        Write-Error $failure
    }
    exit 1
}

Write-Host "Ricochet reference docs validation passed."
