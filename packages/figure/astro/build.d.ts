// Types for `build.js`, for a TypeScript site.

/** Where a site serves what a build made. */
export declare const PUBLIC: '_pencil';

/** A list of sheets, or their names with spaces between. */
export type Sheets = string | readonly string[];

/** What the integration worked out, which the pages it renders read from the environment. */
export interface PencilConfig {
  root: string;
  bin: string;
  cache: string;
  kept: string | null;
  wasm: string;
  dev: boolean;
  base: string;
  playground: string | null;
}

export declare function config(): PencilConfig;

/** A set of sheets that stands in for a figure's own under a media query. */
export interface Choice {
  media: string;
  sheets: string[];
}

/**
 * The sets of sheets a reader can get: the figure's own, and each that stands in for them under a
 * media query, the first that matches winning.
 */
export declare function alternates(options: {
  sheets?: Sheets;
  dark?: Sheets;
  narrow?: Sheets;
  narrowMedia?: string;
}): { sheets: string[]; others: Choice[] };

/** What a figure says of itself, as the element's `info` does. */
export interface FigureInfo {
  /** Its design size, `[width, height]`. */
  size: [number, number];
  panels: { id: string; title: string | null; formula: string | null; rect: [number, number, number, number] }[];
  duration: number | null;
  viewport: { x: [number, number]; y: [number, number]; width: number; height: number };
  beats: string[];
  beatLengths: number[];
  timeDriven: string[];
  usesTime: boolean;
  params: {
    id: string;
    min: number;
    max: number;
    step: number | null;
    follows: string[];
    choice: { of: string; options: string[] } | null;
  }[];
  handles: { panel: number; mark: string; param: string }[];
  poster: { time: number; params: Record<string, number> };
  /** The named colors its sheets define, as CSS. */
  colors: Record<string, string>;
}

export interface Built {
  /** In `astro dev`, the document's URL, which the element is given in place of `compiled`. */
  src?: string;
  /** In `astro dev`, the sheets' URLs. */
  sheets?: string[];
  /** The compiled figure's URL. Null only in `astro dev`, while a source doesn't compile. */
  compiled: string | null;
  poster: { src: string; srcset?: string; webp?: string; width: number; height: number } | null;
  card: { src: string; width: number; height: number } | null;
  info: FigureInfo | null;
  /** The figure as its poster shows it: the time, and every param's value and range by name. */
  moment: { time: number; values: Record<string, number>; ranges: Record<string, [number, number]> } | null;
  /** A link that opens the figure's sources in a playground, when the integration has one. */
  playground: string | null;
  hash: string | null;
}

/** Builds one figure under one set of sheets, by running `pencil build`. Paths are from the
 * integration's `root`. */
export declare function build(options: {
  src: string;
  sheets?: readonly string[];
  /** The poster's time, if not the document's own `poster`. */
  time?: number;
  /** The poster's params. */
  params?: Record<string, number>;
  /** The poster's width, for a figure with no design width. */
  size?: number;
  /** Also a 1200×630 card for sharing. */
  card?: boolean;
}): Promise<Built>;
