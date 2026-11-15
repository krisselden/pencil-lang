// Builds the compiled tube once, if the release binary is there. The compiled test skips without it.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');

export default function globalSetup() {
  const binary = resolve(root, 'target/release/pencil');
  if (!existsSync(binary)) {
    console.warn(`[pencil-figure tests] ${binary} is missing: the compiled-figure test will be skipped.`);
    return;
  }
  mkdirSync(resolve(root, 'packages/figure/tests/.built'), { recursive: true });
  const run = spawnSync(
    binary,
    ['build', 'corpus/013-tube/tube.pen', '--style', 'corpus/theme.style', '--style', 'corpus/013-tube/plain.style',
      '--out', 'packages/figure/tests/.built/tube'],
    { cwd: root, encoding: 'utf8' },
  );
  if (run.status !== 0) console.warn(`[pencil-figure tests] pencil build failed: ${run.stderr || run.stdout}`);
}
