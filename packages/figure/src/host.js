// The host interface: the Rust core, compiled to WASM, as a page calls it. `<pencil-figure>` is
// built on this and is what most pages want. A page that draws to its own canvas, reads how text
// parses, or converts LaTeX uses these directly.
//
// `ready()` fetches and starts the WASM, once. Everything else here needs it to have resolved:
//
//   import { ready, Player, gather } from '@pencil-lang/figure/host';
//   await ready();
//   const player = await Player.create(canvas, false);
//
// `scripts/build-web.sh` writes `../pkg`, which isn't checked in.
import init from '../pkg/pencil_web.js';

export {
  Player,
  figure_size,
  figure_size_compiled,
  named_colors,
  uses,
  fingerprint,
  inspect,
  functions,
  is_latex,
  from_latex,
  symbols,
} from '../pkg/pencil_web.js';
import { uses } from '../pkg/pencil_web.js';

let starting = null;
let started = false;

/**
 * What the page began fetching before this script ran, if it did, and hands it over: a response,
 * or nothing. A page that knows what a figure will need can start on it as it's read, in a
 * script of its own, and leave each promise here by name: `wasm`, or a compiled figure's URL.
 *
 *   (self.__pencilAhead ??= {}).wasm = fetch(url, { priority: 'low' }).catch(() => null);
 *
 * It's this and not `<link rel="preload" as="fetch">` because WebKit doesn't give a preloaded
 * fetch to the script that asks for it, and downloads the file again.
 */
export async function ahead(name) {
  const waiting = globalThis.__pencilAhead;
  const early = waiting?.[name];
  if (!early) return null;
  delete waiting[name];
  try {
    const response = await early;
    return response?.ok ? response : null;
  } catch {
    return null;
  }
}

async function start() {
  const early = await ahead('wasm');
  if (early) {
    try {
      const wasm = await init({ module_or_path: early });
      started = true;
      return wasm;
    } catch {
      // One that was cut short, as a tab left in the background can be, is fetched again.
    }
  }
  const wasm = await init();
  started = true;
  return wasm;
}

/** Fetches and starts the WASM core, once, and resolves when it can be called. */
export const ready = () => (starting ??= start());

/** Whether the core can be called now, without waiting. */
export const isReady = () => started;

const fetchText = async (url) => {
  const response = await fetch(new URL(url, document.baseURI));
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return response.text();
};

/**
 * Reads every file a document and its sheets `use`, and every file those use, for `load`.
 * `names` say where the document and then each sheet came from: URLs, which can be relative to
 * the page. A used file is found from the file that names it, so its own name is a URL too, and
 * `read` is given that. A file that can't be read is left out, and compiling says which line
 * wanted it. `have` are files already at hand, by name, such as ones being edited.
 */
export async function gather(names, texts, read = fetchText, have = {}) {
  await ready();
  const files = {};
  const visit = (name, text) =>
    Promise.all(
      uses(name, text).map(async (file) => {
        if (file in files) return;
        files[file] = null;
        try {
          files[file] = have[file] ?? (await read(file));
        } catch {
          return;
        }
        await visit(file, files[file]);
      }),
    );
  await Promise.all(texts.map((text, i) => visit(names[i] ?? '', text)));
  for (const [file, text] of Object.entries(files)) if (text === null) delete files[file];
  return { names, files };
}
