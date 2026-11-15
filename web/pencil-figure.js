// <pencil-figure>: a Pencil figure on a page. It owns the canvas, the labels over it, the GPU and
// the clock, so a page only says which figure and which sheets:
//
//   <pencil-figure src="unroll.pen" sheets="theme.style blog.style"></pencil-figure>
//
// The clock is a Web Animations `Animation` on the element (`clock`), so its time, pausing and
// rate are the browser's own, and DevTools' Animations panel can scrub it. Time and a reader's input
// both set the figure's bindings, as everywhere else: nothing here works out a value of its own.
//
// The GPU starts only when the element comes near the viewport, the clock pauses while it's away,
// and both are let go when it leaves the page, since browsers cap how many contexts can be live.
// A figure that never reads time gets no clock at all. Nothing draws while nothing changes.
//
// A document or sheet that says `use "parts/arms.pen"` gets that file fetched too, from beside the
// file that says it.
//
// A build can compile a figure ahead (`pencil build`), and then a page gives that one file:
//
//   <pencil-figure compiled="tube.pfig">
//     <source media="(prefers-color-scheme: dark)" compiled="tube.dark.pfig">
//     <img src="tube.png" width="640" height="524" alt="…">
//   </pencil-figure>
//
// Its children are its poster: they show until the first frame is drawn, and stay where a figure
// can't run. With one, nothing is fetched until the figure is near the viewport, since the poster
// already has its shape. A `<source>` child picks other sources while its `media` matches, as in
// `<picture>`: the first that matches wins, and the element's own attributes are the rest. It can
// give `compiled`, or `sheets` (and `src`, which is the element's if it gives none).
//
// Attributes: `src` and `sheets` (space-separated URLs, in order), or `compiled`, `params`
// (`a: 0.5; beat: 2`, held as if a reader set them), `paused` to start paused, and `gl` to skip
// WebGPU. A page can also pick its sheets in script, by setting `sheets` as soon as the element
// is defined: changes made before that script finishes are fetched once.
// Its labels are an em of the text where it sits, and follow when a reader changes their text size.
// Events: `load` when a figure is compiled, `draw` after each frame drawn, `resize` when its drawn
// size or its em changes (its `detail.em`, in CSS pixels), `input` when a reader
// drags a param, `play` and `pause`, and `error` (its `detail.error`) when one can't load or draw.
import init, { Player, figure_size, figure_size_compiled, uses } from './pkg/pencil_web.js';
import { LabelLayer } from './labels.js';
import { match } from './fit.js';

// How long time runs before it loops, for a figure that declares no `duration`.
const LOOP = 10;
// How near a handle a pointer has to be to take it, in CSS pixels.
const REACH = 24;
// How far from the viewport the GPU starts, so a figure is drawn by the time it's scrolled to.
const NEAR = '200px';

let wasm = null;
const ready = () => (wasm ??= init());

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

const CSS = `
:host { display: block; }
:host([hidden]) { display: none; }
.stage { position: relative; overflow: hidden; width: 100%; }
canvas { display: block; width: 100%; aspect-ratio: var(--pencil-aspect, 16 / 9); outline: none; }
/* The poster: the element's children, until the first frame. It has the figure's shape, and the
   canvas waits under it at the same size, so nothing moves when they swap. */
slot { display: none; }
.stage.waiting slot { display: block; }
.stage.waiting canvas { position: absolute; inset: 0; height: 100%; aspect-ratio: auto; opacity: 0; }
::slotted(source) { display: none; }
::slotted(img), ::slotted(picture) { display: block; width: 100%; height: auto; }
.em { position: absolute; left: 0; top: 0; width: 1em; height: 0; visibility: hidden; pointer-events: none; }
.label { position: absolute; left: 0; top: 0; white-space: nowrap; font-family: system-ui, -apple-system, sans-serif; line-height: 1; will-change: transform; }
.label.math { font-family: Charter, 'Bitstream Charter', 'Sitka Text', Cambria, serif; font-style: italic; }
`;

/** `a: 0.5; beat: 2` as pairs. */
const parseParams = (text) =>
  (text ?? '')
    .split(';')
    .map((part) => part.split(':').map((s) => s.trim()))
    .filter(([name, value]) => name && value !== undefined && value !== '' && Number.isFinite(Number(value)))
    .map(([name, value]) => [name, Number(value)]);

export class PencilFigure extends HTMLElement {
  static observedAttributes = ['src', 'sheets', 'compiled', 'params'];

  /** Resolves once the WASM core is ready, after which `load` checks sources at once. */
  static get ready() {
    return ready();
  }

  #stage;
  #canvas;
  #em;
  #emPx = 0;
  #layer;
  #size = null;
  #player = null;
  #starting = false;
  #generation = 0;
  #sources = null; // { doc, sheets, names, files } or { compiled }, loaded or waiting for the GPU
  #drawn = false; // whether a frame has been drawn since the canvas was made
  #children = null; // watches for a poster or a <source> arriving
  #media = []; // each <source>'s media query, and what's listening to it
  #fetched = null;
  #fetching = false;
  #info = null;
  #driven = new Set();
  #held = new Map();
  #clock = null;
  #rate = 1;
  #time = 0;
  #paused = null;
  #visible = false;
  #raf = 0;
  #idle = true;
  #dirty = false;
  #fresh = false;
  #seen = null;
  #resized = null;

  constructor() {
    super();
    const root = this.attachShadow({ mode: 'open' });
    const style = document.createElement('style');
    style.textContent = CSS;
    this.#stage = document.createElement('div');
    this.#stage.className = 'stage';
    this.#stage.setAttribute('part', 'stage');
    root.append(style, this.#stage);
    this.#canvas = this.#makeCanvas();
    // One em wide and out of sight. A reader changing their text size resizes it, though nothing
    // else about the figure changes, so the labels follow.
    this.#em = document.createElement('span');
    this.#em.className = 'em';
    this.#em.setAttribute('aria-hidden', 'true');
    this.#stage.append(document.createElement('slot'), this.#canvas, this.#em);
    this.#layer = new LabelLayer(this.#stage, window.devicePixelRatio || 1);
  }

  // ---- The poster and the sources ----

  /** Whether the element has a poster: any child that isn't a `<source>`. */
  #hasPoster() {
    return [...this.children].some((child) => child.localName !== 'source');
  }

  /** Shows the poster until a frame is drawn. */
  #wait() {
    this.#stage.classList.toggle('waiting', !this.#drawn && this.#hasPoster());
  }

  /** Where the sources come from now: the first `<source>` child whose media matches, or the
   * element itself. */
  #chosen() {
    for (const source of this.querySelectorAll(':scope > source')) {
      const media = source.getAttribute('media');
      if (!media || matchMedia(media).matches) return source;
    }
    return this;
  }

  /** Follows each `<source>`'s media query, so the figure changes sheets when the reader's
   * theme or the page's width does. */
  #listen() {
    for (const [query, listener] of this.#media) query.removeEventListener('change', listener);
    this.#media = [];
    if (!this.isConnected) return;
    for (const source of this.querySelectorAll(':scope > source[media]')) {
      const query = matchMedia(source.getAttribute('media'));
      const listener = () => this.#refetch();
      query.addEventListener('change', listener);
      this.#media.push([query, listener]);
    }
  }

  /** Fetches the sources, unless a poster is standing in and the figure isn't near yet. */
  #refetch() {
    if (this.#visible || !this.#hasPoster()) this.#queueFetch();
  }

  // ---- Lifecycle ----

  connectedCallback() {
    this.#paused ??= this.hasAttribute('paused') || matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (!this.#clock) {
      this.#clock = this.animate([], { id: this.#clockName(), duration: this.duration * 1000, iterations: Infinity });
      this.#clock.pause();
      this.#clock.currentTime = this.#time * 1000;
      this.#clock.playbackRate = this.#rate;
    }
    this.#seen = new IntersectionObserver(
      (entries) => {
        this.#visible = entries[entries.length - 1].isIntersecting;
        if (this.#visible) {
          this.#queueFetch();
          this.#start();
        }
        this.#sync();
      },
      { rootMargin: NEAR },
    );
    this.#seen.observe(this);
    this.#resized = new ResizeObserver(() => this.#follow());
    this.#resized.observe(this.#canvas);
    this.#resized.observe(this.#em);
    // Children can arrive after the element does, while the page is still being read.
    this.#children = new MutationObserver(() => {
      this.#wait();
      this.#listen();
      this.#refetch();
    });
    this.#children.observe(this, { childList: true, attributes: true, subtree: true, attributeFilter: ['media', 'src', 'sheets', 'compiled'] });
    this.#wait();
    this.#listen();
    this.#refetch();
  }

  disconnectedCallback() {
    this.#seen?.disconnect();
    this.#resized?.disconnect();
    this.#children?.disconnect();
    this.#listen();
    this.#visible = false;
    this.#sync();
    // Moving an element disconnects it and connects it again at once: only let go if it stays out.
    queueMicrotask(() => {
      if (!this.isConnected) this.#teardown();
    });
  }

  attributeChangedCallback(name) {
    if (name === 'params') {
      for (const [param, value] of parseParams(this.getAttribute('params'))) this.hold(param, value);
    } else {
      this.#refetch();
    }
  }

  #makeCanvas() {
    const canvas = document.createElement('canvas');
    canvas.setAttribute('part', 'canvas');
    if (this.#size) canvas.style.setProperty('--pencil-aspect', `${this.#size[0]} / ${this.#size[1]}`);
    canvas.addEventListener('pointerdown', this.#pointerDown);
    canvas.addEventListener('pointermove', this.#pointerMove);
    canvas.addEventListener('pointerup', this.#pointerEnd);
    canvas.addEventListener('pointercancel', this.#pointerEnd);
    return canvas;
  }

  /** Starts the GPU, once, when the element is near the viewport. */
  async #start() {
    if (this.#player || this.#starting) return;
    this.#starting = true;
    const generation = this.#generation;
    try {
      await ready();
      if (generation !== this.#generation) return;
      match(this.#canvas);
      const player = await Player.create(this.#canvas, this.hasAttribute('gl'));
      if (generation !== this.#generation) {
        player.destroy();
        return;
      }
      this.#player = player;
      player.set_measure(this.#layer.measure);
      this.#setEm();
      if (this.#sources) {
        try {
          this.#compile(this.#sources);
        } catch (error) {
          this.#fail(error);
        }
      }
    } catch (error) {
      this.#fail(error);
    } finally {
      if (generation === this.#generation) this.#starting = false;
    }
  }

  /** Lets go of the clock and the GPU. The figure's sources, time and held params are kept, so
   * it starts again where it was if it's put back. */
  #teardown() {
    this.#time = this.time;
    this.#clock?.cancel();
    this.#clock = null;
    cancelAnimationFrame(this.#raf);
    this.#raf = 0;
    this.#idle = true;
    const started = this.#player || this.#starting;
    this.#generation += 1;
    this.#starting = false;
    if (this.#player) {
      const gl = this.#player.backend === 'Gl';
      this.#player.destroy();
      this.#player = null;
      if (gl) this.#canvas.getContext('webgl2')?.getExtension('WEBGL_lose_context')?.loseContext();
      else this.#canvas.getContext('webgpu')?.unconfigure();
    }
    // A canvas keeps its context for life, so a figure put back gets a new one.
    if (started) {
      const canvas = this.#makeCanvas();
      this.#canvas.replaceWith(canvas);
      this.#canvas = canvas;
      // And its poster back, until that one has drawn.
      this.#drawn = false;
      this.#wait();
    }
  }

  // ---- Sources ----

  /** Fetches the sources once the task that set them is done, so `src` and `sheets` set one
   * after the other, or set by a page's script just after the element is parsed, fetch once. */
  #queueFetch() {
    if (this.#fetching) return;
    this.#fetching = true;
    queueMicrotask(() => {
      this.#fetching = false;
      if (this.isConnected) this.#fetchSources();
    });
  }

  async #fetchSources() {
    const chosen = this.#chosen();
    const compiled = chosen.getAttribute('compiled');
    if (compiled) {
      const key = JSON.stringify(['compiled', compiled]);
      if (key === this.#fetched) return;
      this.#fetched = key;
      if (this.#clock) this.#clock.id = this.#clockName();
      try {
        const response = await fetch(new URL(compiled, document.baseURI));
        if (!response.ok) throw new Error(`${compiled}: ${response.status}`);
        const bytes = new Uint8Array(await response.arrayBuffer());
        await ready();
        if (key !== this.#fetched) return;
        this.loadCompiled(bytes);
      } catch (error) {
        this.#fail(error);
      }
      return;
    }
    const src = chosen.getAttribute('src') ?? this.getAttribute('src');
    if (!src) return;
    const sheets = (chosen.getAttribute('sheets') ?? this.getAttribute('sheets') ?? '').split(/\s+/).filter(Boolean);
    const key = JSON.stringify([src, sheets]);
    if (key === this.#fetched) return;
    this.#fetched = key;
    if (this.#clock) this.#clock.id = this.#clockName();
    try {
      const names = [src, ...sheets];
      const texts = await Promise.all(names.map(fetchText));
      const used = await gather(names, texts);
      if (key !== this.#fetched) return;
      const [doc, ...sheetTexts] = texts;
      this.load(doc, sheetTexts, used);
    } catch (error) {
      this.#fail(error);
    }
  }

  /** Compiles a figure from its `.pen` text and its sheets' texts, in order. A compile error is
   * thrown, with `line`, `col` and `sheet` saying where, and leaves the last figure on screen.
   * Before the GPU starts it's only checked, and shapes the element; it's drawn once the GPU is up.
   * Params held before keep their values if the new figure has them.
   *
   * `used` is what `gather` read: the files these texts `use`. Without it, a `use "file"` is an
   * error. An error in a used file says so in `file`, its name. */
  load(doc, sheets = [], used = {}) {
    const sources = { doc, sheets, names: used.names ?? [], files: used.files ?? {} };
    if (this.#player) {
      this.#compile(sources);
    } else if (wasm) {
      this.#shape(JSON.parse(figure_size(doc, sheets, sources.names, sources.files)));
      this.#sources = sources;
    } else {
      this.#sources = sources;
    }
  }

  /** Loads a figure a build compiled (`pencil build`, `pencil compile`), from its bytes. One that
   * another build of Pencil wrote is an error that says to compile it again, and leaves the last
   * figure, or the poster, on screen. */
  loadCompiled(bytes) {
    const sources = { compiled: bytes };
    if (this.#player) {
      this.#compile(sources);
    } else if (wasm) {
      this.#shape(JSON.parse(figure_size_compiled(bytes)));
      this.#sources = sources;
    } else {
      this.#sources = sources;
    }
  }

  #compile(sources) {
    if (sources.compiled) this.#player.load_compiled(sources.compiled);
    else this.#player.load(sources.doc, sources.sheets, sources.names, sources.files);
    this.#sources = sources;
    const info = JSON.parse(this.#player.info());
    this.#info = info;
    this.#driven = new Set(info.timeDriven);
    this.#shape(info.size);
    for (const [name, value] of this.#held) {
      if (info.params.some((p) => p.id === name)) this.#player.set_param(name, value);
      else this.#held.delete(name);
    }
    this.#retime();
    this.#fresh = true;
    this.#invalidate();
    this.#sync();
    this.dispatchEvent(new Event('load'));
  }

  /** Shapes the figure to its design size, so nothing moves as it loads. */
  #shape(size) {
    this.#size = size;
    this.#canvas.style.setProperty('--pencil-aspect', `${size[0]} / ${size[1]}`);
    this.#follow();
  }

  #clockName() {
    const src = this.getAttribute('src') ?? this.getAttribute('compiled');
    return `pencil ${this.id || (src ? src.split('/').pop() : 'figure')}`;
  }

  #fail(error) {
    this.dispatchEvent(new CustomEvent('error', { detail: { error, time: this.#time, fresh: this.#fresh } }));
  }

  // ---- Time ----

  /** The figure's loop length in seconds: its `duration`, or 10 s for one that says none. */
  get duration() {
    return this.#info?.duration ?? LOOP;
  }

  /** The time last drawn, in seconds. Setting it seeks, and a `beat` that follows time follows
   * it again. */
  get time() {
    return this.#time;
  }

  set time(seconds) {
    this.#seek(seconds);
    if (this.#driven.has('beat')) this.#release('beat');
    this.#invalidate();
  }

  /** Whether the reader (or the page) has paused it. A figure that's offscreen or never reads
   * time doesn't run its clock either way. */
  get paused() {
    return this.#paused ?? true;
  }

  /** Plays from where it is. Each param time drives is handed back to time, carrying on from
   * where the reader left the last one they moved. */
  play() {
    const moved = [...this.#held.keys()].filter((name) => this.#driven.has(name)).pop();
    if (moved && this.#player) {
      try {
        this.#seek(this.#player.time_for(moved, this.#held.get(moved)));
      } catch {}
    }
    for (const name of [...this.#held.keys()]) if (this.#driven.has(name)) this.#release(name);
    const was = this.#paused;
    this.#paused = false;
    this.#sync();
    this.#invalidate();
    if (was !== false) this.dispatchEvent(new Event('play'));
  }

  pause() {
    const was = this.#paused;
    this.#paused = true;
    this.#sync();
    if (was !== true) this.dispatchEvent(new Event('pause'));
  }

  get playbackRate() {
    return this.#rate;
  }

  set playbackRate(rate) {
    this.#rate = rate;
    if (this.#clock) this.#clock.playbackRate = rate;
  }

  /** The `Animation` that keeps the figure's time. Read it, but play, pause and seek through
   * the element, which knows when to draw. */
  get clock() {
    return this.#clock;
  }

  #clockTime() {
    return (this.#clock?.effect.getComputedTiming().progress ?? 0) * this.duration;
  }

  #seek(seconds) {
    this.#time = Math.min(Math.max(0, seconds), this.duration);
    if (this.#clock) this.#clock.currentTime = this.#time * 1000;
  }

  /** Follows a new figure's duration, keeping the time where it is. */
  #retime() {
    if (!this.#clock) return;
    const time = this.#clock.playState === 'running' ? this.#clockTime() : this.#time;
    this.#clock.effect.updateTiming({ duration: this.duration * 1000 });
    this.#seek(time > this.duration ? time % this.duration : time);
  }

  /** Runs the clock when the figure is playing, near the viewport and reads time, and stops it
   * otherwise. */
  #sync() {
    const clock = this.#clock;
    if (!clock) return;
    const run = this.#paused === false && this.#visible && !!this.#player && !!this.#info?.usesTime;
    const running = clock.playState === 'running';
    if (run && !running) {
      clock.currentTime = this.#time * 1000;
      clock.play();
      this.#schedule();
    } else if (!run && running) {
      this.#time = this.#clockTime();
      clock.pause();
    }
  }

  // ---- Beats ----

  /** The figure's `beat` this frame. Setting it seeks time when the beat follows time, so the
   * two can't come apart, and otherwise holds the param. */
  get beat() {
    return this.value('beat');
  }

  set beat(target) {
    const param = this.#info?.params.find((p) => p.id === 'beat');
    if (!param) return;
    target = Math.min(Math.max(param.min, target), param.max);
    if (!this.#driven.has('beat')) {
      this.hold('beat', target);
      return;
    }
    // A beat that follows time is taken to only count up, so the first moment it reaches the
    // target is found by halving. Sampling, as `time_for` does, lands just short of it.
    let [lo, hi] = [0, this.duration];
    for (let k = 0; k < 40; k++) {
      const mid = (lo + hi) / 2;
      if (this.#valueAt('beat', mid) >= target - 1e-9) hi = mid;
      else lo = mid;
    }
    this.time = hi;
  }

  /** Steps a beat forward or back, pausing, as a reader stepping through does. */
  step(by) {
    const beat = this.beat;
    if (!Number.isFinite(beat)) return;
    this.pause();
    this.beat = by > 0 ? Math.floor(beat + 1e-6) + by : Math.ceil(beat - 1e-6) + by;
  }

  // ---- Params ----

  /** A binding's value at a time (now, by default), or NaN if it doesn't exist then. */
  value(name, time = this.#time) {
    return this.#valueAt(name, time);
  }

  #valueAt(name, time) {
    if (!this.#player || !this.#info) return NaN;
    try {
      return this.#player.value(name, time);
    } catch {
      return NaN;
    }
  }

  /**
   * A param's range this frame, as `[min, max]`. An end that follows other params, as the `q` of
   * `0..q` does, moves with them, and the figure keeps the param inside it. `info.params` gives
   * the widest each range gets, as `min` and `max`, and the params that move it, as `follows`,
   * so a control only has to ask again for a param that follows something.
   */
  range(name, time = this.#time) {
    const param = this.#info?.params.find((p) => p.id === name);
    if (!param) return [NaN, NaN];
    if (!param.follows?.length || !this.#player) return [param.min, param.max];
    try {
      return Array.from(this.#player.range(name, time));
    } catch {
      return [param.min, param.max];
    }
  }

  /** Takes a param over from time, at this value, until it's released or the figure plays. */
  hold(name, value) {
    this.#held.delete(name);
    this.#held.set(name, value);
    this.#player?.set_param(name, value);
    this.#invalidate();
  }

  /** Hands a param back to its binding, or every param with no name. */
  release(name) {
    for (const each of name === undefined ? [...this.#held.keys()] : [name]) this.#release(each);
    this.#invalidate();
  }

  #release(name) {
    this.#held.delete(name);
    this.#player?.release_param(name);
  }

  /** The params a reader or the page holds, by name. */
  get held() {
    return new Map(this.#held);
  }

  /** Every param's value this frame, by name. Setting holds each one given. */
  get params() {
    return Object.fromEntries((this.#info?.params ?? []).map((p) => [p.id, this.value(p.id)]));
  }

  set params(values) {
    for (const [name, value] of Object.entries(values)) this.hold(name, value);
  }

  /** The figure's design size, `[width, height]` in design pixels, from its sheets. */
  get designSize() {
    return this.#size;
  }

  /** What the figure says of itself: its size, duration, beats, params, which bindings time
   * drives (`timeDriven`), and whether anything reads time (`usesTime`). */
  get info() {
    return this.#info;
  }

  /** The named colors the figure's sheets define, by name, so the page around it can match:
   * `{ paper_color: 'rgb(…)', … }`. Empty until the figure has loaded. */
  get colors() {
    return this.#info?.colors ?? {};
  }

  /** The WASM host, for measuring and debugging. */
  get player() {
    return this.#player;
  }

  get backend() {
    return this.#player?.backend ?? null;
  }

  // ---- Handles: a drag sets its param through the figure's own inverse ----

  #device(e) {
    const box = this.#canvas.getBoundingClientRect();
    return [(e.clientX - box.left) * (this.#canvas.width / box.width), (e.clientY - box.top) * (this.#canvas.height / box.height)];
  }

  #pointerDown = (e) => {
    if (!this.#player) return;
    const [x, y] = this.#device(e);
    try {
      if (!this.#player.grab(this.#time, x, y, REACH * (window.devicePixelRatio || 1))) return;
      this.pause();
      this.#canvas.setPointerCapture(e.pointerId);
      this.#canvas.style.cursor = 'grabbing';
      this.#drag(x, y);
    } catch {}
  };

  #pointerMove = (e) => {
    if (!this.#player) return;
    const [x, y] = this.#device(e);
    try {
      if (!this.#canvas.hasPointerCapture(e.pointerId)) {
        this.#canvas.style.cursor = this.#player.grab(this.#time, x, y, REACH * (window.devicePixelRatio || 1)) ? 'grab' : '';
        this.#player.release();
        return;
      }
      this.#drag(x, y);
    } catch {}
  };

  #pointerEnd = (e) => {
    if (this.#canvas.hasPointerCapture(e.pointerId)) this.#canvas.releasePointerCapture(e.pointerId);
    this.#player?.release();
    this.#canvas.style.cursor = '';
  };

  #drag(x, y) {
    const name = this.#player.drag(this.#time, x, y);
    if (!name) return;
    const value = this.#player.value(name, this.#time);
    this.#held.delete(name);
    this.#held.set(name, value);
    this.#invalidate();
    this.dispatchEvent(new CustomEvent('input', { bubbles: true, detail: { name, value } }));
  }

  // ---- Drawing ----

  #invalidate() {
    this.#dirty = true;
    this.#schedule();
  }

  #schedule() {
    if (this.#raf || !this.#player || !this.isConnected) return;
    this.#raf = requestAnimationFrame(this.#frame);
  }

  /** One frame: read the clock if it runs, draw if anything changed, and move label springs.
   * Another frame is asked for only while the clock runs or a label is still settling. */
  #frame = (now) => {
    this.#raf = 0;
    // Springs coming out of a rest take one frame's step, not the whole time since the last.
    if (this.#idle) this.#layer.last = now - 1000 / 60;
    const running = this.#clock?.playState === 'running';
    if (running) {
      this.#time = this.#clockTime();
      this.#dirty = true;
    }
    if (this.#dirty) this.#paint();
    const settling = this.#layer.tick(now);
    this.#idle = !running && !settling;
    if (!this.#idle) this.#schedule();
  };

  #paint() {
    this.#dirty = false;
    if (!this.#player || !this.#info) return;
    const fresh = this.#fresh;
    this.#fresh = false;
    // Zooming or moving to another screen changes it, so it's read each frame.
    const dpr = window.devicePixelRatio || 1;
    this.#layer.dpr = dpr;
    try {
      this.#layer.update(JSON.parse(this.#player.draw(this.#time, dpr)));
    } catch (error) {
      this.pause();
      this.dispatchEvent(new CustomEvent('error', { detail: { error, time: this.#time, fresh } }));
      return;
    }
    if (!this.#drawn) {
      this.#drawn = true;
      this.#wait();
    }
    this.dispatchEvent(new CustomEvent('draw', { detail: { time: this.#time, fresh } }));
  }

  /** An `em` is the page's text size where the figure is, so its labels match the text around
   * it at any size it's drawn. */
  #setEm() {
    const em = parseFloat(getComputedStyle(this.#canvas).fontSize);
    const changed = em !== this.#emPx;
    this.#emPx = em;
    this.#player?.set_em(em);
    return changed;
  }

  /** Keeps the canvas's pixels matched to its laid-out size. A new size clears the canvas, and
   * this runs after the frame's drawing and before it's shown, so it draws at once. */
  #follow() {
    if (!this.#player) {
      match(this.#canvas);
      return;
    }
    const [width, height] = [this.#canvas.width, this.#canvas.height];
    this.#player.resize(...match(this.#canvas));
    const em = this.#setEm();
    if (em || this.#canvas.width !== width || this.#canvas.height !== height) {
      this.#paint();
      this.dispatchEvent(new CustomEvent('resize', { detail: { em: this.#emPx } }));
    }
  }
}

customElements.define('pencil-figure', PencilFigure);
