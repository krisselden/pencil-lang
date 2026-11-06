// Text over a figure, for every page. A settling label's spot comes from the figure, and
// the same spring `place::spring` runs for video runs here over the page's clock, on its
// offset from its point: a reader's drag has no time axis, and only the page has its history.
const RATE = 12;
const REST = 0.01;

export class LabelLayer {
  constructor(stage, dpr) {
    this.stage = stage;
    this.dpr = dpr;
    this.items = new Map();
    this.last = 0;
    this.still = matchMedia('(prefers-reduced-motion: reduce)').matches;
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
        this.stage.append(span);
        item = { span, offset: target.slice(), velocity: [0, 0], shown: false };
        this.items.set(label.id, item);
      }
      const { span } = item;
      // What comes back starts where it belongs, not gliding in from where it went.
      if (!item.shown) {
        item.offset = target.slice();
        item.velocity = [0, 0];
        item.shown = true;
      }
      item.target = target;
      item.anchor = [label.anchorX, label.anchorY];
      item.settles = label.settles;
      if (span.textContent !== label.text) span.textContent = label.text;
      span.hidden = false;
      span.classList.toggle('math', label.math);
      span.style.fontSize = `${label.size / this.dpr}px`;
      span.style.color = label.color;
      span.style.webkitTextStrokeColor = label.haloColor;
      span.style.webkitTextStrokeWidth = `${label.halo / this.dpr}px`;
      span.style.opacity = label.opacity;
    }
    for (const [id, item] of this.items) {
      if (!present.has(id)) {
        item.span.hidden = true;
        item.shown = false;
      }
    }
    this.place();
  }

  place() {
    for (const item of this.items.values()) {
      if (!item.shown) continue;
      const [x, y] = [item.anchor[0] + item.offset[0], item.anchor[1] + item.offset[1]];
      item.span.style.transform = `translate(${x / this.dpr}px, ${y / this.dpr}px) translate(-50%, -50%)`;
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
