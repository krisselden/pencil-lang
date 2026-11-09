// A figure's canvas takes its shape from the figure and its width from the page, and keeps its
// pixels matched to that width, so it draws sharp at any size. The shape is the design size the
// sheets give (`figure_size`). A page built ahead of time can write the same `aspect-ratio` into
// its markup, so nothing moves while the figure loads.

/** Shapes the canvas to a `[width, height]` and sizes its pixels to match. */
export function fit(canvas, [width, height]) {
  canvas.style.aspectRatio = `${width} / ${height}`;
  return match(canvas);
}

/** Sizes the canvas's pixels to its laid-out size, in device pixels. Setting a size clears the
 * canvas, even to the size it has, so it's only set when it changes. */
export function match(canvas) {
  const dpr = window.devicePixelRatio || 1;
  const width = Math.max(1, Math.round(canvas.clientWidth * dpr));
  const height = Math.max(1, Math.round(canvas.clientHeight * dpr));
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
  return [width, height];
}

/** Gives the player the page's text size, and keeps its pixels matched as the page lays the
 * canvas out again. `draw` has to draw at once, not on the next frame: the new size has cleared
 * the canvas, and this runs after the frame's own drawing and before it's shown, so waiting
 * would show it blank. */
export function follow(canvas, player, draw) {
  // An `em` is the page's text size where the figure is, so its labels match the text around it
  // at any size the figure is drawn.
  const em = () => player.set_em(parseFloat(getComputedStyle(canvas).fontSize));
  em();
  draw();
  new ResizeObserver(() => {
    const [width, height] = [canvas.width, canvas.height];
    player.resize(...match(canvas));
    em();
    if (canvas.width !== width || canvas.height !== height) draw();
  }).observe(canvas);
}
