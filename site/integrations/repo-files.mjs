// Serves the parts of the repo that figures load at run time, at the same paths they have in the
// repo, so `<pencil-figure>` and the playground find `corpus/…`, `lab/explainer/…` and
// `web/pkg/…` as they do under `scripts/serve.py`. Nothing is forked or copied into `src/`:
// the dev server reads the files where they are, and a build copies them into the output.
import { cp, readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));

// What is served, from the repo root. Each figure page in `web/` reloads its files every half
// second while Kris edits, which a published site shouldn't do, so only the element, the
// playground and the page that shows how text parses go out, with what they import.
const DIRS = ['corpus', 'lab/explainer', 'web/pkg'];
const WEB_FILES = /^web\/(?:[a-z-]+\.js|playground\.html|ast\.html)$/;
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
  !SKIP.test(rel) && (WEB_FILES.test(rel) || DIRS.some((dir) => rel === dir || rel.startsWith(`${dir}/`)));

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
          await stat(path.join(ROOT, 'web/pkg/pencil_web_bg.wasm'));
        } catch {
          throw new Error('web/pkg is missing: run scripts/build-web.sh from the repo root first.');
        }
        const filter = (src) => {
          const rel = path.relative(ROOT, src).split(path.sep).join('/');
          // Directories on the way to something served are walked.
          return served(rel) || DIRS.some((d) => d.startsWith(`${rel}/`)) || rel === 'web';
        };
        for (const top of ['corpus', 'lab', 'web']) {
          await cp(path.join(ROOT, top), path.join(out, top), { recursive: true, filter });
        }
        logger.info('copied corpus/, lab/explainer/ and web/ from the repo');
      },
    },
  };
}
