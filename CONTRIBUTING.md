# Contributing to Mobiler

Thanks for your interest. Issues and pull requests are welcome.

- **Bugs and ideas:** open an issue first for anything larger than a small fix, so we can agree on
  the shape before you write it.
- **Security problems:** never in a public issue. See [SECURITY.md](SECURITY.md).
- **Rules for changes:** [CLAUDE.md](CLAUDE.md) is the short rulebook for anyone changing this
  repo, human or agent: check the [design decisions](docs/adr/index.md) a change touches, update
  the READMEs that describe a behaviour you change, and keep the tests green.
- **Tests:** `cargo test --workspace` for the root workspace. `mobiler-web`, each demo and the
  site (`cd site && npm ci && npm run build`) build separately; CI runs all of them on your pull
  request.

Pull requests from forks need the maintainer's approval before CI runs, and only the maintainer
merges. The one exception is Dependabot's narrowest updates (patch/minor bumps of GitHub's own
actions, patch bumps of the site's npm packages), which merge automatically once every required
check passes; see `.github/workflows/dependabot-auto-merge.yml`. Releases to crates.io are made by the maintainer.

By contributing, you agree that your contribution is dual-licensed under the MIT and Apache-2.0
licenses, like the rest of the project.
