// A static server for the repo root, for the tests. It does what scripts/serve.py does (no caching,
// .pen and .style as text), but with a deep listen backlog: serve.py's is 5, and the first burst of
// requests from several browsers at once was refused, which showed as "Failed to fetch".
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { dirname, extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const port = Number(process.argv[2] ?? 8743);
const types = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm', '.json': 'application/json', '.css': 'text/css', '.svg': 'image/svg+xml',
  '.png': 'image/png', '.pen': 'text/plain; charset=utf-8', '.style': 'text/plain; charset=utf-8',
  '.pfig': 'application/octet-stream',
};

createServer(async (req, res) => {
  try {
    const path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname));
    const file = join(root, path);
    if (!file.startsWith(root) || !(await stat(file)).isFile()) throw new Error('not found');
    const body = await readFile(file);
    res.writeHead(200, { 'Content-Type': types[extname(file)] ?? 'application/octet-stream', 'Cache-Control': 'no-cache', 'Content-Length': body.length });
    res.end(body);
  } catch {
    res.writeHead(404, { 'Content-Type': 'text/plain' });
    res.end('not found');
  }
}).listen({ port, host: '127.0.0.1', backlog: 1024 }, () => console.log(`serving ${root} at http://127.0.0.1:${port}/`));
