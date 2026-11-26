import assert from "node:assert/strict";
import {
  isTerminalCopyKey,
  isTerminalInterruptKey,
} from "../src/terminalKeys.ts";
const key = (options = {}) => ({
  key: "c",
  ctrlKey: false,
  shiftKey: false,
  metaKey: false,
  altKey: false,
  ...options,
});
assert.equal(
  isTerminalCopyKey(key({ ctrlKey: true }), true),
  true,
  "Ctrl+C copies selected output",
);
assert.equal(
  isTerminalCopyKey(key({ ctrlKey: true }), false),
  false,
  "Ctrl+C without selection must reach the shell as interrupt",
);
assert.equal(
  isTerminalCopyKey(key({ ctrlKey: true, shiftKey: true }), true),
  true,
  "Ctrl+Shift+C copies",
);
assert.equal(
  isTerminalCopyKey(key({ ctrlKey: true, shiftKey: true }), false),
  true,
  "Ctrl+Shift+C with no selection must not interrupt",
);
assert.equal(
  isTerminalCopyKey(key({ metaKey: true }), true),
  true,
  "Cmd+C copies on macOS",
);
assert.equal(
  isTerminalCopyKey(key({ metaKey: true }), false),
  true,
  "Cmd+C must not become remote input",
);
assert.equal(
  isTerminalCopyKey(key({ ctrlKey: true, altKey: true }), true),
  false,
  "Do not intercept AltGr combinations",
);
assert.equal(
  isTerminalCopyKey(key(), true),
  false,
  "Ordinary typing remains terminal input",
);
assert.equal(
  isTerminalCopyKey(key({ ctrlKey: true, key: "v" }), true),
  false,
  "Do not change paste routing",
);

assert.equal(
  isTerminalInterruptKey(key({ ctrlKey: true }), false),
  true,
  "Bare Ctrl+C sends interrupt",
);
assert.equal(
  isTerminalInterruptKey(key({ ctrlKey: true }), true),
  false,
  "Copying selected output must not interrupt",
);
assert.equal(
  isTerminalInterruptKey(key({ ctrlKey: true, shiftKey: true }), false),
  false,
  "Copy shortcut must not interrupt",
);
assert.equal(
  isTerminalCopyKey(key({ ctrlKey: true, code: "KeyC", key: "\u0441" }), true),
  true,
  "Copy works with non-Latin keyboard layouts",
);

console.log("Terminal copy/interrupt shortcut checks passed.");
