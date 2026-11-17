// Builds a figure for a page, by running `pencil build`: the compiled figure, its poster, what the
// page needs to know about it, and on request a card for sharing. `Figure.astro` calls this, and a
// page can too, for a figure's `info` or its card.
//
// Nothing is made twice: what a build wrote is kept in Astro's cache, and used again until one of
// the files that went into it, or `pencil` itself, is newer. A site can keep it in its own repo
// instead (the integration's `kept`), and then a build that has no `pencil` uses what was
// committed, for as long as the sources still hold what they did.
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cp, mkdir, readdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { pack } from '../src/pack.js';

/** Where a site serves what a build made, and in `astro dev` the sources under `src/`. */
export const PUBLIC = '_pencil';

/** What the integration worked out, which it leaves in the environment for the pages it renders,
 * since they're bundled and can run apart from it: where the figures are (`root`), the `pencil`
 * command (`bin`), where to keep what's made (`cache`), the folder in the site's repo that holds
 * built figures if it keeps them there (`kept`), which build of the core the package's WASM is
 * (`wasm`), whether this is `astro dev` (`dev`), and the site's `base`. */
export function config() {
  const text = process.env.PENCIL_FIGURE;
  if (!text) {
    throw new Error(
      "@pencil-lang/figure: add the integration to astro.config (`import pencil from '@pencil-lang/figure/astro'`, then `integrations: [pencil()]`). It says where the figures are and where `pencil` is.",
    );
  }
  return JSON.parse(text);
}

const hash = (text) => createHash('sha1').update(text).digest('hex');
const modified = async (file) => (await stat(file)).mtimeMs;

/** A hash of every file a build wrote into `dir`, by its name and its bytes. */
async function contents(dir) {
  const sum = createHash('sha1');
  for (const file of (await readdir(dir)).sort()) {
    if (file !== STAMP) sum.update(`${file}\0`).update(await readFile(path.join(dir, file)));
  }
  return sum.digest('hex');
}

// At most this many `pencil` at once: each starts a GPU.
const ROOM = 4;
let busy = 0;
const waiting = [];
const turn = async (work) => {
  if (busy >= ROOM) await new Promise((go) => waiting.push(go));
  else busy += 1;
  try {
    return await work();
  } finally {
    // The next one waiting takes this place, so no one else can slip in ahead of it.
    const next = waiting.shift();
    if (next) next();
    else busy -= 1;
  }
};

const run = (bin, args, cwd) =>
  new Promise((resolve, reject) => {
    execFile(bin, args, { cwd, maxBuffer: 1 << 24 }, (error, stdout, stderr) => {
      if (!error) return resolve(stdout);
      if (error.code === 'ENOENT') {
        const missing = new Error(
          `@pencil-lang/figure: couldn't run \`${bin}\`. Build it with \`cargo build --release -p pencil-cli\`, or say where it is with the integration's \`bin\` or PENCIL_BIN.`,
        );
        missing.missing = true;
        return reject(missing);
      }
      // The CLI's own message says the file, the line and what's wrong.
      reject(new Error((stderr || stdout || error.message).trim()));
    });
  });

/** In a folder of kept figures, what its sources held when it was built. */
const STAMP = 'sources.sha1';

/** A hash of a figure's sources, by each one's path and what it holds. */
async function sourcesHash(sources, cfg) {
  const sum = createHash('sha1');
  for (const file of sources) sum.update(`${file}\0`).update(await readFile(path.resolve(cfg.root, file))).update('\0');
  return sum.digest('hex');
}

/** What a build wrote into `dir`, as `{ manifest }` if it's still what a build would write, and
 * otherwise `{ why }`, which says what's the matter with it. */
async function fresh(dir, stem, cfg) {
  let manifest;
  try {
    manifest = JSON.parse(await readFile(path.join(dir, `${stem}.json`), 'utf8'));
  } catch {
    return { why: 'nothing is built for it' };
  }
  try {
    if (cfg.kept) {
      // A checkout gives every file the time it was checked out, so what's kept in a repo is
      // compared by what its sources hold. A compiled figure is read only by the build of the core
      // that wrote it, so one from another build is as good as none.
      if (manifest.build !== cfg.wasm) return { why: 'what is kept was built by another build of Pencil' };
      const stamp = (await readFile(path.join(dir, STAMP), 'utf8')).trim();
      if (stamp !== (await sourcesHash(manifest.sources, cfg))) return { why: 'a source has changed since it was built' };
      return { manifest };
    }
    const since = await modified(path.join(dir, `${stem}.json`));
    const inputs = [...manifest.sources.map((file) => path.resolve(cfg.root, file))];
    // `pencil` is found on the PATH when it isn't a path, and then its age isn't known.
    if (path.isAbsolute(cfg.bin)) inputs.push(cfg.bin);
    for (const file of inputs) if ((await modified(file)) > since) return { why: 'a source or `pencil` is newer' };
    return { manifest };
  } catch {
    return { why: 'a source has changed since it was built' };
  }
}

const list = (value) => (Array.isArray(value) ? value : String(value ?? '').split(/\s+/).filter(Boolean));

/**
 * The sets of sheets a reader can get: the figure's own as `sheets`, and as `others` each set that
 * stands in for them under a media query, `{ media, sheets }`. `dark` are added after `sheets`
 * when the reader's theme is dark, and `narrow` before those when `narrowMedia` matches. As in
 * `<picture>` the first that matches wins, so the narrowest case goes first. Each of `sheets`,
 * `dark` and `narrow` is a list, or names with spaces between.
 */
export function alternates({ sheets, dark, narrow, narrowMedia = '(max-width: 600px)' }) {
  const [always, whenDark, whenNarrow] = [list(sheets), list(dark), list(narrow)];
  const DARK = '(prefers-color-scheme: dark)';
  const others = [
    whenNarrow.length && whenDark.length
      ? { media: `${narrowMedia} and ${DARK}`, sheets: [...always, ...whenNarrow, ...whenDark] }
      : null,
    whenDark.length ? { media: DARK, sheets: [...always, ...whenDark] } : null,
    whenNarrow.length ? { media: narrowMedia, sheets: [...always, ...whenNarrow] } : null,
  ].filter((choice) => choice !== null);
  return { sheets: always, others };
}

/** A link into a playground that carries everything the figure was built from, or nothing if
 * the integration wasn't given a playground. */
async function playgroundLink(cfg, name, doc, sheets, sources) {
  if (!cfg.playground) return null;
  const files = {};
  for (const file of sources) files[file] = await readFile(path.resolve(cfg.root, file), 'utf8');
  return `${cfg.playground}#pen=${await pack({ name, doc, sheets, files })}`;
}

const making = new Map();

/**
 * Builds one figure under one set of sheets.
 *
 * - `src`: the document, a path from the integration's `root`.
 * - `sheets`: its sheets, in order, the same way.
 * - `time`, `params`: the poster's moment, if not the document's own `poster`.
 * - `size`: the poster's width, for a figure with no design width.
 * - `card`: also a 1200×630 card for sharing.
 *
 * Gives `{ compiled, poster: { src, srcset, webp, width, height }, card, info, moment, hash }`,
 * with URLs from the site's root. `moment` is the figure as its poster shows it, which is how a
 * page that holds the same params starts it: `{ time, values, ranges }`, the last two for every
 * param by name, so a page can write its controls as they'll be before the figure runs.
 * `playground` is a link that opens the figure's sources in a playground, when the integration
 * was told where one is: the page ships none of them, so the link carries them all. `webp` is a `srcset` of the same poster as WebP, where that's the smaller,
 * and in `astro dev` also `src` and `sheets`, the sources' URLs, which the
 * element is given there so a figure follows its files as they're edited. A document that doesn't
 * compile fails the build with the compiler's message. In `astro dev` it gives the sources alone,
 * and the element says what's wrong.
 */
export function build({ src, sheets = [], time, params = {}, size, card = false }) {
  const cfg = config();
  const key = hash(JSON.stringify([src, sheets, time ?? null, Object.entries(params).sort(), size ?? null, card]));
  if (!making.has(key)) {
    const made = make(cfg, key, { src, sheets, time, params, size, card }).finally(() => making.delete(key));
    making.set(key, made);
  }
  return making.get(key);
}

async function make(cfg, key, { src, sheets, time, params, size, card }) {
  const base = `${cfg.base.replace(/\/$/, '')}/${PUBLIC}`;
  const sources = cfg.dev
    ? { src: `${base}/src/${src}`, sheets: sheets.map((sheet) => `${base}/src/${sheet}`) }
    : {};
  const stem = path.basename(src).replace(/\.[^.]+$/, '');
  const folder = `${stem}-${key.slice(0, 12)}`;
  const dir = path.join(cfg.kept ?? path.join(cfg.cache, 'made'), folder);
  const manifestFile = path.join(dir, `${stem}.json`);
  // So a build can say afterwards which kept folders nothing asked for.
  if (cfg.kept && !cfg.dev) {
    await mkdir(path.join(cfg.cache, 'used'), { recursive: true });
    await writeFile(path.join(cfg.cache, 'used', folder), '');
  }

  let { manifest, why } = await fresh(dir, stem, cfg);
  if (!manifest) {
    const args = ['build', src, ...sheets.flatMap((sheet) => ['--style', sheet])];
    if (time != null) args.push('--t', String(time));
    for (const [name, value] of Object.entries(params)) args.push('--param', `${name}=${value}`);
    if (size != null) args.push('--size', String(size));
    if (card) args.push('--card');
    args.push('--out', dir);
    // Whatever an older `pencil` left there under another name would be shipped, and named in.
    await rm(dir, { recursive: true, force: true });
    try {
      await turn(() => run(cfg.bin, args, cfg.root));
    } catch (error) {
      // While a figure is being written it's often broken for a moment.
      if (cfg.dev) {
        console.warn(`[pencil] ${error.message}`);
        return { ...sources, compiled: null, poster: null, card: null, info: null, moment: null, playground: null, hash: null };
      }
      // A site that keeps its figures is built where there's no `pencil`, and then the one thing
      // to say is which figure isn't what was committed, and why.
      if (cfg.kept && error.missing) {
        const kept = path.relative(process.cwd(), cfg.kept) || '.';
        throw new Error(
          `@pencil-lang/figure: \`${src}\`${sheets.length ? ` with ${sheets.join(', ')}` : ''} needs building (${why}), and \`${cfg.bin}\` isn't here to build it. Build the site where \`pencil\` and a GPU are, and commit \`${kept}\`.`,
        );
      }
      throw error;
    }
    manifest = JSON.parse(await readFile(manifestFile, 'utf8'));
    if (cfg.kept) await writeFile(path.join(dir, STAMP), `${await sourcesHash(manifest.sources, cfg)}\n`);
  }

  if (manifest.build !== cfg.wasm) {
    throw new Error(
      `@pencil-lang/figure: \`${cfg.bin}\` is build ${manifest.build} of Pencil and the package's WASM is ${cfg.wasm}, so a page couldn't load what it compiles. Build both from the same source: \`cargo build --release -p pencil-cli\` and \`scripts/build-web.sh\`.`,
    );
  }

  // What a reader fetches is named for what's in it, so a copy they kept is never a stale one. That's
  // the files' own bytes: a change to the renderer draws the same figure's poster differently, and
  // nothing the build wrote about the figure would say so.
  const name = `${stem}-${(await contents(dir)).slice(0, 12)}`;
  const out = path.join(cfg.cache, 'out', name);
  if (!existsSync(path.join(out, `${stem}.json`))) {
    await mkdir(path.dirname(out), { recursive: true });
    await cp(dir, out, { recursive: true, filter: (file) => path.basename(file) !== STAMP });
  }
  const url = (file) => `${base}/${name}/${file}`;
  const [one, two] = manifest.poster.images;
  // The WebP is offered only when both sizes have one, so a reader gets one kind at any density.
  const lighter = one.webp && (!two || two.webp);
  return {
    ...sources,
    compiled: url(manifest.figure),
    poster: {
      src: url(one.src),
      srcset: two ? `${url(one.src)}, ${url(two.src)} 2x` : undefined,
      webp: lighter ? (two ? `${url(one.webp)}, ${url(two.webp)} 2x` : url(one.webp)) : undefined,
      width: one.width,
      height: one.height,
    },
    card: manifest.card ? { src: url(manifest.card.src), width: manifest.card.width, height: manifest.card.height } : null,
    info: manifest.info,
    moment: { time: manifest.poster.time, values: manifest.poster.values, ranges: manifest.poster.ranges },
    playground: await playgroundLink(cfg, stem, src, sheets, manifest.sources),
    hash: manifest.hash,
  };
}
