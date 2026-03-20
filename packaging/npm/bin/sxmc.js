#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const binaryName = process.platform === "win32" ? "sxmc.exe" : "sxmc";
const binaryPath = path.join(__dirname, "..", "vendor", binaryName);

if (!fs.existsSync(binaryPath)) {
  console.error(
    "sxmc binary was not installed. Reinstall the package or run the postinstall step again.",
  );
  process.exit(1);
}

const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
});

if (result.error) {
  console.error(`Failed to launch sxmc: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 1);
