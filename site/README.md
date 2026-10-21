# mobiler.rs

The documentation site, built with [Astro](https://astro.build) and [Starlight](https://starlight.astro.build).

```sh
npm ci
npm run dev     # http://localhost:4321
npm run build   # site/dist, with link validation
```

Both run `scripts/sync.mjs` first. It generates, from the repository (these files are gitignored):

- `src/content/docs/tutorial/`, the Saldo tutorial, from `docs/tutorial/saldo/`;
- `src/content/docs/reference/capabilities.md`, from `capabilities.json`;
- `src/content/docs/reference/plugins.md`, from `mobiler/plugins/*/mobiler-plugin.toml`;
- `public/demos/`, the screenshots from `demos/*/screenshots/`.

It then fails the build when a page quotes a `mobiler-core`, `mobiler-web` or `crux_core` version behind
the repository's.

Everything else in `src/content/docs/` is hand-written and moves with the code (see `CLAUDE.md`).

Deploy: run the **Deploy site** workflow (`.github/workflows/site-deploy.yml`) from main.
