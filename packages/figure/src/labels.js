// Text over a figure, for every page. A settling label's spot comes from the figure, and
// the same spring `place::spring` runs for video runs here over the page's clock, on its
// offset from its point: a reader's drag has no time axis, and only the page has its history.
const RATE = 12;
const REST = 0.01;

// As `frame::LEADER_MARGIN` and `frame::TEXT_HALF_HEIGHT`, which video uses.
const LEADER_MARGIN = 0.25;
const TEXT_HALF_HEIGHT = 0.35;
const SVG = 'http://www.w3.org/2000/svg';
// The size text is measured at, in pixels.
const RULER = 100;

// The package's own fonts, Libertinus under the SIL Open Font License (`../fonts/OFL.txt`): the
// files the `pencil` command has built in, so a poster and the figure that takes over from it
// set the same glyphs at the same widths. A role is a text face with the math font behind it,
// for what the text face lacks. The math font is registered as italic too, so the browser
// doesn't slant an `ℝ` it finds there for a label in italics.
const FILES = {
  sans: new URL('../fonts/LibertinusSans-Regular.otf', import.meta.url),
  italic: new URL('../fonts/LibertinusSerif-Italic.otf', import.meta.url),
  math: new URL('../fonts/LibertinusMath-Regular.otf', import.meta.url),
};

// As `formula::BASELINE`: how far below the middle of a line the fonts' baseline is, in ems.
const BASELINE = 0.324;

export const FAMILY = {
  math: "'Pencil Serif', 'Pencil Math', Charter, 'Bitstream Charter', 'Sitka Text', Cambria, serif",
  sans: "'Pencil Sans', 'Pencil Math', system-ui, -apple-system, sans-serif",
};

// The command sets glyphs one after another by their advances, with no kerning or ligatures, so
// the page does too.
export const LABEL_CSS = `
.label { position: absolute; left: 0; top: 0; white-space: nowrap; font-family: ${FAMILY.sans}; line-height: 1; will-change: transform; font-kerning: none; font-variant-ligatures: none; font-feature-settings: "calt" 0; }
.label.math { font-family: ${FAMILY.math}; font-style: italic; }
/* A label with mathematics in it is a point, with each run and rule the core laid out placed from
   it. A run is set from the top of its line, and a stretched one grows about its baseline. */
.label.laid { width: 0; height: 0; }
.label .run { position: absolute; white-space: pre; line-height: 1; transform-origin: 0 ${(0.5 + BASELINE) * 100}%; }
.label .run.formula { font-family: 'Pencil Math', ${FAMILY.math}; font-style: normal; }
.label .rule { position: absolute; background: var(--rule); outline: var(--grow, 0px) solid var(--rule); }
`;

/** Registers the faces, and resolves when the two text faces are in, or can't be had. The math
 * font is fetched when something is first set in it. */
function loadFonts() {
  if (typeof document === 'undefined' || typeof FontFace === 'undefined') return Promise.resolve();
  const face = (family, url, style) => {
    const made = new FontFace(family, `url("${url.href}")`, { style });
    document.fonts.add(made);
    return made;
  };
  const text = [face('Pencil Sans', FILES.sans, 'normal'), face('Pencil Serif', FILES.italic, 'italic')];
  face('Pencil Math', FILES.math, 'normal');
  face('Pencil Math', FILES.math, 'italic');
  // A figure without its fonts still draws, in the fallbacks.
  return Promise.allSettled(text.map((made) => made.load())).then(() => {});
}

export const fonts = loadFonts();

export class LabelLayer {
  constructor(stage, dpr) {
    this.stage = stage;
    this.dpr = dpr;
    // Leaders under everything, then every halo under every glyph, so one term's can't eat
    // into its neighbor. The leaders' layer is in the page only while a label draws one.
    this.leaders = document.createElementNS(SVG, 'svg');
    this.leaders.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;pointer-events:none;overflow:visible';
    this.halos = this.makeLayer();
    this.glyphs = this.makeLayer();
    this.ruler = document.createElement('canvas').getContext('2d');
    this.items = new Map();
    this.advances = new Map();
    this.last = 0;
    this.still = matchMedia('(prefers-reduced-motion: reduce)').matches;
  }

  makeLayer() {
    const layer = document.createElement('div');
    layer.style.cssText = 'position:absolute;inset:0;pointer-events:none';
    this.stage.append(layer);
    return layer;
  }

  /** For `Player.set_measure`: a width in device pixels, in the same font labels are drawn in.
   * It's the sum of each character's advance, as the command measures, so no kerning. */
  measure = (text, size, math) => {
    const role = math ? 'math' : 'sans';
    let width = 0;
    for (const character of text) {
      const key = role + character;
      let advance = this.advances.get(key);
      if (advance === undefined) {
        // At a large size, so the browser's rounding of a small one doesn't add up.
        this.ruler.font = `${math ? 'italic ' : ''}${RULER}px ${FAMILY[role]}`;
        advance = this.ruler.measureText(character).width / RULER;
        this.advances.set(key, advance);
      }
      width += advance;
    }
    return width * size;
  };

  /** Forgets what was measured, when a font arrives after text was measured without it. */
  remeasure() {
    this.advances.clear();
  }

  /** Takes a frame's labels. One that isn't listed doesn't exist this frame. */
  update(labels) {
    const present = new Set();
    for (const label of labels) {
      present.add(label.id);
      let item = this.items.get(label.id);
      const target = [label.x - label.anchorX, label.y - label.anchorY];
      if (!item) {
        const span = document.createElement('span');
        span.className = 'label';
        const halo = span.cloneNode();
        // Which label it is, for whoever looks: a page's own script, or a test.
        span.dataset.label = label.id;
        this.halos.append(halo);
        this.glyphs.append(span);
        item = { span, halo, line: null, offset: target.slice(), velocity: [0, 0], shown: false };
        this.items.set(label.id, item);
      }
      const { span, halo } = item;
      // What comes back starts where it belongs, not gliding in from where it went.
      if (!item.shown) {
        item.offset = target.slice();
        item.velocity = [0, 0];
        item.shown = true;
      }
      item.target = target;
      item.anchor = [label.anchorX, label.anchorY];
      item.settles = label.settles;
      item.leader = label.leader;
      item.text = label.text;
      item.size = label.size;
      item.math = label.math;
      item.color = label.color;
      item.opacity = label.opacity;
      // What's laid out is rebuilt only when it changes, which a label that only moves doesn't.
      const laid = label.laid ? JSON.stringify(label.laid) : '';
      item.half = label.laid ? [label.laid.halfWidth, label.laid.halfHeight] : null;
      for (const part of [span, halo]) {
        if (label.laid) {
          if (part.laid !== laid) this.lay(part, label.laid);
        } else if (part.laid || part.textContent !== label.text) {
          part.textContent = label.text;
        }
        part.laid = laid;
        part.classList.toggle('laid', !!label.laid);
        part.hidden = false;
        part.classList.toggle('math', label.math);
        part.style.fontSize = `${label.size / this.dpr}px`;
        part.style.opacity = label.opacity;
      }
      span.style.color = label.color;
      span.style.setProperty('--rule', label.color);
      halo.style.color = 'transparent';
      halo.style.webkitTextStrokeColor = label.haloColor;
      halo.style.webkitTextStrokeWidth = `${label.halo / this.dpr}px`;
      // A rule's halo is the rule grown by half the halo's width, as a glyph's is its outline's.
      halo.style.setProperty('--rule', label.halo > 0 ? label.haloColor : 'transparent');
      halo.style.setProperty('--grow', `${label.halo / 2 / this.dpr}px`);
    }
    for (const [id, item] of this.items) {
      if (!present.has(id)) {
        item.span.hidden = true;
        item.halo.hidden = true;
        this.dropLeader(item);
        item.shown = false;
      }
    }
    this.place();
  }

  /** Fills a label with what the core laid out: runs of text and rules, in device pixels from the
   * label's point. */
  lay(part, laid) {
    const px = (v) => `${v / this.dpr}px`;
    part.textContent = '';
    for (const run of laid.runs) {
      const piece = document.createElement('span');
      piece.className = run.math ? 'run formula' : 'run';
      piece.textContent = run.text;
      piece.style.cssText = `left:${px(run.x)};top:${px(run.top)};font-size:${px(run.size)}`;
      if (run.stretch !== 1) piece.style.transform = `scaleY(${run.stretch})`;
      part.append(piece);
    }
    for (const rule of laid.rules) {
      const piece = document.createElement('span');
      piece.className = 'rule';
      piece.style.cssText = `left:${px(rule.x)};top:${px(rule.y)};width:${px(rule.width)};height:${px(rule.height)}`;
      part.append(piece);
    }
  }

  place() {
    for (const item of this.items.values()) {
      if (!item.shown) continue;
      const [x, y] = [item.anchor[0] + item.offset[0], item.anchor[1] + item.offset[1]];
      const transform = `translate(${x / this.dpr}px, ${y / this.dpr}px) translate(-50%, -50%)`;
      item.span.style.transform = transform;
      item.halo.style.transform = transform;
      this.placeLeader(item, x, y);
    }
  }

  /** As `frame::leader_ends`: from its gap off the point to a little short of the text's box.
   * A label has a line only while it draws one: most labels have no leader, and one that has can
   * sit too near its point to need it. */
  placeLeader(item, x, y) {
    const { leader, anchor } = item;
    const [dx, dy] = [x - anchor[0], y - anchor[1]];
    const apart = Math.hypot(dx, dy);
    if (!leader || !(apart > 0)) return this.dropLeader(item);
    const [ux, uy] = [dx / apart, dy / apart];
    const half = item.half ?? [this.measure(item.text, item.size, item.math) / 2, item.size * TEXT_HALF_HEIGHT];
    const toEdge = Math.min(half[0] / Math.max(Math.abs(ux), 1e-9), half[1] / Math.max(Math.abs(uy), 1e-9));
    const end = apart - toEdge - LEADER_MARGIN * item.size;
    if (!(end > leader.gap)) return this.dropLeader(item);
    if (!item.line) {
      item.line = document.createElementNS(SVG, 'line');
      if (this.leaders.parentNode !== this.stage) this.stage.insertBefore(this.leaders, this.halos);
      this.leaders.append(item.line);
    }
    const { line } = item;
    const at = (d) => [(anchor[0] + ux * d) / this.dpr, (anchor[1] + uy * d) / this.dpr];
    const [a, b] = [at(leader.gap), at(end)];
    line.setAttribute('x1', a[0]);
    line.setAttribute('y1', a[1]);
    line.setAttribute('x2', b[0]);
    line.setAttribute('y2', b[1]);
    line.style.strokeWidth = `${leader.width / this.dpr}px`;
    line.style.stroke = item.color;
    line.style.opacity = item.opacity;
  }

  /** Takes a label's line out, and the layer with the last one. */
  dropLeader(item) {
    if (!item.line) return;
    item.line.remove();
    item.line = null;
    if (!this.leaders.firstChild) this.leaders.remove();
  }

  /** Moves labels along by the clock. Needs no redraw of the figure. */
  tick(now) {
    const dt = Math.min((now - this.last) / 1000, 0.1);
    this.last = now;
    let moving = false;
    for (const item of this.items.values()) {
      if (!item.shown) continue;
      if (!item.settles || this.still) {
        item.offset = item.target.slice();
        item.velocity = [0, 0];
        continue;
      }
      // The exact step of a critically damped spring, so it's the same at any frame rate.
      const decay = Math.exp(-RATE * dt);
      for (let k = 0; k < 2; k++) {
        const error = item.offset[k] - item.target[k];
        const carry = (item.velocity[k] + RATE * error) * dt;
        item.offset[k] = item.target[k] + (error + carry) * decay;
        item.velocity[k] = (item.velocity[k] - RATE * carry) * decay;
        if (Math.abs(item.offset[k] - item.target[k]) > REST || Math.abs(item.velocity[k]) > REST) moving = true;
      }
    }
    this.place();
    return moving;
  }
}
