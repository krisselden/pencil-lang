// Builds the compiled tube once, and the controls' figure, if the release binary is there. The tests
// that need them skip without it.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');

const BUILDS = [
  ['corpus/013-tube/tube.pen', ['corpus/theme.style', 'corpus/013-tube/plain.style'], 'tube'],
  // What a build says of a figure with controls and readouts, for a page written as the Astro
  // component writes one (controls.spec.js).
  ['packages/figure/tests/pages/controls.pen', ['packages/figure/tests/pages/controls.style'], 'controls'],
];

export default function globalSetup() {
  const binary = resolve(root, 'target/release/pencil');
  if (!existsSync(binary)) {
    console.warn(`[pencil-figure tests] ${binary} is missing: the tests of compiled figures will be skipped.`);
    return;
  }
  mkdirSync(resolve(root, 'packages/figure/tests/.built'), { recursive: true });
  for (const [doc, sheets, out] of BUILDS) {
    const run = spawnSync(
      binary,
      ['build', doc, ...sheets.flatMap((sheet) => ['--style', sheet]), '--out', `packages/figure/tests/.built/${out}`],
      { cwd: root, encoding: 'utf8' },
    );
    if (run.status !== 0) console.warn(`[pencil-figure tests] pencil build ${doc} failed: ${run.stderr || run.stdout}`);
  }
}
