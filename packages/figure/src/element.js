// <pencil-figure>: a Pencil figure on a page. It owns the canvas, the labels over it, the GPU and
// the clock, so a page only says which figure and which sheets:
//
//   <pencil-figure src="unroll.pen" sheets="theme.style blog.style"></pencil-figure>
//
// Importing this module defines the element: `import '@pencil-lang/figure'`. The WASM it runs
// on is `./host.js`, which a page can call directly too.
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
// Where a page pins a theme over the reader's system, with `color-scheme` in its CSS, the element
// answers `prefers-color-scheme` in those `media` with the page's scheme (`scheme.js`).
//
// Attributes: `src` and `sheets` (space-separated URLs, in order), or `compiled`, `params`
// (`a: 0.5; beat: 2`, held as if a reader set them), `paused` to start paused, and `gl` to skip
// WebGPU. A page can also pick its sheets in script, by setting `sheets` as soon as the element
// is defined: changes made before that script finishes are fetched once.
// Its labels are an em of the text where it sits, and follow when a reader changes their text size.
// `reload()` fetches the sources again, for a page that watches them change.
// A handle is dragged with a mouse, a pen or a finger, or tabbed to and moved with the keys. A touch
// within a handle's reach drags it and doesn't scroll the page; anywhere else on the figure the
// page scrolls and zooms as it would over an image. Nothing in the figure is text to select, and a
// drag never selects the page's. The element does all this itself: a page needs no CSS for it.
// A browser can lose the GPU under a figure, after sleep or when its GPU process restarts. The
// figure then starts its GPU again, keeping its sources, time, held params and whether it's
// paused, and shows its poster meanwhile. It tries at once, after a second and after five, then
// says `error` (with `detail.gpu`), and tries once more each time the page is shown.
// Events: `load` when a figure is compiled, `draw` after each frame drawn, `resize` when its drawn
// size or its em changes (its `detail.em`, in CSS pixels), `input` when a reader
// drags a param, `play` and `pause`, `error` (its `detail.error`) when one can't load or draw, and
// `recover` when one that said `error` because it lost its GPU draws again.
import { Player, let_go, figure_size, figure_size_compiled, ready, isReady, gather, ahead } from './host.js';
import { LABEL_CSS, LabelLayer, fonts } from './labels.js';
import { match } from './fit.js';
import { follow, settle, unfollow } from './scheme.js';

// How long time runs before it loops, for a figure that declares no `duration`.
const LOOP = 10;
// How near a handle a pointer has to be to take it, in CSS pixels.
const REACH = 24;
// How far from the viewport the GPU starts, so a figure is drawn by the time it's scrolled to.
const NEAR = '200px';

// How long a figure has to draw after starting again before a loss counts as a new one.
const SETTLED = 30_000;
// When a figure that has lost its GPU tries again: at once, then after a moment for a GPU process
// that's restarting, then a little longer. After those it says `error`.
const RETRIES = [0, 1000, 5000];

const fetchText = async (url) => {
  const response = await fetch(new URL(url, document.baseURI));
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return response.text();
};

const CSS = `
/* Nothing in a figure is text to select, and a long press on it shows no callout: a handle is
   pressed and held to be dragged. WebKit doesn't carry the host's down, so each part says it. */
:host { display: block; }
:host, .stage *, ::slotted(*) { -webkit-user-select: none; user-select: none; -webkit-touch-callout: none; -webkit-tap-highlight-color: transparent; }
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
/* Something to focus over each handle: a reader on a keyboard tabs to it and moves it with the
   arrow keys. Its ring is two tones, so it shows on any paper. A pointer takes it within the
   handle's reach, the circle its ::before fills, and a touch there drags the handle and doesn't
   scroll the page. Anywhere else on the figure the touch goes to the canvas, which leaves the
   page to scroll and zoom as it would over an image. */
.handles { position: absolute; inset: 0; pointer-events: none; }
.handle { position: absolute; left: -14px; top: -14px; width: 28px; height: 28px; border-radius: 50%; outline: none; will-change: transform; pointer-events: auto; touch-action: none; }
.handle::before { content: ''; position: absolute; inset: ${14 - REACH}px; border-radius: 50%; clip-path: var(--reach, none); }
/* With no GPU, as while the poster shows, a handle can't be dragged, so it takes no pointer, and
   a touch on it scrolls the page. The keys still find it. */
.stage:not(.live) .handle { pointer-events: none; touch-action: auto; }
.handle:focus-visible { outline: 2px solid var(--pencil-focus, currentColor); outline-offset: 1px; box-shadow: 0 0 0 2px var(--pencil-focus-halo, Canvas); }
.handle[hidden] { display: none; }
.said { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
${LABEL_CSS}`;

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
  #handleLayer;
  #handles = []; // what a reader focuses for each handle, in `info.handles`' order
  #said; // what a key just did, for a screen reader
  #internals = null;
  #tabbable = false; // whether the element made itself a tab stop, for stepping beats
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
  // Starting again after the GPU is lost: how many times since the figure last settled, when it
  // last did, whether it has drawn since, whether it's waiting for the page to be seen again,
  // and that the `load` a start sends isn't news.
  #tries = 0;
  #restarted = 0;
  #healthy = false;
  #awaiting = false;
  #quiet = false;
  #retry = 0; // the timer for the next try
  #reported = false; // whether it said `error` since it last drew
  #playerId = -1; // the player's number and backend, which a trapped player can't say
  #backend = null;
  // A WebGL2 canvas whose context is being brought back, for the next player, and a start that
  // waits for it.
  #restoring = false;
  #restoreTimer = 0;
  #startPending = false;
  #lose = null; // the canvas's `WEBGL_lose_context`, which a lost context doesn't give

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
    this.#handleLayer = document.createElement('div');
    this.#handleLayer.className = 'handles';
    this.#said = document.createElement('div');
    this.#said.className = 'said';
    this.#said.setAttribute('aria-live', 'polite');
    this.#stage.append(this.#handleLayer, this.#said);
    // A press on the canvas or on a handle's reach comes here, and a grab captures the pointer.
    const stage = this.#stage;
    stage.addEventListener('pointerdown', this.#pointerDown);
    stage.addEventListener('pointermove', this.#pointerMove);
    stage.addEventListener('pointerup', this.#pointerEnd);
    stage.addEventListener('pointercancel', this.#pointerEnd);
    // A grab focuses its handle, and the press mustn't take the focus away again, or start a
    // selection that follows the pointer out over the page. Nor does a long press open a menu.
    const grabbing = (e) => {
      if (this.#player?.grabbed() >= 0) e.preventDefault();
    };
    stage.addEventListener('mousedown', grabbing);
    stage.addEventListener('contextmenu', grabbing);
    this.addEventListener('keydown', this.#key);
    try {
      this.#internals = this.attachInternals();
    } catch {}
  }

  // ---- The poster and the sources ----

  /** Whether the element has a poster: any child that isn't a `<source>`, or a script. */
  #hasPoster() {
    return [...this.children].some((child) => child.localName !== 'source' && child.localName !== 'script');
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
      // A `<source>` that has just arrived is answered too. What that changes comes back here
      // once, and then changes nothing.
      settle(this);
      this.#wait();
      this.#listen();
      this.#refetch();
    });
    this.#children.observe(this, { childList: true, attributes: true, subtree: true, attributeFilter: ['media', 'src', 'sheets', 'compiled'] });
    document.fonts?.addEventListener('loadingdone', this.#fontsChanged);
    document.addEventListener('visibilitychange', this.#shown);
    window.addEventListener('pageshow', this.#shown);
    follow(this);
    this.#wait();
    this.#listen();
    this.#refetch();
  }

  /** A font that arrives after text was measured without it, as the math font does the first time
   * a label needs a glyph from it: what was measured is forgotten, here and in the player. */
  #fontsChanged = () => {
    this.#layer.remeasure();
    this.#player?.set_measure(this.#layer.measure);
    this.#invalidate();
  };

  disconnectedCallback() {
    document.fonts?.removeEventListener('loadingdone', this.#fontsChanged);
    document.removeEventListener('visibilitychange', this.#shown);
    window.removeEventListener('pageshow', this.#shown);
    this.#seen?.disconnect();
    this.#resized?.disconnect();
    this.#children?.disconnect();
    unfollow(this);
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
      this.#hold(parseParams(this.getAttribute('params')));
    } else {
      this.#refetch();
    }
  }

  #makeCanvas() {
    const canvas = document.createElement('canvas');
    canvas.setAttribute('part', 'canvas');
    if (this.#size) canvas.style.setProperty('--pencil-aspect', `${this.#size[0]} / ${this.#size[1]}`);
    // WebGL says so when it loses its context, and drawing on carries on drawing nothing. The
    // context is kept, to be brought back, since a browser counts a canvas's against its cap
    // until the canvas is collected, which WebKit can leave for a long while.
    canvas.addEventListener('webglcontextlost', (e) => {
      e.preventDefault();
      if (canvas !== this.#canvas) return;
      if (this.#player) this.#recover(new Error('GPU context lost'));
      else if (this.#restoring) this.#restoreLater(canvas);
    });
    canvas.addEventListener('webglcontextrestored', () => {
      if (canvas !== this.#canvas || !this.#restoring) return;
      this.#restoring = false;
      clearTimeout(this.#restoreTimer);
      this.#startIfPending();
    });
    return canvas;
  }

  /** Starts the GPU, once, when the element is near the viewport. */
  async #start() {
    if (this.#player || this.#starting) return;
    // Its WebGL context is on its way back.
    if (this.#restoring) {
      this.#startPending = true;
      return;
    }
    this.#starting = true;
    const generation = this.#generation;
    try {
      // The text faces too, so labels are measured and first drawn in the fonts the poster has.
      await Promise.all([ready(), fonts]);
      if (generation !== this.#generation) return;
      match(this.#canvas);
      const player = await Player.create(this.#canvas, this.hasAttribute('gl'));
      if (generation !== this.#generation) {
        player.destroy();
        return;
      }
      this.#player = player;
      this.#playerId = player.id;
      this.#backend = player.backend;
      this.#lose = player.backend === 'Gl' ? (this.#canvas.getContext('webgl2')?.getExtension('WEBGL_lose_context') ?? null) : null;
      this.#stage.classList.add('live');
      // A browser loses a GPU device after sleep or when its GPU process restarts, and says so.
      player.on_lost(() => {
        if (this.#player === player) this.#recover(new Error('GPU device lost'));
      });
      player.set_measure(this.#layer.measure);
      this.#setEm();
      if (this.#sources) {
        try {
          this.#compile(this.#sources);
        } catch (error) {
          this.#fail(error);
        }
      }
      // Started, so nothing is waiting to try again, and the next `load` is news.
      this.#awaiting = false;
      this.#quiet = false;
      clearTimeout(this.#retry);
      this.#retry = 0;
    } catch (error) {
      // Starting again can fail while the browser's GPU is still coming back.
      if (this.#tries && generation === this.#generation) {
        this.#starting = false;
        this.#recover(error);
      } else this.#fail(error);
    } finally {
      if (generation === this.#generation) this.#starting = false;
    }
  }

  // ---- A lost GPU ----

  /** Whether a draw failed on the GPU's side, which a new player gets past: a trap inside the
   * WASM (`unreachable`), after which that player can't be used again ("recursive use of an
   * object"), a lost device or context, or no texture to draw into, as when a draw comes before
   * the word that the device is lost. A GPU error on a device that isn't lost is the figure's,
   * and is said at once, as any other. A trap can be the core's too: that one fails each time it's
   * tried, and is said once the tries are done. */
  #recoverable(error) {
    if (error instanceof WebAssembly.RuntimeError) return true;
    const message = String(error?.message ?? '');
    if (/recursive use of an object|^GPU (device lost|context lost|surface)/.test(message)) return true;
    if (!message.startsWith('GPU error:')) return false;
    try {
      return this.#player?.lost ?? false;
    } catch {
      return true;
    }
  }

  /**
   * The GPU went away under the figure, or a draw failed on its side: start the GPU again, keeping
   * the figure's sources, time, held params and whether it's paused, with its poster meanwhile.
   * It's tried at once, then after a second and after five, for a GPU process that's coming back,
   * and then it's an `error` (with `detail.gpu`), tried once more each time the page is shown. A
   * figure that draws again after that says `recover`. One that has drawn for a while since, or
   * that the reader has come back to, has its tries back.
   */
  #recover(error) {
    clearTimeout(this.#retry);
    this.#retry = 0;
    if (this.#healthy && performance.now() - this.#restarted > SETTLED) this.#tries = 0;
    this.#tries += 1;
    this.#dropGpu();
    const wait = RETRIES[this.#tries - 1];
    if (wait === undefined) {
      this.#awaiting = true;
      this.#reported = true;
      this.dispatchEvent(new CustomEvent('error', { detail: { error, time: this.#time, fresh: false, gpu: true } }));
    } else if (wait === 0) {
      this.#restart();
    } else {
      this.#retry = setTimeout(() => {
        this.#retry = 0;
        this.#restart();
      }, wait);
    }
  }

  #restart() {
    this.#restarted = performance.now();
    this.#quiet = true;
    if (this.#visible) this.#start();
  }

  /** The page was seen again, or brought back from the back/forward cache. */
  #shown = () => {
    if (document.visibilityState !== 'visible') return;
    if (this.#awaiting && !this.#player) {
      // One more try, which says `error` again if it fails.
      this.#awaiting = false;
      this.#tries = RETRIES.length;
      this.#restart();
    } else if (this.#healthy) {
      this.#tries = 0;
    }
  };

  /** Lets go of a player, even one that trapped, which can't be borrowed again: its device is
   * let go of by its number. */
  #letGo(player) {
    try {
      player.destroy();
    } catch {
      let_go(this.#playerId);
    }
  }

  /** Lets go of the player, and nothing else, as `#teardown` does with the rest. The poster shows
   * again until the figure draws. A WebGL2 canvas keeps its context, lost and brought back for
   * the next player, and a WebGPU one gets a new canvas. */
  #dropGpu() {
    cancelAnimationFrame(this.#raf);
    this.#raf = 0;
    this.#idle = true;
    this.#generation += 1;
    this.#starting = false;
    this.#healthy = false;
    this.#stage.classList.remove('live');
    const player = this.#player;
    this.#player = null;
    // The clock stops where it is, and runs again once there's a figure to draw.
    this.#sync();
    if (player) {
      this.#letGo(player);
      if (this.#backend === 'Gl') this.#restore(this.#canvas);
      else this.#freshCanvas();
    }
    this.#drawn = false;
    this.#wait();
  }

  /** Loses a WebGL2 context, if it isn't already, and brings it back, for the next player. One the
   * browser lost and doesn't bring back within two seconds is given up on, for a new canvas. */
  #restore(canvas) {
    const gl = canvas.getContext('webgl2');
    if (!gl || !this.#lose) {
      this.#freshCanvas();
      return;
    }
    this.#restoring = true;
    // Its `webglcontextlost` brings it back; one already lost is brought back now.
    if (gl.isContextLost()) this.#restoreLater(canvas);
    else this.#lose.loseContext();
    clearTimeout(this.#restoreTimer);
    this.#restoreTimer = setTimeout(() => {
      if (!this.#restoring || canvas !== this.#canvas) return;
      this.#restoring = false;
      this.#freshCanvas();
      this.#startIfPending();
    }, 2000);
  }

  /** Asks for a lost context back, once the event that says it's lost is done: only a loss whose
   * event was cancelled can be restored. One the browser lost comes back by itself, if it can. */
  #restoreLater(canvas) {
    setTimeout(() => {
      if (this.#restoring && canvas === this.#canvas) this.#lose?.restoreContext();
    }, 0);
  }

  #startIfPending() {
    if (!this.#startPending) return;
    this.#startPending = false;
    if (this.#visible) this.#start();
  }

  /** A new canvas in place of the old one, which keeps its context for life. */
  #freshCanvas() {
    this.#lose = null;
    const canvas = this.#makeCanvas();
    this.#resized?.unobserve(this.#canvas);
    this.#canvas.replaceWith(canvas);
    this.#canvas = canvas;
    this.#resized?.observe(canvas);
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
    const started = this.#player || this.#starting || this.#restoring;
    this.#generation += 1;
    this.#starting = false;
    // Nothing waits to try again: a figure put back starts afresh.
    clearTimeout(this.#retry);
    this.#retry = 0;
    this.#awaiting = false;
    this.#quiet = false;
    this.#tries = 0;
    this.#reported = false;
    this.#restoring = false;
    this.#startPending = false;
    clearTimeout(this.#restoreTimer);
    this.#stage.classList.remove('live');
    if (this.#player) {
      this.#letGo(this.#player);
      this.#player = null;
      // A browser has only so many contexts live, and loses the oldest past that.
      if (this.#backend === 'Gl') this.#lose?.loseContext();
      else this.#canvas.getContext('webgpu')?.unconfigure();
      this.#lose = null;
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

  /** Fetches the sources again and loads them, as after a file changed. The figure on screen,
   * its time and its held params stay until the new one is compiled. */
  reload() {
    this.#fetched = null;
    this.#queueFetch();
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
        const url = new URL(compiled, document.baseURI);
        // The page may have started on it already (`ahead`, in host.js).
        let bytes = null;
        const early = await ahead(url.href);
        if (early) {
          try {
            bytes = new Uint8Array(await early.arrayBuffer());
          } catch {}
        }
        if (!bytes) {
          const response = await fetch(url);
          if (!response.ok) throw new Error(`${compiled}: ${response.status}`);
          bytes = new Uint8Array(await response.arrayBuffer());
        }
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
    } else if (isReady()) {
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
    } else if (isReady()) {
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
    this.#operable();
    this.#fresh = true;
    this.#invalidate();
    this.#sync();
    // A figure started again after losing its GPU is the same figure.
    if (this.#quiet) this.#quiet = false;
    else this.dispatchEvent(new Event('load'));
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

  /** The time last drawn, in seconds. Setting it seeks, and hands every param time drives back
   * to time, as a reader setting any param hands back the params whose bindings read it. */
  get time() {
    return this.#time;
  }

  set time(seconds) {
    this.#seek(seconds);
    for (const name of [...this.#held.keys()]) if (this.#driven.has(name)) this.#release(name);
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

  /** Takes a param over from its binding, at this value, as a reader setting it does, until it's
   * released, the figure plays, or a param its binding reads is set. */
  hold(name, value) {
    this.#hold([[name, value]]);
  }

  /** Holds params together: none of them hands another back. */
  #hold(pairs) {
    this.#reclaim(pairs.map(([name]) => name));
    for (const [name, value] of pairs) {
      this.#held.delete(name);
      this.#held.set(name, value);
      this.#player?.set_param(name, value);
    }
    this.#invalidate();
  }

  /**
   * A param with a binding has two drivers, its binding and the reader, and the reader's last
   * action picks. So when a reader sets params, every other held param whose binding reads one
   * of them (`reads`, in `info.params`, which the core works out) goes back to its binding, as
   * θ in Bounds goes back to closing the flower when q or the case is set. Before a figure is
   * loaded nothing is known of its bindings, so holds made then are all kept.
   */
  #reclaim(names) {
    const params = this.#info?.params;
    if (!params) return;
    for (const name of [...this.#held.keys()]) {
      if (names.includes(name)) continue;
      const reads = params.find((p) => p.id === name)?.reads ?? [];
      if (reads.some((read) => names.includes(read))) this.#release(name);
    }
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

  /** Every param's value this frame, by name. Setting holds each one given, together. */
  get params() {
    return Object.fromEntries((this.#info?.params ?? []).map((p) => [p.id, this.value(p.id)]));
  }

  set params(values) {
    this.#hold(Object.entries(values));
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
    // One pointer drags at a time, and only with the main button.
    if (!this.#player || !e.isPrimary || e.button !== 0) return;
    const [x, y] = this.#device(e);
    try {
      if (!this.#player.grab(this.#time, x, y, REACH * (window.devicePixelRatio || 1))) return;
      this.pause();
      this.#stage.setPointerCapture(e.pointerId);
      this.#stage.style.cursor = 'grabbing';
      // The keys carry on from where the pointer leaves it.
      this.#handles[this.#player.grabbed()]?.focus({ preventScroll: true });
      this.#drag(x, y);
    } catch {}
  };

  #pointerMove = (e) => {
    if (!this.#player) return;
    const [x, y] = this.#device(e);
    try {
      if (!this.#stage.hasPointerCapture(e.pointerId)) {
        this.#stage.style.cursor = this.#player.grab(this.#time, x, y, REACH * (window.devicePixelRatio || 1)) ? 'grab' : '';
        this.#player.release();
        return;
      }
      this.#drag(x, y);
    } catch {}
  };

  #pointerEnd = (e) => {
    if (!this.#stage.hasPointerCapture(e.pointerId)) return;
    this.#stage.releasePointerCapture(e.pointerId);
    this.#player?.release();
    this.#stage.style.cursor = '';
  };

  #drag(x, y) {
    this.#moved(this.#player.drag(this.#time, x, y));
  }

  /** A handle set this param in the player: hold it here too, draw, and tell the page. */
  #moved(name) {
    if (!name) return;
    const value = this.#player.value(name, this.#time);
    this.#reclaim([name]);
    this.#held.delete(name);
    this.#held.set(name, value);
    this.#invalidate();
    this.dispatchEvent(new CustomEvent('input', { bubbles: true, detail: { name, value } }));
  }

  // ---- Keys: every handle can be tabbed to and moved, and beats stepped ----

  /** Whether the figure has beats to step through. */
  #steps() {
    const info = this.#info;
    return !!info && info.beats.length > 0 && info.params.some((p) => p.id === 'beat');
  }

  /**
   * Makes what a reader can do with the figure reachable without a pointer, once it's known.
   * Each handle gets something to focus, named for its mark, that says its param's value as a
   * slider does. A figure with beats becomes a tab stop itself, unless the page has said where
   * it is in the tab order, and a figure with either is a group, so its name is read. One with
   * neither is an image.
   */
  #operable() {
    const handles = this.#info.handles ?? [];
    const key = JSON.stringify(handles);
    if (this.#handleLayer.dataset.for !== key) {
      this.#handleLayer.dataset.for = key;
      this.#handles = handles.map((handle, index) => {
        const el = document.createElement('div');
        el.className = 'handle';
        el.setAttribute('part', 'handle');
        el.setAttribute('role', 'slider');
        el.setAttribute('aria-label', handle.mark);
        el.tabIndex = 0;
        el.hidden = true;
        el.addEventListener('keydown', (e) => this.#handleKey(e, index));
        return el;
      });
      this.#handleLayer.replaceChildren(...this.#handles);
    }
    const steps = this.#steps();
    if (steps && !this.hasAttribute('tabindex')) {
      this.tabIndex = 0;
      this.#tabbable = true;
    } else if (!steps && this.#tabbable) {
      this.removeAttribute('tabindex');
      this.#tabbable = false;
    }
    // One a reader only looks at is an image, when the page has named it.
    const named = this.hasAttribute('aria-label') || this.hasAttribute('aria-labelledby');
    if (this.#internals) this.#internals.role = steps || handles.length ? 'group' : named ? 'img' : null;
  }

  /** Puts each handle's focus where its mark is, and says its value, after a frame is drawn. */
  #placeHandles() {
    if (!this.#handles.length) return;
    let places;
    try {
      places = this.#player.handle_places(this.#time);
    } catch {
      return;
    }
    const scale = this.#canvas.clientWidth / this.#canvas.width;
    const [width, height] = [this.#canvas.clientWidth, this.#canvas.clientHeight];
    const panels = this.#info.panels ?? [];
    this.#handles.forEach((el, i) => {
      const [x, y] = [places[2 * i] * scale, places[2 * i + 1] * scale];
      const there = Number.isFinite(x) && Number.isFinite(y);
      if (el.hidden === there) el.hidden = !there;
      if (!there) return;
      const moved = `translate(${x.toFixed(2)}px, ${y.toFixed(2)}px)`;
      if (el.style.transform !== moved) el.style.transform = moved;
      // A handle can't be grabbed outside its panel, so its reach stops there, and a touch past
      // it scrolls the page. The figure's own edge already stops it.
      if (panels.length > 1) {
        const [px, py, pw, ph] = panels[this.#info.handles[i].panel]?.rect ?? [0, 0, 1, 1];
        const cut = [py * height - (y - REACH), x + REACH - (px + pw) * width, y + REACH - (py + ph) * height, px * width - (x - REACH)];
        const reach = cut.some((c) => c > 0) ? `inset(${cut.map((c) => `${Math.max(0, c).toFixed(1)}px`).join(' ')})` : 'none';
        if (el.style.getPropertyValue('--reach') !== reach) el.style.setProperty('--reach', reach);
      }
      const param = this.#info.handles[i].param;
      const value = this.#valueAt(param, this.#time);
      const [min, max] = this.range(param);
      const said = [min, max, value].join(' ');
      if (el.dataset.said === said) return;
      el.dataset.said = said;
      el.setAttribute('aria-valuemin', String(min));
      el.setAttribute('aria-valuemax', String(max));
      el.setAttribute('aria-valuenow', String(value));
      el.setAttribute('aria-valuetext', `${param} = ${this.#format(param, value)}`);
    });
  }

  /** A param's value as text: as many decimals as its step has, or three. */
  #format(name, value) {
    const step = this.#info?.params.find((p) => p.id === name)?.step;
    if (step != null) return value.toFixed((String(step).split('.')[1] ?? '').length);
    return String(Number(value.toFixed(3)));
  }

  /**
   * A key on a focused handle. An arrow moves it that way on the page, a hundredth of the
   * figure's shorter side, or a tenth with Shift, through the handle's own inverse, as a drag
   * that far would. A param with steps goes to its next one that way. Home and End take the
   * param to the ends of its range, and Page Up and Page Down move it a tenth of the range.
   */
  #handleKey(e, index) {
    if (!this.#player || e.altKey || e.ctrlKey || e.metaKey) return;
    const way = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key];
    const param = this.#info.handles[index].param;
    try {
      if (way) {
        const { width, height } = this.#canvas;
        const step = Math.max(Math.min(width, height) / 100, 2 * (window.devicePixelRatio || 1)) * (e.shiftKey ? 10 : 1);
        this.pause();
        this.#moved(this.#player.nudge(this.#time, index, way[0] * step, way[1] * step));
      } else if (['Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) {
        const [min, max] = this.range(param);
        const now = this.#valueAt(param, this.#time);
        const to = { Home: min, End: max, PageUp: now + (max - min) / 10, PageDown: now - (max - min) / 10 }[e.key];
        this.pause();
        this.#player.set_param(param, to);
        this.#moved(param);
      } else {
        return;
      }
    } catch {
      return;
    }
    // The key is the handle's: the page doesn't scroll, and the figure doesn't step a beat.
    e.preventDefault();
    e.stopPropagation();
  }

  /** A key on the figure itself: ← and → step its beats, and Home and End go to the first and
   * last. What it came to is said for a screen reader, with the beat's caption. */
  #key = (e) => {
    if (e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey || !this.#player || !this.#steps()) return;
    const range = this.#info.params.find((p) => p.id === 'beat');
    if (e.key === 'ArrowRight') this.step(1);
    else if (e.key === 'ArrowLeft') this.step(-1);
    else if (e.key === 'Home' || e.key === 'End') {
      this.pause();
      this.beat = e.key === 'Home' ? range.min : range.max;
    } else return;
    e.preventDefault();
    const beats = this.#info.beats;
    const beat = this.beat;
    // Beats with lengths count the ones done, from 0. Ones without say which it's on.
    const timed = (this.#info.beatLengths?.length ?? 0) > 0;
    const n = Math.round(timed ? beat : beat + 1);
    const caption = beats[n - 1] ?? '';
    this.#said.textContent = n < 1 ? 'Start' : `Step ${n} of ${beats.length}${caption ? `: ${caption}` : ''}`;
  };

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
      if (this.#recoverable(error)) {
        this.#recover(error);
        return;
      }
      this.pause();
      this.dispatchEvent(new CustomEvent('error', { detail: { error, time: this.#time, fresh } }));
      return;
    }
    if (!this.#drawn) {
      this.#drawn = true;
      this.#wait();
    }
    this.#healthy = true;
    this.#placeHandles();
    if (this.#reported) {
      this.#reported = false;
      this.dispatchEvent(new Event('recover'));
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
    let em;
    try {
      this.#player.resize(...match(this.#canvas));
      em = this.#setEm();
    } catch (error) {
      if (!this.#recoverable(error)) throw error;
      this.#recover(error);
      return;
    }
    if (em || this.#canvas.width !== width || this.#canvas.height !== height) {
      this.#paint();
      this.dispatchEvent(new CustomEvent('resize', { detail: { em: this.#emPx } }));
    }
  }
}

customElements.define('pencil-figure', PencilFigure);
