// A figure's canvas takes its shape from the figure and its width from the page, and keeps its
// pixels matched to that width, so it draws sharp at any size. `<pencil-figure>` shapes it to the
// design size the sheets give and calls this as the page lays it out again.

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
