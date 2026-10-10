// The extension host entry point.
//
// Everything else this extension ships is data: the grammar, the snippets and
// the keybinding are all read out of package.json without running any of this.
// This file is the one thing that has to exist at run time — `contributes`
// lists a command in the palette and binds a key to it, and VS Code runs
// nothing until an extension host registers it under that exact id.

const vscode = require("vscode");

// The interpreter this extension drives. `rb` has to be on PATH the way `cargo`
// has to be; there is no bundled copy.
const CLI = "rb";

/** The path of the active editor, or null when nothing is open. */
function activeFile() {
  const editor = vscode.window.activeTextEditor;
  return editor ? editor.document.uri.fsPath : null;
}

/**
 * Single-quote a path for the shell.
 *
 * A workspace path may hold spaces and may hold the quote itself, and a path
 * pasted into a terminal unquoted is a command that runs something else.
 */
function shellQuote(path) {
  return `'${path.split("'").join(`'\\''`)}'`;
}

/** Run one `.rb` file in a terminal of its own and show it. */
function run(file) {
  const terminal = vscode.window.createTerminal({
    name: `rb run ${file}`,
    cwd: vscode.Uri.file(file).with({ path: file.replace(/[^/\\]+$/, "") }),
  });
  terminal.sendText(`${CLI} run ${shellQuote(file)}`);
  terminal.show(true);
}

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("redblue.run", () => {
      const file = activeFile();

      if (!file) {
        vscode.window.showErrorMessage("Open a Redblue file to run it.");
        return;
      }

      run(file);
    })
  );
}

function deactivate() {}

module.exports = { activate, deactivate };
