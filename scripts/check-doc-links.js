#!/usr/bin/env node
// check-doc-links.js — validates internal links in docs/*.md files
// Exits with code 1 if any broken links are found.

import { readFileSync, readdirSync, existsSync } from 'fs';
import { join, dirname, resolve, relative } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const DOCS_DIR = join(__dirname, '..', 'docs');

function getMdFiles(dir) {
  return readdirSync(dir)
    .filter(f => f.endsWith('.md'))
    .map(f => join(dir, f));
}

function extractInternalLinks(content) {
  const linkRegex = /\[([^\]]*)\]\(([^)]+)\)/g;
  const links = [];
  let match;
  while ((match = linkRegex.exec(content)) !== null) {
    const href = match[2];
    if (href.startsWith('http') || href.startsWith('#')) continue;
    const filePart = href.split('#')[0];
    if (filePart) links.push(filePart);
  }
  return links;
}

function checkLinks() {
  const files = getMdFiles(DOCS_DIR);
  const broken = [];

  for (const file of files) {
    const content = readFileSync(file, 'utf8');
    const links = extractInternalLinks(content);

    for (const link of links) {
      const resolved = resolve(dirname(file), link);
      if (!existsSync(resolved)) {
        broken.push({ file: relative(DOCS_DIR, file), link });
      }
    }
  }

  if (broken.length === 0) {
    console.log('Checked ' + files.length + ' file(s) — no broken links.');
    return true;
  }

  console.error('Found ' + broken.length + ' broken link(s):');
  for (const { file, link } of broken) {
    console.error('  ' + file + ' -> ' + link);
  }
  return false;
}

if (!checkLinks()) process.exit(1);
