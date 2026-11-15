// The demos: one entry per corpus figure, and the explainer. Paths are from the repo root, which
// is where the site serves them too. Each look is the sheets its page in `web/` (or, for a figure
// with no page, the playground) loads, in order, and `playground` is that look's id in
// `web/playground.js`, for `?figure=`.

export const REPO = 'https://github.com/krisselden/pencil-lang';
export const TREE = `${REPO}/tree/main`;
export const BLOB = `${REPO}/blob/main`;

const THEME = 'corpus/theme.style';
const DARK = 'corpus/dark.style';

export interface Look {
  /** What the look is called, when an entry has more than one. */
  name?: string;
  pen: string;
  sheets: string[];
  /** Added after `sheets` when the reader's theme is dark, as the figure's page does. */
  dark?: string[];
  /** Added before `dark` on a narrow screen, as the lattice's page does. */
  narrow?: string[];
  /** Its id in the playground's menu. */
  playground: string;
}

export interface Demo {
  id: string;
  number: string;
  title: string;
  /** A sentence or two on what it shows, from its page in `web/` or the comment atop its `.pen`.
   * Backticks mark code. */
  about: string;
  /** What a screen reader says for the figure. */
  label: string;
  /** The folder of its sources, for the GitHub link. */
  folder: string;
  looks: Look[];
  /** Params held as the figure's own page starts it, as the element's `params` attribute. */
  params?: string;
  paused?: boolean;
  /** What a choice between its looks is called, when they're not styles. */
  choose?: string;
  /** The time its page starts it at, in seconds. */
  start?: number;
}

export const DEMOS: Demo[] = [
  {
    id: 'circle',
    number: '001',
    title: 'A breathing circle',
    about:
      'The smallest figure that exercises the whole path: an implicit curve whose radius follows time, colored on each side of its signed distance. Its second sheet bands both sides by that distance, and not a line of the figure changes.',
    label: 'A circle that breathes, its radius swinging a quarter either side of 1.',
    folder: 'corpus/001-circle',
    looks: [
      { name: 'circle.style', pen: 'corpus/001-circle/circle.pen', sheets: ['corpus/001-circle/circle.style'], playground: 'circle' },
      { name: 'bands.style', pen: 'corpus/001-circle/circle.pen', sheets: ['corpus/001-circle/bands.style'], playground: 'circle-bands' },
    ],
  },
  {
    id: 'strokes',
    number: '002',
    title: 'Strokes',
    about:
      'Two figures for comparing traced strokes with SVG: a fixed circle filled on both sides, and an open wave whose width is an expression of arc length, which an SVG stroke can’t do.',
    label: 'A ring filled warm inside and cool outside, or a sine wave whose stroke widens along its length.',
    folder: 'corpus/002-strokes',
    choose: 'Figure',
    looks: [
      { name: 'ring', pen: 'corpus/002-strokes/ring.pen', sheets: ['corpus/002-strokes/filled.style'], playground: 'ring' },
      { name: 'wave', pen: 'corpus/002-strokes/wave.pen', sheets: ['corpus/002-strokes/taper.style'], playground: 'wave' },
    ],
  },
  {
    id: 'unroll',
    number: '003',
    title: 'Unwinding a string from a circle',
    about:
      'The end of the string traces an involute, and the triangle it makes with the center always has the area of the sector swept so far. The night sheet restyles the same document.',
    label:
      'A string unwinding from a circle. Its end traces an involute, and the triangle it makes with the center always has the area of the sector swept so far.',
    folder: 'corpus/003-unroll',
    looks: [
      { name: 'blog.style', pen: 'corpus/003-unroll/unroll.pen', sheets: ['corpus/003-unroll/blog.style'], playground: 'unroll' },
      { name: 'night.style', pen: 'corpus/003-unroll/unroll.pen', sheets: ['corpus/003-unroll/night.style'], playground: 'unroll-night' },
    ],
  },
  {
    id: 'bounds',
    number: '004',
    title: 'The two boundary flowers, by ruler and compass',
    about:
      'The two boundary cases of the Doyle spiral, built step by step around one circle at 1. Step through the beats, or drag P.',
    label:
      'A ruler-and-compass construction of the two boundary Doyle flowers around the circle at 1, one step at a time.',
    folder: 'corpus/004-bounds',
    looks: [{ pen: 'corpus/004-bounds/bounds.pen', sheets: ['corpus/004-bounds/blog.style'], playground: 'bounds' }],
    paused: true,
  },
  {
    id: 'tangents',
    number: '005',
    title: 'A point and a circle',
    about:
      'With P outside the circle, the two tangents from P. With P inside, the chord through P at right angles to OP. Drag P.',
    label:
      'A circle and a point P. With P outside the circle, the two tangents from P are drawn. With P inside, the chord through P at right angles to OP is drawn.',
    folder: 'corpus/005-tangents',
    looks: [{ pen: 'corpus/005-tangents/tangents.pen', sheets: [THEME, 'corpus/005-tangents/plain.style'], playground: 'tangents' }],
    params: 'd: 3',
    paused: true,
  },
  {
    id: 'square',
    number: '006',
    title: 'Squaring z = a + bi',
    about:
      'F.O.I.L. with arrows: a copy of z scaled by a, another scaled by b and turned by i, and tip to tail they make z², at twice the angle and the length squared.',
    label:
      'z = a + bi is built from a and bi. A copy of z is scaled by a, and another by b and turned 90 degrees by i. Tip to tail they make z squared, at twice the angle and the length squared.',
    folder: 'corpus/006-square',
    looks: [{ pen: 'corpus/006-square/square.pen', sheets: [THEME, 'corpus/006-square/plain.style'], playground: 'square' }],
  },
  {
    id: 'lattice',
    number: '007',
    title: 'One lattice, through the maps that make a Doyle spiral',
    about:
      'Four linked panels of one lattice triangle: the lattice, its image in log space, after exp as a spiral of circles, and a close-up near 1.',
    label:
      'Four linked panels of a lattice triangle: the lattice itself, after the map to log space, after exp as a spiral of circles, and a close-up of the spiral near 1.',
    folder: 'corpus/007-lattice',
    looks: [
      {
        pen: 'corpus/007-lattice/lattice.pen',
        sheets: [THEME, 'corpus/007-lattice/plain.style'],
        narrow: ['corpus/007-lattice/narrow.style'],
        dark: [DARK],
        playground: 'lattice',
      },
    ],
  },
  {
    id: 'hearts',
    number: '008',
    title: 'Bipolar hearts',
    about: 'Two hearts in bipolar coordinates around the foci at ±1, with the coordinate circles through a moving point on each.',
    label:
      'Two hearts in bipolar coordinates around the foci at plus and minus 1, with the coordinate circles through a moving point on each.',
    folder: 'corpus/008-hearts',
    looks: [{ pen: 'corpus/008-hearts/hearts.pen', sheets: [THEME, 'corpus/008-hearts/plain.style'], dark: [DARK], playground: 'hearts' }],
  },
  {
    id: 'squaring',
    number: '009',
    title: 'The squaring map',
    about:
      'A circle orthogonal to the unit circle, and a grid of lines inside it, raised to a power from 1 to 2. The circle becomes a limaçon, and the grid stays square at every crossing.',
    label:
      'A circle through n and 1 over n with a grid inside it, raised to a power that moves from 1 to 2, so the circle morphs into a limaçon.',
    folder: 'corpus/009-squaring',
    looks: [{ pen: 'corpus/009-squaring/squaring.pen', sheets: [THEME, 'corpus/009-squaring/plain.style'], playground: 'squaring' }],
  },
  {
    id: 'mobius',
    number: '010',
    title: 'Coxeter’s loxodromic sequence, through a Möbius map',
    about:
      'Circles at the powers of w, each touching its neighbors, and their image under a Möbius map, where they still touch. When the pole lands on a circle, that circle is a line for a moment.',
    label:
      'Circles at the powers of w, each touching its neighbors, and their image under a Möbius map, where they still touch.',
    folder: 'corpus/010-mobius',
    looks: [{ pen: 'corpus/010-mobius/mobius.pen', sheets: [THEME, 'corpus/010-mobius/plain.style'], dark: [DARK], playground: 'mobius' }],
    paused: true,
    start: 3.5,
  },
  {
    id: 'circumcircle',
    number: '011',
    title: 'A circumcircle, going through a line',
    about:
      'Warm is its negative side. When the corners pass, the circle goes through a line and comes back the other way round, and that side doesn’t change.',
    label:
      'A triangle and the circle through its corners, warm on one side and cool on the other. When the triangle turns over, the circle straightens into a line and comes back the other way round.',
    folder: 'corpus/011-circumcircle',
    looks: [{ pen: 'corpus/011-circumcircle/circumcircle.pen', sheets: ['corpus/011-circumcircle/plain.style'], playground: 'circumcircle' }],
  },
  {
    id: 'limacon',
    number: '012',
    title: 'A limaçon, from both sides of its circles',
    about:
      'Every circle passes through one point, and each is warm inside and cool outside. They add up, and tanh keeps them in range.',
    label:
      'Twenty-four circles through one point, whose envelope is a limaçon, each lit warm on its disk and cool outside it, adding up.',
    folder: 'corpus/012-limacon',
    looks: [{ pen: 'corpus/012-limacon/limacon.pen', sheets: ['corpus/012-limacon/plain.style'], playground: 'limacon' }],
  },
  {
    id: 'tube',
    number: '013',
    title: 'Rolling the strip into a tube',
    about:
      'The one-turn strip of the grid, with rows colored by arm, rolling up into a tube. Each row runs off the top and comes back in at the bottom as the next row of its arm, so an arm is one line wound around. Its points are in space, and the sheet fades the far side from `self.z`.',
    label:
      'The one-turn strip of the triangular grid, with rows colored by arm, rolling up into a tube whose seam joins each row to another row of the same arm.',
    folder: 'corpus/013-tube',
    looks: [{ pen: 'corpus/013-tube/tube.pen', sheets: [THEME, 'corpus/013-tube/plain.style'], dark: [DARK], playground: 'tube' }],
  },
  {
    id: 'scaled-copy',
    number: '014',
    title: 'A circle of the spiral, scaled and turned',
    about:
      'Multiplying by z grows a circle of a log spiral by |z| and turns it by arg z about the center, which lands it on the next circle. A copy of circle 0 is multiplied by zˢ as s runs from 0 to 1, and each circle’s tick shows the turn.',
    label:
      'A log spiral of touching circles. A copy of one circle grows and turns about the spiral’s center until it lands exactly on the next circle.',
    folder: 'corpus/014-scaled-copy',
    looks: [
      {
        pen: 'corpus/014-scaled-copy/scaled-copy.pen',
        sheets: [THEME, 'corpus/014-scaled-copy/plain.style'],
        dark: [DARK],
        playground: 'scaled-copy',
      },
    ],
  },
  {
    id: 'log-nearest',
    number: '015',
    title: 'Nearest isn’t nearest after log',
    about:
      'Dots at 1, e and e² on the line are evenly spaced after log, at 0, 1 and 2. Each line marks the dot nearest x by its own distance, and they disagree between e^(n+½) and (eⁿ + eⁿ⁺¹)/2, which is shaded on both. Drag x.',
    label:
      'Two number lines: dots at 1, e and e² on the line, and at 0, 1 and 2 after taking the log. A draggable x and its nearest dot on each, with the stretches where they disagree shaded.',
    folder: 'corpus/015-log-nearest',
    looks: [
      {
        pen: 'corpus/015-log-nearest/log-nearest.pen',
        sheets: [THEME, 'corpus/015-log-nearest/plain.style'],
        dark: [DARK],
        playground: 'log-nearest',
      },
    ],
    paused: true,
  },
  {
    id: 'explainer',
    number: 'Lab',
    title: 'Explainer: the limaçon from its central circle',
    about:
      'The first explainer, a draft: beats with captions that build the limaçon from one circle. It’s split into a file for each part of its story, which the element fetches as its `use` lines ask.',
    label: 'The limaçon built from its central circle, one captioned beat at a time, ending with the circles through 1 lighting up.',
    folder: 'lab/explainer',
    looks: [
      {
        pen: 'lab/explainer/limacon.pen',
        sheets: [THEME, DARK, 'lab/explainer/limacon.style', 'lab/explainer/caption.style'],
        playground: 'explainer',
      },
    ],
  },
];

/** A repo path as the site serves it. */
export const url = (path: string) => `/${path}`;
