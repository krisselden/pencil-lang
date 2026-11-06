// Text over a figure, for every page. A settling label's spot comes from the figure, and
// the same spring `place::spring` runs for video runs here over the page's clock, on its
// offset from its point: a reader's drag has no time axis, and only the page has its history.
const RATE = 12;
const REST = 0.01;

const FAMILY = {
  math: "Charter, 'Bitstream Charter', 'Sitka Text', Cambria, serif",
  sans: 'system-ui, -apple-system, sans-serif',
};

export class LabelLayer {
  constructor(stage, dpr) {
    this.stage = stage;
    this.dpr = dpr;
    // Every halo under every glyph, so one term's can't eat into its neighbor.
    this.halos = this.makeLayer();
    this.glyphs = this.makeLayer();
    this.ruler = document.createElement('canvas').getContext('2d');
    this.items = new Map();
    this.last = 0;
    this.still = matchMedia('(prefers-reduced-motion: reduce)').matches;
  }

  makeLayer() {
    const layer = document.createElement('div');
    layer.style.cssText = 'position:absolute;inset:0;pointer-events:none';
    this.stage.append(layer);
    return layer;
  }

  /** For `Player.set_measure`: a width in device pixels, in the same font labels are drawn in. */
  measure = (text, size, math) => {
    this.ruler.font = `${math ? 'italic ' : ''}${size / this.dpr}px ${math ? FAMILY.math : FAMILY.sans}`;
    return this.ruler.measureText(text).width * this.dpr;
  };

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
        this.halos.append(halo);
        this.glyphs.append(span);
        item = { span, halo, offset: target.slice(), velocity: [0, 0], shown: false };
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
      for (const part of [span, halo]) {
        if (part.textContent !== label.text) part.textContent = label.text;
        part.hidden = false;
        part.classList.toggle('math', label.math);
        part.style.fontSize = `${label.size / this.dpr}px`;
        part.style.opacity = label.opacity;
      }
      span.style.color = label.color;
      halo.style.color = 'transparent';
      halo.style.webkitTextStrokeColor = label.haloColor;
      halo.style.webkitTextStrokeWidth = `${label.halo / this.dpr}px`;
    }
    for (const [id, item] of this.items) {
      if (!present.has(id)) {
        item.span.hidden = true;
        item.halo.hidden = true;
        item.shown = false;
      }
    }
    this.place();
  }

  place() {
    for (const item of this.items.values()) {
      if (!item.shown) continue;
      const [x, y] = [item.anchor[0] + item.offset[0], item.anchor[1] + item.offset[1]];
      const transform = `translate(${x / this.dpr}px, ${y / this.dpr}px) translate(-50%, -50%)`;
      item.span.style.transform = transform;
      item.halo.style.transform = transform;
    }
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
