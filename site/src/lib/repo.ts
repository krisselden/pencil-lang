// Reads files from the repo around the site at build, so the site shows the real sources and the
// real docs, never a copy of them. Pages are bundled before they run, so the repo is found from
// where `astro` was started, which is `site/`.
import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';

export const REPO_ROOT = path.resolve(process.cwd(), '..');

if (!existsSync(path.join(REPO_ROOT, 'docs/language.md'))) {
  throw new Error(`Run astro from site/: no docs/language.md under ${REPO_ROOT}`);
}

export const readRepo = (file: string) => readFile(path.join(REPO_ROOT, file), 'utf8');
