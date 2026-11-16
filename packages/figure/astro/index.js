// The Astro side of Pencil: an integration and, beside it, `Figure.astro`.
//
//   // astro.config.mjs
//   import pencil from '@pencil-lang/figure/astro';
//   export default defineConfig({ integrations: [pencil({ root: './figures' })] });
//
//   // a page
//   import Figure from '@pencil-lang/figure/Figure.astro';
//   <Figure src="tube.pen" sheets="theme.style tube.style" dark="dark.style" alt="…" />
//
// A build runs `pencil build` for each figure and each set of sheets a reader can get, and the page
// ships the compiled figures and their posters and no sources. `astro dev` hands the element the
// sources, served from where they are, and tells it when one changes, so a figure follows its
// files as they're edited without the page reloading.
//
// Options:
// - `root`: where `src` and `sheets` paths start, from the Astro project. The project, by default.
// - `playground`: the address of a Pencil playground, as `https://pencil-lang.dev/playground/`.
//   With one, `build()` gives each figure a link that opens its sources there. A built page ships
//   no sources, so the link carries them, after its `#`, which no server is sent.
// - `bin`: the `pencil` command. By default PENCIL_BIN, then `target/release/pencil` in the repo
//   around `root` or around this package, then `pencil` on the PATH.
import { cp, readFile, rm, stat } from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { PUBLIC } from './build.js';

export { build, PUBLIC } from './build.js';

const PACKAGE = fileURLToPath(new URL('..', import.meta.url));

// What a document or a sheet can `use`, and so what `astro dev` serves from `root`.
const SOURCES = new Set(['.pen', '.style']);
const TYPES = {
  '.pen': 'text/plain; charset=utf-8',
  '.style': 'text/plain; charset=utf-8',
  '.pfig': 'application/octet-stream',
  '.png': 'image/png',
  '.webp': 'image/webp',
  '.json': 'application/json',
};

function findBin(bin, root) {
  if (bin) return bin;
  if (process.env.PENCIL_BIN) return process.env.PENCIL_BIN;
  // A checkout of Pencil has it under `target/`, wherever in the checkout these are.
  for (const from of [root, PACKAGE]) {
    for (let dir = from; ; dir = path.dirname(dir)) {
      const built = path.join(dir, 'target/release/pencil');
      if (existsSync(built)) return built;
      if (dir === path.dirname(dir)) break;
    }
  }
  return 'pencil';
}

/** Which build of the core the package's WASM is, which a compiled figure has to match. */
async function wasmBuild() {
  const file = path.join(PACKAGE, 'pkg/pencil_web_bg.wasm');
  if (!existsSync(file)) {
    throw new Error(`@pencil-lang/figure: ${path.dirname(file)} has no WASM. Run scripts/build-web.sh in the Pencil repo.`);
  }
  const wasm = await import('../pkg/pencil_web.js');
  wasm.initSync({ module: readFileSync(file) });
  return wasm.fingerprint();
}

export default function pencil({ root, bin, playground } = {}) {
  let cfg;
  return {
    name: '@pencil-lang/figure',
    hooks: {
      'astro:config:setup': async ({ config, command, updateConfig }) => {
        const figures = path.resolve(fileURLToPath(config.root), root ?? '.');
        cfg = {
          root: figures,
          bin: findBin(bin && path.resolve(fileURLToPath(config.root), bin), figures),
          cache: path.join(fileURLToPath(config.cacheDir), 'pencil'),
          wasm: await wasmBuild(),
          dev: command === 'dev',
          base: config.base,
          playground: playground ?? null,
        };
        process.env.PENCIL_FIGURE = JSON.stringify(cfg);
        updateConfig({
          vite: {
            // The WASM is found beside the script that loads it, which bundling ahead would move.
            optimizeDeps: { exclude: ['@pencil-lang/figure'] },
            server: { fs: { allow: [PACKAGE] } },
          },
        });
      },

      'astro:server:setup': ({ server }) => {
        const prefix = `${cfg.base.replace(/\/$/, '')}/${PUBLIC}/`;
        const watched = new Set();
        server.watcher.on('change', (file) => {
          if (watched.has(file)) (server.hot ?? server.ws).send({ type: 'custom', event: 'pencil:change', data: {} });
        });
        server.middlewares.use(async (req, res, next) => {
          const url = decodeURIComponent((req.url ?? '').split('?')[0]);
          if (!url.startsWith(prefix)) return next();
          const rel = url.slice(prefix.length);
          // Sources come from where they are, and what a build made from the cache.
          const source = rel.startsWith('src/');
          const top = source ? cfg.root : path.join(cfg.cache, 'out');
          const file = path.resolve(top, source ? rel.slice(4) : rel);
          const type = TYPES[path.extname(file)];
          if (!file.startsWith(top + path.sep) || !type || (source && !SOURCES.has(path.extname(file)))) return next();
          try {
            if (!(await stat(file)).isFile()) return next();
            if (source && !watched.has(file)) {
              watched.add(file);
              server.watcher.add(file);
            }
            res.setHeader('Content-Type', type);
            res.setHeader('Cache-Control', 'no-store');
            res.end(await readFile(file));
          } catch {
            next();
          }
        });
      },

      // What goes out is what this build's pages asked for, and nothing an earlier one left.
      'astro:build:start': async () => {
        await rm(path.join(cfg.cache, 'out'), { recursive: true, force: true });
      },

      'astro:build:done': async ({ dir, logger }) => {
        const out = path.join(cfg.cache, 'out');
        if (!existsSync(out)) return;
        await cp(out, path.join(fileURLToPath(dir), PUBLIC), { recursive: true });
        logger.info(`wrote the figures to ${PUBLIC}/`);
      },
    },
  };
}
