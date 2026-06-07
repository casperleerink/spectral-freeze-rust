#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  console.error('Usage: node scripts/set-version.mjs <semver>');
  process.exit(1);
}

function replace(path, pattern, replacement) {
  const before = readFileSync(path, 'utf8');
  if (!pattern.test(before)) throw new Error(`No version found in ${path}`);
  const after = before.replace(pattern, replacement);
  writeFileSync(path, after);
}

replace('Cargo.toml', /^version = ".*"$/m, `version = "${version}"`);

execFileSync('cargo', ['metadata', '--format-version', '1'], { stdio: 'ignore' });
console.log(`Set release version to ${version}`);
