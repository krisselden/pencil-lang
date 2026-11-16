#!/usr/bin/env node
// Writes the site's icon, site/public/favicon.svg: the two hearts of corpus/008-hearts, one passing in front
// of the other, with their foci.
//
//   node scripts/logo.mjs           the SVG
//   node scripts/logo.mjs --touch   and site/public/apple-touch-icon.png, drawn by Playwright's Chromium
//                                   (after pnpm install)
//
// Every point of a heart is z = coth((τ − σi)/2) in bipolar coordinates about the foci ±1, with σ = t and
// τ = arctanh(cos(t − π)) for one heart and arctanh(cos t) for the other. Each is the other's mirror image.
import { writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

// The site's ink on light paper and on dark, and its light paper.
const INK = '#1c1b19';
const INK_ON_DARK = '#ece9e2';
const PAPER = '#fbfaf7';

// In the units of a 32 by 32 box, so at 32 px a unit is a pixel.
const BOX = 32;
const MARGIN = 1;
const WEIGHT = 2.6;
// What is cut from the heart behind on each side of the heart in front.
const GAP = 1.2;
// How far the drawn curves may be from the hearts, in the hearts' own units.
const TOLERANCE = 0.005;

// As the figure does, just short of 1, so that τ stays finite at the foci.
function heartAt(second, t) {
  const tau = Math.atanh(Math.cos(t + (second ? Math.PI : 0) - Math.PI) * 0.999999);
  // coth((τ − σi)/2) = (sinh τ + i sin σ) / (cosh τ − cos σ)
  const den = Math.cosh(tau) - Math.cos(t);
  return [Math.sinh(tau) / den, Math.sin(t) / den];
}

const n3 = (v) => Number(v.toFixed(3));

// Both hearts, and half a stroke around them, inside the margin.
function fit() {
  const b = { x0: Infinity, x1: -Infinity, y0: Infinity, y1: -Infinity };
  for (const second of [false, true]) {
    for (let s = 0; s <= 720; s++) {
      const [x, y] = heartAt(second, (s / 720) * 2 * Math.PI);
      b.x0 = Math.min(b.x0, x);
      b.x1 = Math.max(b.x1, x);
      b.y0 = Math.min(b.y0, y);
      b.y1 = Math.max(b.y1, y);
    }
  }
  const k = (BOX - 2 * MARGIN - WEIGHT) / Math.max(b.x1 - b.x0, b.y1 - b.y0);
  const ox = BOX / 2 - (k * (b.x0 + b.x1)) / 2;
  const oy = BOX / 2 + (k * (b.y0 + b.y1)) / 2;
  return { k, x: (x) => ox + k * x, y: (y) => oy - k * y };
}

// A heart as cubic Béziers that match its position and direction at each joint. It turns back on itself at
// both foci, t = 0 and t = π, so each half is fitted alone.
function heartPath(second, at, pieces = 20) {
  const h = 1e-6;
  let d = '';
  for (const [t0, t1] of [[0, Math.PI], [Math.PI, 2 * Math.PI]]) {
    const step = (t1 - t0) / pieces;
    const slope = (t) => {
      const lo = Math.max(t0, t - h);
      const hi = Math.min(t1, t + h);
      const p = heartAt(second, lo);
      const q = heartAt(second, hi);
      return [(q[0] - p[0]) / (hi - lo), (q[1] - p[1]) / (hi - lo)];
    };
    for (let k = 0; k < pieces; k++) {
      const ta = t0 + k * step;
      const tb = ta + step;
      const p0 = heartAt(second, ta);
      const p3 = heartAt(second, tb);
      const m0 = slope(ta);
      const m3 = slope(tb);
      const p1 = [p0[0] + (m0[0] * step) / 3, p0[1] + (m0[1] * step) / 3];
      const p2 = [p3[0] - (m3[0] * step) / 3, p3[1] - (m3[1] * step) / 3];
      if (!d) d = `M${n3(at.x(p0[0]))} ${n3(at.y(p0[1]))}`;
      d += `C${[p1, p2, p3].map((p) => `${n3(at.x(p[0]))} ${n3(at.y(p[1]))}`).join(' ')}`;
      // The math is the authority: each piece is held to the curve it stands for.
      for (let j = 1; j < 16; j++) {
        const u = j / 16;
        const v = 1 - u;
        const b = [0, 1].map((i) => v * v * v * p0[i] + 3 * v * v * u * p1[i] + 3 * v * u * u * p2[i] + u * u * u * p3[i]);
        let near = Infinity;
        for (let i = 0; i <= 400; i++) {
          const c = heartAt(second, ta + (i / 400) * step);
          near = Math.min(near, Math.hypot(c[0] - b[0], c[1] - b[1]));
        }
        if (near > TOLERANCE) throw new Error(`a piece of the heart is ${near} from its curve, past ${TOLERANCE}`);
      }
    }
  }
  return `${d}Z`;
}

// The hearts cross at (0, 1) and (0, −1), and the first is in front at both. A mask cuts the gap from the
// heart behind, so the gap is see-through and the mark sits on any background. Nothing is cut at a focus,
// where one heart's point meets the other's notch and neither passes the other.
function mark() {
  const at = fit();
  const casing = n3(WEIGHT + 2 * GAP);
  // Far enough from a crossing to hold the whole gap, and short of the foci.
  const reach = n3(Math.min(0.9 * (casing + WEIGHT), 0.8 * at.k * Math.SQRT2 - WEIGHT));
  const spot = (y) => `<circle cx="${n3(at.x(0))}" cy="${n3(at.y(y))}" r="${reach}"/>`;
  const defs =
    `<path id="a" d="${heartPath(false, at)}"/><path id="b" d="${heartPath(true, at)}"/>` +
    `<clipPath id="both">${spot(1)}${spot(-1)}</clipPath>` +
    `<mask id="under-a" maskUnits="userSpaceOnUse" x="0" y="0" width="${BOX}" height="${BOX}"><rect width="${BOX}" height="${BOX}" fill="#fff"/>` +
    `<use href="#a" style="fill:none;stroke:#000;stroke-width:${casing};stroke-linejoin:round" clip-path="url(#both)"/></mask>`;
  const foci = [-1, 1].map((x) => `<circle class="f" cx="${n3(at.x(x))}" cy="${n3(at.y(0))}" r="${n3(WEIGHT * 0.9)}"/>`).join('');
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${BOX} ${BOX}">` +
    `<style>.s{fill:none;stroke:${INK};stroke-width:${n3(WEIGHT)};stroke-linejoin:round}.f{fill:${INK}}` +
    `@media (prefers-color-scheme:dark){.s{stroke:${INK_ON_DARK}}.f{fill:${INK_ON_DARK}}}</style>` +
    `<defs>${defs}</defs><use class="s" href="#b" mask="url(#under-a)"/><use class="s" href="#a"/>${foci}</svg>\n`
  );
}

const svg = mark();
writeFileSync(join(root, 'site/public/favicon.svg'), svg);
console.log(`site/public/favicon.svg, ${svg.length} bytes`);

if (process.argv.includes('--touch')) {
  // A home screen fills what an icon leaves see-through, so this one is on the site's paper, in light ink,
  // with room around the mark for the corners a phone rounds off.
  const SIZE = 180;
  const INSET = 22;
  const { chromium } = createRequire(join(root, 'packages/figure/package.json'))('@playwright/test');
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: SIZE, height: SIZE }, deviceScaleFactor: 1, colorScheme: 'light' });
  const inner = SIZE - 2 * INSET;
  await page.setContent(
    `<body style="margin:0;background:${PAPER}"><div style="width:${inner}px;height:${inner}px;padding:${INSET}px">${svg}</div></body>`,
  );
  await page.screenshot({ path: join(root, 'site/public/apple-touch-icon.png') });
  await browser.close();
  console.log(`site/public/apple-touch-icon.png, ${SIZE}×${SIZE}`);
}
