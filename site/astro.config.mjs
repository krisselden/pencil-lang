// @ts-check
import { defineConfig, passthroughImageService } from 'astro/config';
import repoFiles from './integrations/repo-files.mjs';

// A static site: no server code. The figures, the element and the playground are files from the
// rest of the repo, served at the paths they have there (see integrations/repo-files.mjs).
export default defineConfig({
  site: 'https://pencil-lang.dev',
  output: 'static',
  trailingSlash: 'always',
  integrations: [repoFiles()],
  image: { service: passthroughImageService() },
  devToolbar: { enabled: false },
  vite: {
    // `docs/language.md` is read from outside `site/`.
    server: { fs: { allow: ['..'] } },
  },
});
