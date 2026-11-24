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
- `src/generated/features.json`, the home page's list of capabilities and plugins (`src/components/Features.astro`),
  from the same two sources;
- `public/demos/`, the screenshots from `demos/*/screenshots/`.

The home page groups plugins by area with `PLUGIN_GROUPS` in `scripts/sync.mjs`. A new plugin that isn't in a
group fails the build, so the home page can't leave it out. The sync also fails the build when a page quotes a `mobiler-core`, `mobiler-web` or `crux_core` version behind
the repository's.

Everything else in `src/content/docs/` is hand-written and moves with the code (see `CLAUDE.md`).

Deploy: `.github/workflows/site-deploy.yml` publishes on every push to main that changes the site or
what it is generated from, and can be run by hand. The one-time GitHub Pages and DNS setup is in
[DEPLOY.md](DEPLOY.md).
