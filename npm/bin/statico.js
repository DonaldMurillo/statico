#!/usr/bin/env node
// Thin shim that execs the platform-specific statico binary fetched by
// scripts/install.js. Keeps stdout/stderr/exit-code transparent.

'use strict';

const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');

const binDir = path.join(__dirname);
const exe = process.platform === 'win32' ? 'statico.exe' : 'statico';
const binary = path.join(binDir, exe);

if (!fs.existsSync(binary)) {
  process.stderr.write(
    `statico: binary not found at ${binary}.\n` +
    `Re-run \`npm install\` (or \`npm install @statico/cli --force\`) to fetch it.\n`
  );
  process.exit(1);
}

const child = spawn(binary, process.argv.slice(2), {
  stdio: 'inherit',
  windowsHide: true,
});

child.on('error', (err) => {
  process.stderr.write(`statico: failed to spawn binary: ${err.message}\n`);
  process.exit(1);
});

child.on('close', (code, signal) => {
  if (signal) {
    // Re-raise the signal so the parent shell sees it.
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 0);
  }
});
