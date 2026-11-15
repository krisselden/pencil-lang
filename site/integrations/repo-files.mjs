// Serves the parts of the repo that the playground loads at run time, at the same paths they have
// in the repo, so `web/playground.html` finds `corpus/…`, `lab/explainer/…` and the element's
// package as it does under `scripts/serve.py`. Nothing is forked or copied into `src/`: the dev
// server reads the files where they are, and a build copies them into the output.
//
// The site's own figures don't come through here. `@pencil-lang/figure` builds those.
import { cp, readdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const WASM = 'packages/figure/pkg/pencil_web_bg.wasm';

// What is served, from the repo root. Each figure page in `web/` reloads its files every half
// second while Kris edits, which a published site shouldn't do, so only the playground and the
// page that shows how text parses go out, with what they import.
const DIRS = ['corpus', 'lab/explainer', 'packages/figure/pkg'];
const FILES = /^(?:web\/(?:[a-z-]+\.js|playground\.html|ast\.html)|packages\/figure\/src\/[a-z-]+\.js)$/;
// Folders with nothing of their own to serve, on the way to something that is.
const WAY = ['web', 'lab', 'packages', 'packages/figure', 'packages/figure/src'];
// Each corpus figure's golden IR is the test suite's contract, and no page reads it.
const SKIP = /(?:^|\/)(?:expected\.ir\.json|\.gitignore|\.DS_Store)$/;

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': 'application/json',
  '.pen': 'text/plain; charset=utf-8',
  '.style': 'text/plain; charset=utf-8',
  '.tex': 'text/plain; charset=utf-8',
};

/** Whether a repo-relative path is one the site serves. */
const served = (rel) =>
  !SKIP.test(rel) && (FILES.test(rel) || DIRS.some((dir) => rel === dir || rel.startsWith(`${dir}/`)));

export default function repoFiles() {
  return {
    name: 'pencil-repo-files',
    hooks: {
      'astro:server:setup': ({ server }) => {
        server.middlewares.use(async (req, res, next) => {
          const rel = decodeURIComponent((req.url ?? '').split('?')[0]).replace(/^\/+/, '');
          if (!served(rel) || rel.includes('..')) return next();
          const file = path.join(ROOT, rel);
          try {
            if (!(await stat(file)).isFile()) return next();
            res.setHeader('Content-Type', TYPES[path.extname(file)] ?? 'application/octet-stream');
            res.setHeader('Cache-Control', 'no-store');
            res.end(await readFile(file));
          } catch {
            next();
          }
        });
      },
      'astro:build:done': async ({ dir, logger }) => {
        const out = fileURLToPath(dir);
        try {
          await stat(path.join(ROOT, WASM));
        } catch {
          throw new Error('packages/figure/pkg is missing: run scripts/build-web.sh from the repo root first.');
        }
        const filter = (src) => {
          const rel = path.relative(ROOT, src).split(path.sep).join('/');
          return served(rel) || WAY.includes(rel);
        };
        for (const top of ['corpus', 'lab', 'web', 'packages']) {
          await cp(path.join(ROOT, top), path.join(out, top), { recursive: true, filter });
        }
        // The site's pages already ship the WASM, as a bundled asset. The playground loads the
        // package's files as they are, so its loader is pointed at that copy and a reader
        // fetches the 5 MB once.
        const bundled = (await readdir(path.join(out, '_astro')).catch(() => [])).find((name) => /^pencil_web_bg.*\.wasm$/.test(name));
        if (bundled) {
          const loader = path.join(out, 'packages/figure/pkg/pencil_web.js');
          const text = await readFile(loader, 'utf8');
          const there = "new URL('pencil_web_bg.wasm', import.meta.url)";
          if (!text.includes(there)) throw new Error(`${loader} doesn't load its WASM the way this build expects.`);
          await writeFile(loader, text.replace(there, `new URL('/_astro/${bundled}', import.meta.url)`));
          await rm(path.join(out, WASM));
        }
        logger.info('copied the playground, corpus/, lab/explainer/ and the package it loads from the repo');
      },
    },
  };
}
