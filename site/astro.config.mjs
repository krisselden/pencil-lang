// @ts-check
import { defineConfig, passthroughImageService } from 'astro/config';
import pencil from '@pencil-lang/figure/astro';
import repoFiles from './integrations/repo-files.mjs';

// A static site: no server code. Its figures are built by `@pencil-lang/figure`, the package in
// this repo, from the corpus at the repo's root. The playground is `web/playground.html` itself,
// served with what it loads at the paths they have in the repo (see integrations/repo-files.mjs).
export default defineConfig({
  site: 'https://pencil-lang.dev',
  output: 'static',
  trailingSlash: 'always',
  integrations: [pencil({ root: '..' }), repoFiles()],
  image: { service: passthroughImageService() },
  devToolbar: { enabled: false },
  vite: {
    // `docs/language.md` is read from outside `site/`.
    server: { fs: { allow: ['..'] } },
  },
});
