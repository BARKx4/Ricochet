[CmdletBinding()]
param(
    [string]$Rco,
    [string]$OutputRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if ([string]::IsNullOrWhiteSpace($Rco)) {
    $Rco = Join-Path $RepoRoot "target\debug\rco.exe"
}
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $RepoRoot "examples\words"
}

if (-not (Test-Path -LiteralPath $Rco -PathType Leaf)) {
    throw "Could not find rco at '$Rco'. Build it first with: cargo build -p ricochet_cli --bin rco"
}

function New-OrdinalSet {
    param([string[]]$Values)

    $set = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($value in $Values) {
        [void]$set.Add($value)
    }
    return ,$set
}

function Get-WordSnippet {
    param([object]$Entry)

    $match = [regex]::Match(
        [string]$Entry.documentation,
        '(?s)```ricochet\r?\n(.*?)\r?\n```'
    )
    if (-not $match.Success) {
        throw "Word '$($Entry.word)' has no Ricochet example fence"
    }
    return $match.Groups[1].Value.TrimEnd()
}

function Get-WordSlug {
    param(
        [string]$Word,
        [int]$Index
    )

    $symbolSlugs = @{
        "+" = "plus"
        "-" = "minus"
        "=" = "equals-symbol"
        "!=" = "not-equals-symbol"
        "<" = "less-than-symbol"
        ">" = "greater-than-symbol"
        "<=" = "less-or-equals-symbol"
        ">=" = "greater-or-equals-symbol"
        "*" = "multiply-symbol"
        "/" = "divide-symbol"
        "%" = "modulo-symbol"
        "field.get / field.set" = "field-get-set"
    }
    if ($symbolSlugs.ContainsKey($Word)) {
        return [string]$symbolSlugs[$Word]
    }

    $slug = $Word.ToLowerInvariant()
    $slug = $slug.Replace("?", "-predicate")
    $slug = [regex]::Replace($slug, '[^a-z0-9]+', '-')
    $slug = $slug.Trim('-')
    if ([string]::IsNullOrWhiteSpace($slug)) {
        return "word-$Index"
    }
    return $slug
}

$wordsJson = (& $Rco words --json | Out-String)
if ($LASTEXITCODE -ne 0) {
    throw "rco words --json failed with exit code $LASTEXITCODE"
}
$liveWords = $wordsJson | ConvertFrom-Json
if ($null -eq $liveWords -or $liveWords.Count -eq 0) {
    throw "rco words --json returned an empty inventory"
}

$checkMvc = New-OrdinalSet -Values @(
    "route", "all", "find_record", "default_page", "where", "limit",
    "count_records", "first_record", "exists?", "insert", "update",
    "transaction", "begin", "commit", "rollback", "savepoint",
    "GET", "POST", "PUT", "PATCH", "DELETE"
)
$checkFilesystemWrite = New-OrdinalSet -Values @(
    "fs_write_text", "fs_create_dir", "fs_delete", "workspace_write_text",
    "workspace_mkdir", "workspace_delete", "workspace_copy", "workspace_move"
)
$checkHttp = New-OrdinalSet -Values @(
    "http_get", "http_get_task", "http_request", "http_post_json",
    "http_post_json_task", "http_request_task", "http_stream_start",
    "http_streams", "http_stream", "http_stream_read", "http_stream_cancel",
    "http_stream_release"
)
$checkUpload = New-OrdinalSet -Values @("upload_stream", "upload_read", "upload_release")
$checkSocket = New-OrdinalSet -Values @(
    "tcp_listen", "tcp_listeners", "tcp_listener", "tcp_accept",
    "tcp_listener_close", "tcp_listener_release", "tcp_connect",
    "tcp_connections", "tcp_connection", "tcp_write", "tcp_read",
    "tcp_close", "tcp_release", "ws_listen", "ws_listeners", "ws_listener",
    "ws_accept", "ws_listener_close", "ws_listener_release", "ws_connect",
    "ws_connections", "ws_connection", "ws_send", "ws_read", "ws_close",
    "ws_release"
)
$checkTui = New-OrdinalSet -Values @(
    "tui_enter", "tui_leave", "tui_clear", "tui_move_to", "tui_write",
    "tui_flush", "tui_size", "tui_poll_key", "tui_read_key"
)
$checkWebviewInteractive = New-OrdinalSet -Values @(
    "webview_open_file", "webview_save_file", "webview_choose_folder",
    "webview_clipboard_read", "webview_clipboard_write", "webview_open_url"
)
$checkProcess = New-OrdinalSet -Values @(
    "process_spawn", "process_spawn_task", "process_start", "process_jobs",
    "process_job", "process_cancel", "process_release", "process_write",
    "process_read", "pty_start", "pty_write", "pty_read", "pty_resize",
    "pty_stop", "pty_release", "pty_list", "pty_detail"
)

$runEnvironment = New-OrdinalSet -Values @("env_get", "env_set", "secret_resolve", "cwd", "env")
$runFilesystemReadonly = New-OrdinalSet -Values @(
    "fs_read_text", "fs_exists?", "fs_list", "workspace_resolve",
    "workspace_contains?", "workspace_metadata", "workspace_list",
    "workspace_read_text", "workspace_read_text_snapshot"
)
$runWebview = New-OrdinalSet -Values @(
    "webview_text", "webview_heading", "webview_button", "web_command",
    "web_command_button", "webview_action", "webview_input", "webview_link",
    "webview_container", "web_toolbar", "web_sidebar", "web_tabs",
    "web_split_pane", "web_table", "web_form_row", "web_status_bar",
    "web_menu", "web_menu_bar", "webview_window", "webview_window_state",
    "webview_window_app", "webview_document"
)

$collectionFixtureWords = New-OrdinalSet -Values @(
    "push", "put", "insert_at", "remove", "remove_at", "clear_items", "at",
    "count", "first", "last", "take", "skip", "reverse", "has?", "keys",
    "values", "each", "transform", "select", "reduce", "find", "any?",
    "all?", "join", "json_encode"
)
$oopFixtureWords = New-OrdinalSet -Values @(
    "new", "send", "field.get / field.set", "class_of", "instance_of?",
    "responds_to?", "fields", "methods"
)
$regexFixtureWords = New-OrdinalSet -Values @("matches?", "regex_find", "captures")
$taskFixtureWords = New-OrdinalSet -Values @(
    "await", "await_all", "release_task", "task_status", "id", "info",
    "pending?", "running?", "completed?", "failed?"
)
$timeFixtureWords = New-OrdinalSet -Values @(
    "timestamp_format", "timestamp_format_pattern", "timestamp_parts",
    "timestamp_from_parts", "timestamp_add", "timestamp_diff",
    "date_from_timestamp", "date_to_timestamp", "date_format", "date_add_days",
    "date_diff_days", "duration_parts"
)
$httpFixtureWords = New-OrdinalSet -Values @(
    "http_header_put", "http_bearer_auth", "http_json_body", "http_timeout"
)
$webviewFixtureWords = New-OrdinalSet -Values @(
    "webview_container", "web_toolbar", "web_sidebar", "web_tabs",
    "web_split_pane", "web_table", "web_form_row", "web_menu", "web_menu_bar",
    "webview_window", "webview_window_state", "webview_window_app"
)

$sourceOverrides = [System.Collections.Generic.Dictionary[string,string]]::new(
    [System.StringComparer]::Ordinal
)
$sourceOverrides.Add("assert", '"Ada" empty? not assert')
$sourceOverrides.Add("assert_true", 'true assert_true')
$sourceOverrides.Add("assert_false", 'false assert_false')
$sourceOverrides.Add("assert_ok", '42 ok assert_ok')
$sourceOverrides.Add("swap", @'
ctx map
$ctx "home/index" swap view
'@.Trim())
$sourceOverrides.Add("dup", '1 dup + 2 assert_equals')
$sourceOverrides.Add("drop", '1 2 drop 1 assert_equals')
$sourceOverrides.Add("get", @'
"Ada" name var
"name" get
"Ada" assert_equals
'@.Trim())
$sourceOverrides.Add("set", @'
"Ada" name var
"Grace" name set
$name "Grace" assert_equals
'@.Trim())
$sourceOverrides.Add("nil?", 'nil nil? true assert_equals')
$sourceOverrides.Add("self", @'
Identity Object Subclass
  [ self ] "identity" Method
end
Identity new object var
$object identity $object assert_equals
'@.Trim())
$sourceOverrides.Add("Method", @'
Greeter Object Subclass
  [ "hello" ] "greet" Method
end
Greeter new greet "hello" assert_equals
'@.Trim())
$sourceOverrides.Add("send", '$user "displayName" send "ada@example.com" assert_equals')
$sourceOverrides.Add("field.get / field.set", @'
$user email.get "ada@example.com" assert_equals
"grace@example.com" $user email.set user set
$user email.get "grace@example.com" assert_equals
'@.Trim())
$sourceOverrides.Add("return", @'
answer function
  42 return
  0
end
answer 42 assert_equals
'@.Trim())
$sourceOverrides.Add("while", @'
0 count var
$count 3 < while
  $count 1 + count set
end
$count 3 assert_equals
'@.Trim())
$sourceOverrides.Add("view", @'
ctx map
$ctx "home/index" swap view
'@.Trim())
$sourceOverrides.Add("ok?", '42 ok ok? true assert_equals')
$sourceOverrides.Add("value", '42 ok value 42 assert_equals')
$sourceOverrides.Add("error", '"Validation" "bad" fail error "kind" at "Validation" assert_equals')
$sourceOverrides.Add("error?", '"Validation" "bad" fail error? true assert_equals')
$sourceOverrides.Add("print", @'
"Ada" name var
"Name: " print
$name print
'@.Trim())
$sourceOverrides.Add("read_line", 'read_line trim "Ada" assert_equals')
$sourceOverrides.Add("env_get", '"RICOCHET_WORD_EXAMPLE" env_get value "present" assert_equals')
$sourceOverrides.Add("env_set", @'
"RICOCHET_WORD_EXAMPLE" "updated" env_set value drop
"RICOCHET_WORD_EXAMPLE" env_get value "updated" assert_equals
'@.Trim())
$sourceOverrides.Add("secret_resolve", '"RICOCHET_WORD_EXAMPLE" secret_env secret_resolve value drop')
$sourceOverrides.Add("password_verify", @'
"Long unique passphrase 2026" password_hash value storedHash var
"Long unique passphrase 2026" $storedHash password_verify value true assert_equals
'@.Trim())
$sourceOverrides.Add("config_get", @'
config map
provider map
$provider "token" "configured" put drop
$config "provider" $provider put drop
path array
$path "provider" push drop
$path "token" push drop
$config $path config_get value "configured" assert_equals
'@.Trim())
$sourceOverrides.Add("approval_claim", @'
operation map
options map
$operation $options approval_create value approval var
$approval "id" at $approval "token" at approval_claim value "claimed" at true assert_equals
'@.Trim())
$sourceOverrides.Add("approval_complete", @'
operation map
options map
$operation $options approval_create value approval var
$approval "id" at $approval "token" at approval_claim value drop
result map
$result "ok" true put drop
$approval "id" at $result approval_complete value "completed" at true assert_equals
'@.Trim())
$sourceOverrides.Add("approval_reject", @'
operation map
options map
$operation $options approval_create value approval var
$approval "id" at "Rejected by example" approval_reject value "rejected" at true assert_equals
'@.Trim())
$sourceOverrides.Add("approval_detail", @'
operation map
options map
$operation $options approval_create value approval var
$approval "id" at approval_detail value "id" at $approval "id" at assert_equals
'@.Trim())
$sourceOverrides.Add("approval_release", @'
operation map
options map
$operation $options approval_create value approval var
$approval "id" at approval_release value true assert_equals
'@.Trim())
$sourceOverrides.Add("sleep", '1 sleep')
$sourceOverrides.Add("inspect", @'
settings map
$settings "theme" "dark" put drop
$settings inspect println
'@.Trim())
$sourceOverrides.Add("debug", @'
payload map
$payload "ok" true put drop
$payload debug
'@.Trim())
$sourceOverrides.Add("class_of", '$user class_of User assert_equals')
$sourceOverrides.Add("instance_of?", '$user User instance_of? true assert_equals')
$sourceOverrides.Add("responds_to?", '"displayName" $user responds_to? true assert_equals')
$sourceOverrides.Add("env", '"RICOCHET_WORD_EXAMPLE" env value "present" assert_equals')

function Get-ValidationMode {
    param([string]$Word)

    if ($checkMvc.Contains($Word)) { return "check-mvc" }
    if ($checkFilesystemWrite.Contains($Word)) { return "check-filesystem-write" }
    if ($checkHttp.Contains($Word)) { return "check-http-loopback" }
    if ($checkUpload.Contains($Word)) { return "check-upload-context" }
    if ($checkSocket.Contains($Word)) { return "check-socket-loopback" }
    if ($checkTui.Contains($Word)) { return "check-tui" }
    if ($checkWebviewInteractive.Contains($Word)) { return "check-webview-interactive" }
    if ($checkProcess.Contains($Word)) { return "check-process" }
    if ($runEnvironment.Contains($Word)) { return "run-environment" }
    if ($runFilesystemReadonly.Contains($Word)) { return "run-filesystem-readonly" }
    if ($runWebview.Contains($Word)) { return "run-webview" }
    if ($Word -eq "sleep") { return "run-sleep" }
    if ($Word -eq "read_line") { return "run-stdin" }
    return "run-sandboxed"
}

function Get-ValidationReason {
    param([string]$Mode)

    switch ($Mode) {
        "check-mvc" { return "Requires the MVC route or database runtime." }
        "check-filesystem-write" { return "Mutates or deletes files; compile-checked by default." }
        "check-http-loopback" { return "Requires a bounded loopback HTTP peer." }
        "check-upload-context" { return "Requires an MVC upload stream supplied by a request." }
        "check-socket-loopback" { return "Requires a coordinated loopback socket peer." }
        "check-tui" { return "Requires an interactive terminal." }
        "check-webview-interactive" { return "Opens a native dialog, clipboard, or external URL surface." }
        "check-process" { return "Starts or controls an operating-system process or PTY." }
        default { return $null }
    }
}

function Get-ValidationEvidence {
    param([string]$Mode)

    switch ($Mode) {
        "check-mvc" { return @("examples/learn/23-mvc/first_app", "examples/learn/23-mvc/controllers", "examples/learn/26-data/contacts_app", "crates/ricochet_web/src/router.rs", "crates/ricochet_web/src/database_capability.rs", "crates/ricochet_web/tests/web_mvc.rs") }
        "check-filesystem-write" { return @("crates/ricochet_cli/tests/cli_smoke.rs", "crates/ricochet_vm/src/builtins.rs") }
        "check-http-loopback" { return @("examples/learn/18-http-streams/api-client.rco") }
        "check-upload-context" { return @("examples/learn/23-mvc/templates_uploads") }
        "check-socket-loopback" { return @("examples/learn/19-sockets/tcp_echo.rco", "examples/learn/19-sockets/ws_echo.rco") }
        "check-tui" { return @("examples/learn/21-tui/task-dashboard.rco", "examples/tui_counter.rco") }
        "check-webview-interactive" { return @("crates/ricochet_vm/src/vm.rs", "crates/ricochet_vm/src/builtins.rs") }
        "check-process" { return @("examples/learn/20-processes-and-ptys/tool-runner.rco", "crates/ricochet_cli/tests/cli_smoke.rs") }
        default { return @() }
    }
}

$appsRoot = Join-Path $OutputRoot "apps"
New-Item -ItemType Directory -Path $appsRoot -Force | Out-Null

$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$manifestEntries = [System.Collections.Generic.List[object]]::new()
$expectedFiles = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
)

for ($index = 0; $index -lt $liveWords.Count; $index++) {
    $entry = $liveWords[$index]
    $word = [string]$entry.word
    $group = [string]$entry.detail
    $number = $index + 1
    $slug = Get-WordSlug -Word $word -Index $number
    $fileName = "{0:D4}-{1}.rco" -f $number, $slug
    [void]$expectedFiles.Add($fileName)

    if ($sourceOverrides.ContainsKey($word)) {
        $source = $sourceOverrides[$word]
    } else {
        $source = Get-WordSnippet -Entry $entry
    }

    $fixture = $null
    if ($collectionFixtureWords.Contains($word)) { $fixture = "_collections" }
    if ($oopFixtureWords.Contains($word)) { $fixture = "_oop" }
    if ($regexFixtureWords.Contains($word)) { $fixture = "_regex" }
    if ($taskFixtureWords.Contains($word)) { $fixture = "_tasks" }
    if ($timeFixtureWords.Contains($word)) { $fixture = "_time" }
    if ($httpFixtureWords.Contains($word)) { $fixture = "_http" }
    if ($webviewFixtureWords.Contains($word)) { $fixture = "_webview" }
    if ($null -ne $fixture) {
        $source = "`"$fixture`" import`n`n$source"
    }

    $mode = Get-ValidationMode -Word $word
    $header = "(( Generated by scripts/generate-word-examples.ps1. ))`n(( Word: $word | Group: $group | Validation: $mode ))"
    $contents = "$header`n`n$($source.Trim())`n"
    [System.IO.File]::WriteAllText((Join-Path $appsRoot $fileName), $contents, $Utf8NoBom)

    $tokens = if ($word -eq "field.get / field.set") {
        @("email.get", "email.set")
    } else {
        @($word)
    }
    $manifestEntry = [ordered]@{
        word = $word
        group = $group
        path = "apps/$fileName"
        validation = $mode
        tokens = $tokens
    }
    $reason = Get-ValidationReason -Mode $mode
    if ($null -ne $reason) {
        $manifestEntry.reason = $reason
        $manifestEntry.evidence = @(Get-ValidationEvidence -Mode $mode)
    }
    $manifestEntries.Add([pscustomobject]$manifestEntry)
}

$staleFiles = @(
    Get-ChildItem -LiteralPath $appsRoot -Filter "*.rco" -File |
        Where-Object {
            $_.Name -match '^\d{4}-' -and -not $expectedFiles.Contains($_.Name)
        }
)
if ($staleFiles.Count -gt 0) {
    $names = ($staleFiles.Name -join ", ")
    throw "Stale generated word examples require explicit removal: $names"
}

$manifest = [ordered]@{
    schema = "ricochet.word-examples.v1"
    inventory = "rco words --json"
    count = $manifestEntries.Count
    examples = $manifestEntries
}
$manifestJson = $manifest | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText(
    (Join-Path $OutputRoot "manifest.json"),
    $manifestJson + "`n",
    $Utf8NoBom
)

Write-Host "Generated $($manifestEntries.Count) word examples in $appsRoot"
