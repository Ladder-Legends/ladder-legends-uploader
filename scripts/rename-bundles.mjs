#!/usr/bin/env node

/**
 * Post-build script to rename bundle files to cleaner names
 * Run after: cargo tauri build
 */

import { readFileSync, readdirSync, renameSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const rootDir = join(__dirname, '..');

// Read version from tauri.conf.json
const tauriConfig = JSON.parse(
  readFileSync(join(rootDir, 'src-tauri', 'tauri.conf.json'), 'utf8')
);
const version = tauriConfig.version;

console.log(`📦 Renaming bundle files for version ${version}...`);

// Define bundle directories and rename patterns
const bundleConfigs = [
  {
    platform: 'macOS',
    dir: join(rootDir, 'src-tauri', 'target', 'release', 'bundle', 'dmg'),
    pattern: /Ladder Legends Uploader.*\.dmg$/,
    newName: 'LadderLegendsUploader.dmg'
  },
  {
    platform: 'Windows',
    dir: join(rootDir, 'src-tauri', 'target', 'release', 'bundle', 'msi'),
    pattern: /Ladder Legends Uploader.*\.msi$/,
    newName: 'LadderLegendsUploader.msi'
  }
];

let renamedCount = 0;

bundleConfigs.forEach(config => {
  if (!existsSync(config.dir)) {
    console.log(`⏭️  Skipping ${config.platform} (directory not found)`);
    return;
  }

  const files = readdirSync(config.dir);
  const matchingFile = files.find(file => config.pattern.test(file));

  if (matchingFile) {
    const oldPath = join(config.dir, matchingFile);
    const newPath = join(config.dir, config.newName);

    try {
      renameSync(oldPath, newPath);
      console.log(`✅ ${config.platform}: ${matchingFile} → ${config.newName}`);
      renamedCount++;
    } catch (error) {
      console.error(`❌ Failed to rename ${matchingFile}:`, error.message);
    }
  } else {
    console.log(`⚠️  No ${config.platform} bundle file found matching pattern`);
  }
});

console.log(`\n✨ Done! Renamed ${renamedCount} file(s)`);

if (renamedCount === 0) {
  console.log('\n💡 Tip: Make sure you run "cargo tauri build" first');
  process.exit(1);
}
