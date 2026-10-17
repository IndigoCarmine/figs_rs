// The figs extension: starts `figs-lsp` for `.figs` files. Highlighting comes
// from the TextMate grammar, refined by the server's semantic tokens.

const fs = require("fs");
const path = require("path");
const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

/** The server executable: the setting, a build in the workspace, or PATH. */
function serverPath() {
  const configured = vscode.workspace.getConfiguration("figs").get("server.path");
  if (configured) {
    return configured;
  }
  const exe = process.platform === "win32" ? "figs-lsp.exe" : "figs-lsp";
  for (const folder of vscode.workspace.workspaceFolders || []) {
    for (const profile of ["release", "debug"]) {
      const candidate = path.join(folder.uri.fsPath, "target", profile, exe);
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    }
  }
  return "figs-lsp";
}

function start() {
  const command = serverPath();
  client = new LanguageClient(
    "figs",
    "figs",
    { command, args: ["--stdio"] },
    { documentSelector: [{ language: "figs" }] }
  );
  return client.start().catch((err) => {
    vscode.window.showErrorMessage(
      `figs: could not start the language server (${command}): ${err.message}. ` +
        "Install it with `cargo install --path crates/figs-lsp`, or set figs.server.path."
    );
  });
}

async function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("figs.restartServer", async () => {
      if (client) {
        await client.stop();
      }
      await start();
    })
  );
  await start();
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
