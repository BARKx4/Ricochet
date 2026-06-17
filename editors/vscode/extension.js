"use strict";

const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

let client;

function serverOptions() {
  const config = vscode.workspace.getConfiguration("ricochet");
  const command = config.get("server.path", "rco");
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

async function activate(context) {
  client = createClient();
  context.subscriptions.push(client);
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "ricochet.restartLanguageServer",
      restartLanguageServer,
    ),
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
