#!/usr/bin/env node

const { spawnSync } = require("node:child_process");
const path = require("node:path");

const targets = {
  "darwin-arm64": "mvp-darwin-arm64",
  "darwin-x64": "mvp-darwin-x64",
  "linux-arm64": "mvp-linux-arm64",
  "linux-x64": "mvp-linux-x64",
  "win32-x64": "mvp-win32-x64.exe",
};

const target = `${process.platform}-${process.arch}`;
const executable = targets[target];
if (!executable) {
  console.error(`MVP does not support ${process.platform} ${process.arch}.`);
  process.exit(1);
}

const result = spawnSync(path.join(__dirname, executable), process.argv.slice(2), {
  stdio: "inherit",
});
if (result.error) {
  console.error(`Could not start MVP: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
