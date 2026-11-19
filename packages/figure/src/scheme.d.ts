// Types for `scheme.js`, which `Figure.astro` imports.

/** Answers `prefers-color-scheme` in the `media` of every `<source>` in a figure with the scheme
 * the page pins there, and gives that scheme, or `null` with none pinned. */
export declare function settle(figure: Element): 'light' | 'dark' | null;
export declare function follow(figure: Element): void;
export declare function unfollow(figure: Element): void;
