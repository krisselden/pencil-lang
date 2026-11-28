// Holds a page to what's visible on a touch screen, so it never scrolls while a keyboard comes and
// goes. The playground, the site's page that frames it, and the keyboard lab all use it.
//
// iOS Safari never shrinks the layout viewport for its keyboard, only the visual one, and it
// scrolls the page to show the caret even when nothing in the page can scroll
// (lab/keyboard/README.md). So the element given is sized to the visual viewport, through
// `--app-h`, and moved to where that sits, through `--app-top`, and a page that scrolled is put
// back. Chrome on Android, asked for `interactive-widget=resizes-content`, shrinks both viewports,
// and this changes nothing more there. Inside a frame the visual viewport is the frame's, which the
// framing page sizes with this same function.
//
// The styles do the rest: the element is `position: fixed` with those two properties, and the
// page has `overflow: hidden`.

/** Whether the main pointer is a finger: a phone or a tablet without a mouse. It can change, as
 * when a mouse is paired with a tablet, and a page follows its `change`. */
export const touch = matchMedia('(pointer: coarse)');

/** Whether a touch on `el` can move something: a scroller with more in it than it shows, or a
 * control that takes a drag. */
function moves(el) {
  for (let at = el; at && at !== document.documentElement; at = at.parentElement) {
    if (at.matches('input, select, textarea, pencil-figure')) return true;
    const css = getComputedStyle(at);
    const y = /auto|scroll/.test(css.overflowY) && at.scrollHeight > at.clientHeight + 1;
    const x = /auto|scroll/.test(css.overflowX) && at.scrollWidth > at.clientWidth + 1;
    if (x || y) return true;
  }
  return false;
}

/** A pinch has zoomed the page in. Its visual viewport is then a window onto the page, and the
 * reader pans it with a finger, so the hold lets go until it's back to its size. */
const zoomed = () => (window.visualViewport?.scale ?? 1) > 1.01;

/**
 * Sizes `el` to the visual viewport as it changes. One finger can then only move what scrolls,
 * since anything else would drag the page, or pan the visual viewport into what a keyboard pushed
 * up. Two fingers still zoom, and three move nothing.
 *
 * - `pin: false` leaves a scrolled page where it is, to try without.
 * - `short`: below this many pixels of height, `el` has `data-short`, which a page's styles can
 *   squeeze for: a phone held sideways, or one with the system keyboard up. A media query can't
 *   say that, since Safari's keyboard leaves the layout viewport as it was.
 *
 * Returns `set(on)`, which lets go of the page and takes hold again, for a page that follows
 * `touch`.
 */
export function holdToViewport(el, { pin = true, short = 0 } = {}) {
  const vv = window.visualViewport;
  let on = true;
  const fit = () => {
    if (!on || zoomed()) return;
    if (pin && (scrollX || scrollY)) scrollTo(0, 0);
    const height = Math.round(vv ? vv.height : innerHeight);
    el.style.setProperty('--app-h', `${height}px`);
    el.style.setProperty('--app-top', `${Math.round(vv ? vv.offsetTop : 0)}px`);
    el.toggleAttribute('data-short', height < short);
  };
  vv?.addEventListener('resize', fit);
  vv?.addEventListener('scroll', fit);
  addEventListener('resize', fit);
  addEventListener('scroll', fit, { passive: true });
  // Three fingers are iOS's undo and redo, a swipe of them scrolled the editor or the page as well,
  // so they move nothing here. Two are a pinch, and still zoom. Whether iOS lets a page keep its
  // own gesture from scrolling is for a device to say (docs/goals.md, 2026-11-20).
  document.addEventListener('touchmove', (e) => {
    if (!on) return;
    if (e.touches.length >= 3 || (e.touches.length === 1 && !zoomed() && !moves(e.target))) e.preventDefault();
  }, { passive: false });
  fit();
  return (now) => {
    on = now;
    if (on) return fit();
    for (const p of ['--app-h', '--app-top']) el.style.removeProperty(p);
    el.removeAttribute('data-short');
  };
}
