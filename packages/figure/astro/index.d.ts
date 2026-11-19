// Types for the Astro integration, for a TypeScript site. `Figure.astro` declares its own props.
import type { AstroIntegration } from 'astro';

export { alternates, build, PUBLIC } from './build.js';
export type { Built, Choice, FigureInfo, Sheets } from './build.js';

export interface PencilOptions {
  /** Where `src` and `sheets` paths start, from the Astro project. The project, by default. */
  root?: string;
  /** The `pencil` command. By default PENCIL_BIN, then a Pencil checkout's, then `pencil` on the PATH. */
  bin?: string;
  /** A Pencil playground's address, as `https://pencil-lang.dev/playground/`, for links that open a
   * figure's sources there. */
  playground?: string;
  /** A folder in the project to keep built figures in, committed with the site, for a build that
   * runs where there's no `pencil` or no GPU. */
  kept?: string;
}

export default function pencil(options?: PencilOptions): AstroIntegration;
