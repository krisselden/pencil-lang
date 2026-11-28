// A figure's sources as one string that fits in a link, and back. The playground reads one from
// the address after `#pen=`, and writes one to share what's in its editor. A build writes one for
// a figure it compiled, so a page can link a reader from the figure to its sources.
//
//   const link = `https://pencil-lang.dev/playground/#pen=${await pack(sources)}`;
//
// `sources` is `{ name, doc, sheets, files }`: what to call it, the document's name, its sheets'
// names in order, and every file's text by name, with the document, the sheets and each file they
// `use`. Names are paths, and a used file is found from the file that names it, as everywhere.
//
// It's the JSON, deflated, in the base64 that's safe in an address. Nothing is sent anywhere: what
// follows a `#` stays in the browser. It runs the same in a page and in Node.

const VERSION = 1;

// Writing and reading are waited on together, so text that isn't deflate fails here, as one
// error, and not as a write nobody was listening to.
const through = async (stream, bytes) => {
  const writer = stream.writable.getWriter();
  const [out] = await Promise.all([
    new Response(stream.readable).arrayBuffer(),
    writer.write(bytes),
    writer.close(),
  ]);
  return new Uint8Array(out);
};

const toBase64 = (bytes) => {
  let text = '';
  for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(text).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};

const fromBase64 = (text) => {
  const binary = atob(text.replace(/-/g, '+').replace(/_/g, '/'));
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
};

/** Sources as a string for a link. */
export async function pack({ name = '', doc, sheets = [], files }) {
  const json = JSON.stringify({ v: VERSION, name, doc, sheets, files });
  return toBase64(await through(new CompressionStream('deflate-raw'), new TextEncoder().encode(json)));
}

/**
 * Sources back from a link's string. Throws, with a message a reader can be shown, when it isn't
 * one, was cut short, or doesn't hold what it says it does.
 */
export async function unpack(text) {
  let sources;
  try {
    const bytes = await through(new DecompressionStream('deflate-raw'), fromBase64(text));
    sources = JSON.parse(new TextDecoder().decode(bytes));
  } catch {
    throw new Error('This link doesn’t hold a figure: it may have been cut short when it was copied.');
  }
  if (sources?.v !== VERSION) throw new Error('This link was made by a newer Pencil than this page has.');
  const { name = '', doc, sheets = [], files } = sources;
  const text_ = (path) => typeof files?.[path] === 'string';
  if (typeof doc !== 'string' || !Array.isArray(sheets) || ![doc, ...sheets].every(text_)) {
    throw new Error('This link doesn’t hold a figure: it names a file it doesn’t carry.');
  }
  for (const [path, body] of Object.entries(files)) {
    if (typeof body !== 'string') throw new Error(`This link doesn’t hold a figure: ${path} isn’t text.`);
  }
  return { name: String(name), doc, sheets, files };
}
