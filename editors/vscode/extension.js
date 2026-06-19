"use strict";

const childProcess = require("child_process");
const fs = require("fs/promises");
const os = require("os");
const path = require("path");
const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

let client;
let outputChannel;
let debugStackPanel;
const pausedDebugSessions = new Set();

function ricochetCommand() {
  const config = vscode.workspace.getConfiguration("ricochet");
  if (!vscode.workspace.isTrusted) {
    const inspected = config.inspect("server.path");
    if (inspected?.workspaceFolderValue !== undefined || inspected?.workspaceValue !== undefined) {
      return inspected.globalValue ?? inspected.defaultValue ?? "rco";
    }
  }
  return config.get("server.path", "rco");
}

function workspaceFolderForPath(documentPath) {
  return vscode.workspace.getWorkspaceFolder(vscode.Uri.file(documentPath));
}

function serverOptions() {
  const config = vscode.workspace.getConfiguration("ricochet");
  const command = ricochetCommand();
  const trace = config.get("server.trace", false);
  const args = ["lsp"];
  if (trace) {
    args.push("--trace");
  }
  const workspaceFolder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  return {
    command,
    args,
    transport: TransportKind.stdio,
    options: workspaceFolder ? { cwd: workspaceFolder } : undefined,
  };
}

function clientOptions() {
  return {
    documentSelector: [{ scheme: "file", language: "ricochet" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.rco"),
    },
  };
}

function createClient() {
  return new LanguageClient(
    "ricochetLanguageServer",
    "Ricochet Language Server",
    serverOptions(),
    clientOptions(),
  );
}

async function restartLanguageServer() {
  if (client) {
    await client.stop();
    client.dispose();
  }
  client = createClient();
  await client.start();
}

async function runWithStackVisualizer(context) {
  try {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== "ricochet") {
      vscode.window.showWarningMessage("Open a Ricochet .rco file before running the stack visualizer.");
      return;
    }

    if (editor.document.isDirty) {
      const saved = await editor.document.save();
      if (!saved) {
        vscode.window.showWarningMessage("Save the Ricochet file before running the stack visualizer.");
        return;
      }
    }

    const tracePath = path.join(
      os.tmpdir(),
      `ricochet-stack-${Date.now()}-${Math.random().toString(16).slice(2)}.json`,
    );
    const documentPath = editor.document.uri.fsPath;
    await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: "Running Ricochet stack visualizer",
        cancellable: false,
      },
      async () => {
        await runRcoTrace(documentPath, tracePath);
      },
    );

    const traceSource = await fs.readFile(tracePath, "utf8");
    const events = JSON.parse(traceSource);
    const panel = vscode.window.createWebviewPanel(
      "ricochetStackVisualizer",
      "Ricochet Stack",
      vscode.ViewColumn.Beside,
      { enableScripts: true },
    );
    panel.webview.html = stackVisualizerHtml(panel.webview, documentPath, tracePath, events);
    context.subscriptions.push(panel);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    outputChannel.show(true);
    vscode.window.showErrorMessage(`Ricochet stack visualizer failed: ${message}`);
  }
}

function runRcoTrace(documentPath, tracePath) {
  return new Promise((resolve, reject) => {
    const command = ricochetCommand();
    const workspaceFolder = workspaceFolderForPath(documentPath);
    const cwd = workspaceFolder?.uri.fsPath ?? path.dirname(documentPath);
    const child = childProcess.execFile(
      command,
      ["run", "--trace-file", tracePath, documentPath],
      { cwd, windowsHide: true },
      (error, stdout, stderr) => {
        outputChannel.appendLine(`> ${command} run --trace-file ${tracePath} ${documentPath}`);
        if (stdout) {
          outputChannel.append(stdout);
        }
        if (stderr) {
          outputChannel.append(stderr);
        }
        if (error) {
          reject(new Error(stderr || error.message));
          return;
        }
        resolve();
      },
    );
    child.on("error", reject);
  });
}

function stackVisualizerHtml(webview, documentPath, tracePath, events) {
  const nonce = Math.random().toString(36).slice(2);
  const csp = [
    "default-src 'none'",
    `style-src ${webview.cspSource} 'unsafe-inline'`,
    `script-src 'nonce-${nonce}'`,
  ].join("; ");
  const payload = JSON.stringify({
    documentPath,
    tracePath,
    events,
  }).replace(/</g, "\\u003c");

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="${csp}">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Ricochet Stack</title>
  <style>
    :root {
      color-scheme: dark light;
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
    }
    body {
      margin: 0;
      color: var(--vscode-foreground);
      background: var(--vscode-editor-background);
    }
    header {
      padding: 14px 16px 10px;
      border-bottom: 1px solid var(--vscode-panel-border);
    }
    h1 {
      margin: 0 0 6px;
      font-size: 16px;
      font-weight: 600;
    }
    .meta {
      color: var(--vscode-descriptionForeground);
      font-size: 12px;
      overflow-wrap: anywhere;
    }
    main {
      display: grid;
      grid-template-columns: minmax(220px, 34%) minmax(280px, 1fr);
      min-height: calc(100vh - 70px);
    }
    .timeline {
      border-right: 1px solid var(--vscode-panel-border);
      overflow: auto;
      max-height: calc(100vh - 70px);
    }
    .event {
      width: 100%;
      display: grid;
      grid-template-columns: 72px 1fr;
      gap: 8px;
      padding: 10px 12px;
      border: 0;
      border-bottom: 1px solid var(--vscode-panel-border);
      color: inherit;
      background: transparent;
      text-align: left;
      cursor: pointer;
    }
    .event:hover,
    .event.active {
      background: var(--vscode-list-hoverBackground);
    }
    .event.active {
      outline: 1px solid var(--vscode-focusBorder);
      outline-offset: -1px;
    }
    .event-kind {
      color: var(--vscode-symbolIcon-functionForeground);
      font-size: 11px;
      text-transform: uppercase;
    }
    .event-source,
    .event-opcode {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .detail {
      padding: 16px;
      overflow: auto;
      max-height: calc(100vh - 70px);
    }
    .section {
      margin: 0 0 18px;
    }
    .section h2 {
      margin: 0 0 8px;
      font-size: 13px;
      font-weight: 600;
      color: var(--vscode-descriptionForeground);
      text-transform: uppercase;
    }
    .stack {
      display: grid;
      gap: 6px;
    }
    .value,
    pre {
      margin: 0;
      padding: 8px 10px;
      border: 1px solid var(--vscode-panel-border);
      border-radius: 4px;
      background: var(--vscode-editor-inactiveSelectionBackground);
      font-family: var(--vscode-editor-font-family);
      overflow-wrap: anywhere;
      white-space: pre-wrap;
    }
    .binding {
      display: grid;
      grid-template-columns: minmax(80px, 25%) 1fr;
      gap: 8px;
      align-items: start;
      margin-bottom: 6px;
    }
    .binding-name {
      color: var(--vscode-symbolIcon-variableForeground);
      overflow-wrap: anywhere;
    }
    @media (max-width: 720px) {
      main {
        grid-template-columns: 1fr;
      }
      .timeline {
        max-height: 38vh;
        border-right: 0;
        border-bottom: 1px solid var(--vscode-panel-border);
      }
    }
  </style>
</head>
<body>
  <header>
    <h1>Ricochet Stack</h1>
    <div class="meta" id="meta"></div>
  </header>
  <main>
    <nav class="timeline" id="timeline"></nav>
    <section class="detail" id="detail"></section>
  </main>
  <script id="trace-data" type="application/json">${payload}</script>
  <script nonce="${nonce}">
    const data = JSON.parse(document.getElementById("trace-data").textContent);
    const timeline = document.getElementById("timeline");
    const detail = document.getElementById("detail");
    document.getElementById("meta").textContent = data.documentPath + " | " + data.events.length + " events";

    function text(value) {
      return String(value ?? "");
    }

    function stackFor(event) {
      return event.stack_after || event.stack || [];
    }

    function renderTimeline() {
      timeline.textContent = "";
      data.events.forEach((event, index) => {
        const button = document.createElement("button");
        button.className = "event";
        button.type = "button";
        button.innerHTML = "<div class='event-kind'></div><div><div class='event-source'></div><div class='event-opcode'></div></div>";
        button.querySelector(".event-kind").textContent = event.event;
        button.querySelector(".event-source").textContent = event.source || event.frame || "";
        button.querySelector(".event-opcode").textContent = event.opcode || event.message || "";
        button.addEventListener("click", () => renderDetail(index));
        timeline.appendChild(button);
      });
    }

    function renderBindings(title, bindings) {
      const section = document.createElement("section");
      section.className = "section";
      const heading = document.createElement("h2");
      heading.textContent = title;
      section.appendChild(heading);
      if (!bindings || bindings.length === 0) {
        const empty = document.createElement("div");
        empty.className = "value";
        empty.textContent = "<empty>";
        section.appendChild(empty);
        return section;
      }
      bindings.forEach((binding) => {
        const row = document.createElement("div");
        row.className = "binding";
        const name = document.createElement("div");
        name.className = "binding-name";
        name.textContent = binding.name;
        const value = document.createElement("div");
        value.className = "value";
        value.textContent = binding.value?.debug ?? "";
        row.append(name, value);
        section.appendChild(row);
      });
      return section;
    }

    function renderStack(event) {
      const section = document.createElement("section");
      section.className = "section";
      const heading = document.createElement("h2");
      heading.textContent = "Stack";
      section.appendChild(heading);
      const stack = stackFor(event);
      const stackNode = document.createElement("div");
      stackNode.className = "stack";
      if (stack.length === 0) {
        const empty = document.createElement("div");
        empty.className = "value";
        empty.textContent = "<empty>";
        stackNode.appendChild(empty);
      } else {
        stack.forEach((value, index) => {
          const node = document.createElement("div");
          node.className = "value";
          node.textContent = "[" + index + "] " + text(value.debug);
          stackNode.appendChild(node);
        });
      }
      section.appendChild(stackNode);
      return section;
    }

    function renderDetail(index) {
      [...timeline.children].forEach((node, current) => node.classList.toggle("active", current === index));
      const event = data.events[index];
      detail.textContent = "";
      const summary = document.createElement("section");
      summary.className = "section";
      const heading = document.createElement("h2");
      heading.textContent = event.event + " event";
      const pre = document.createElement("pre");
      pre.textContent = [
        "source: " + text(event.source),
        "frame: " + text(event.frame),
        "opcode: " + text(event.opcode),
        "reason: " + text(event.reason),
        "message: " + text(event.message),
      ].filter((line) => !line.endsWith(": ")).join("\\n");
      summary.append(heading, pre);
      detail.appendChild(summary);
      detail.appendChild(renderStack(event));
      detail.appendChild(renderBindings("Locals", event.locals));
      detail.appendChild(renderBindings("Globals", event.globals));
      if (event.self) {
        const section = document.createElement("section");
        section.className = "section";
        const selfHeading = document.createElement("h2");
        selfHeading.textContent = "Self";
        const value = document.createElement("div");
        value.className = "value";
        value.textContent = event.self.debug;
        section.append(selfHeading, value);
        detail.appendChild(section);
      }
    }

    renderTimeline();
    renderDetail(Math.max(0, data.events.length - 1));
  </script>
</body>
</html>`;
}

function debugAdapterDescriptorFactory() {
  return {
    createDebugAdapterDescriptor(session) {
      const command = ricochetCommand();
      const cwd = session.workspaceFolder?.uri.fsPath;
      return new vscode.DebugAdapterExecutable(
        command,
        ["debug-adapter"],
        cwd ? { cwd } : undefined,
      );
    },
  };
}

function debugAdapterTrackerFactory(context) {
  return {
    createDebugAdapterTracker(session) {
      return {
        onDidSendMessage(message) {
          if (message?.type === "event" && message.event === "stopped") {
            pausedDebugSessions.add(session.id);
            updateDebuggerStackPanel(context, session).catch((error) => {
              const messageText = error instanceof Error ? error.message : String(error);
              outputChannel.appendLine(`Ricochet debugger stack update failed: ${messageText}`);
            });
          } else if (
            message?.type === "event" &&
            ["continued", "terminated", "exited"].includes(message.event)
          ) {
            pausedDebugSessions.delete(session.id);
          }
        },
      };
    },
  };
}

async function showDebuggerStack(context) {
  const session = vscode.debug.activeDebugSession;
  if (!session || session.type !== "ricochet" || !pausedDebugSessions.has(session.id)) {
    const panel = ensureDebugStackPanel(context);
    panel.reveal(vscode.ViewColumn.Beside);
    panel.webview.postMessage({
      type: "snapshot",
      snapshot: {
        status: "Pause a Ricochet debug session to see the live stack.",
        frame: null,
        scopes: [],
      },
    });
    return;
  }

  await updateDebuggerStackPanel(context, session, true);
}

async function updateDebuggerStackPanel(context, session, reveal = false) {
  const panel = ensureDebugStackPanel(context);
  if (reveal) {
    panel.reveal(vscode.ViewColumn.Beside);
  }

  const snapshot = await readDebuggerSnapshot(session);
  await panel.webview.postMessage({ type: "snapshot", snapshot });
}

function ensureDebugStackPanel(context) {
  if (debugStackPanel) {
    return debugStackPanel;
  }

  debugStackPanel = vscode.window.createWebviewPanel(
    "ricochetDebuggerStack",
    "Ricochet Debug Stack",
    { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
    { enableScripts: true },
  );
  debugStackPanel.webview.html = debuggerStackHtml(debugStackPanel.webview);
  debugStackPanel.onDidDispose(
    () => {
      debugStackPanel = undefined;
    },
    null,
    context.subscriptions,
  );
  context.subscriptions.push(debugStackPanel);
  return debugStackPanel;
}

async function readDebuggerSnapshot(session) {
  const stackTrace = await session.customRequest("stackTrace", {
    threadId: 1,
    startFrame: 0,
    levels: 1,
  });
  const frame = stackTrace.stackFrames?.[0] ?? null;
  if (!frame) {
    return {
      status: "Debugger is paused without an available stack frame.",
      frame: null,
      scopes: [],
    };
  }

  const scopesResponse = await session.customRequest("scopes", { frameId: frame.id });
  const scopes = [];
  for (const scope of scopesResponse.scopes ?? []) {
    const variables =
      scope.variablesReference > 0
        ? (await session.customRequest("variables", {
            variablesReference: scope.variablesReference,
          })).variables ?? []
        : [];
    scopes.push({
      name: scope.name,
      variables,
    });
  }

  return {
    status: "paused",
    frame,
    scopes,
  };
}

function debuggerStackHtml(webview) {
  const nonce = Math.random().toString(36).slice(2);
  const csp = [
    "default-src 'none'",
    `style-src ${webview.cspSource} 'unsafe-inline'`,
    `script-src 'nonce-${nonce}'`,
  ].join("; ");

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="${csp}">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Ricochet Debug Stack</title>
  <style>
    :root {
      color-scheme: dark light;
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
    }
    body {
      margin: 0;
      color: var(--vscode-foreground);
      background: var(--vscode-editor-background);
    }
    header {
      padding: 14px 16px 10px;
      border-bottom: 1px solid var(--vscode-panel-border);
    }
    h1 {
      margin: 0 0 6px;
      font-size: 16px;
      font-weight: 600;
    }
    .meta {
      color: var(--vscode-descriptionForeground);
      font-size: 12px;
      overflow-wrap: anywhere;
    }
    main {
      padding: 16px;
    }
    .section {
      margin: 0 0 18px;
    }
    .section h2 {
      margin: 0 0 8px;
      font-size: 13px;
      font-weight: 600;
      color: var(--vscode-descriptionForeground);
      text-transform: uppercase;
    }
    .value {
      margin: 0 0 6px;
      padding: 8px 10px;
      border: 1px solid var(--vscode-panel-border);
      border-radius: 4px;
      background: var(--vscode-editor-inactiveSelectionBackground);
      font-family: var(--vscode-editor-font-family);
      overflow-wrap: anywhere;
      white-space: pre-wrap;
    }
    .binding {
      display: grid;
      grid-template-columns: minmax(80px, 25%) 1fr;
      gap: 8px;
      align-items: start;
      margin-bottom: 6px;
    }
    .binding-name {
      color: var(--vscode-symbolIcon-variableForeground);
      overflow-wrap: anywhere;
    }
  </style>
</head>
<body>
  <header>
    <h1>Ricochet Debug Stack</h1>
    <div class="meta" id="meta">Waiting for a paused Ricochet debug session.</div>
  </header>
  <main id="detail"></main>
  <script nonce="${nonce}">
    const detail = document.getElementById("detail");
    const meta = document.getElementById("meta");

    function text(value) {
      return String(value ?? "");
    }

    function section(title) {
      const node = document.createElement("section");
      node.className = "section";
      const heading = document.createElement("h2");
      heading.textContent = title;
      node.appendChild(heading);
      return node;
    }

    function valueNode(value) {
      const node = document.createElement("div");
      node.className = "value";
      node.textContent = text(value);
      return node;
    }

    function renderSnapshot(snapshot) {
      detail.textContent = "";
      if (!snapshot || !snapshot.frame) {
        meta.textContent = snapshot?.status || "Waiting for a paused Ricochet debug session.";
        return;
      }

      const frame = snapshot.frame;
      meta.textContent = [frame.source?.path, frame.line ? ":" + frame.line : ""].filter(Boolean).join("");

      const frameSection = section("Frame");
      frameSection.appendChild(valueNode(frame.name + " at line " + frame.line));
      detail.appendChild(frameSection);

      for (const scope of snapshot.scopes || []) {
        const scopeSection = section(scope.name);
        const variables = scope.variables || [];
        if (variables.length === 0) {
          scopeSection.appendChild(valueNode("<empty>"));
        } else {
          for (const variable of variables) {
            const row = document.createElement("div");
            row.className = "binding";
            const name = document.createElement("div");
            name.className = "binding-name";
            name.textContent = text(variable.name);
            const value = valueNode(variable.value);
            row.append(name, value);
            scopeSection.appendChild(row);
          }
        }
        detail.appendChild(scopeSection);
      }
    }

    window.addEventListener("message", (event) => {
      if (event.data?.type === "snapshot") {
        renderSnapshot(event.data.snapshot);
      }
    });
  </script>
</body>
</html>`;
}

async function activate(context) {
  client = createClient();
  outputChannel = vscode.window.createOutputChannel("Ricochet");
  context.subscriptions.push(client);
  context.subscriptions.push(outputChannel);
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "ricochet.restartLanguageServer",
      restartLanguageServer,
    ),
  );
  context.subscriptions.push(
    vscode.commands.registerCommand("ricochet.runWithStackVisualizer", () =>
      runWithStackVisualizer(context),
    ),
  );
  context.subscriptions.push(
    vscode.commands.registerCommand("ricochet.showDebuggerStack", () =>
      showDebuggerStack(context),
    ),
  );
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(
      "ricochet",
      debugAdapterDescriptorFactory(),
    ),
  );
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterTrackerFactory(
      "ricochet",
      debugAdapterTrackerFactory(context),
    ),
  );
  context.subscriptions.push(
    vscode.workspace.onDidGrantWorkspaceTrust(() => {
      restartLanguageServer().catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        outputChannel.appendLine(`Ricochet language server restart after trust grant failed: ${message}`);
      });
    }),
  );
  await client.start();
}

function deactivate() {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

module.exports = {
  activate,
  deactivate,
};
