---
title: Upgrading an app
description: Bring a new Mobiler release into an existing app with mobiler upgrade, and fix your code where an API changed.
---

A Mobiler app has two parts that move with the framework:

- **Your Rust code** depends on `mobiler-core` (and on `mobiler-web` for a web client), like any crate.
- **The shells** in `Android/` and `iOS/` were written into your project by `mobiler new`, so a new release's
  shell improvements don't arrive by bumping a dependency.

`mobiler upgrade` handles both.

## Upgrade

```sh
cargo install mobiler            # the newer CLI first
cd myapp
mobiler upgrade                  # look first: changes are offered as *.mobiler-new files
mobiler upgrade --apply          # write them in place (old versions go to .mobiler/backup/)
```

- **A real three-way merge.** `mobiler new` saved the original shells in `.mobiler/base/`, so upgrade
  merges three versions of each file: the original, yours and the new one, like `git merge`. Framework
  changes apply and your own edits and plugin registrations stay. Only truly overlapping changes become a
  conflict, written as `<file>.mobiler-new` with conflict markers and never applied by itself.
- **Dependencies move too.** `--apply` moves the framework's own pins (`crux_core`, `mobiler-core`) in
  your app. A pin in another crate, such as a shared `app-core` or a crate outside the app folder, is
  listed for you to change by hand.
- **It never touches your Rust code** in `shared/src/`.
- **Nothing is forgotten.** Files waiting for review are recorded in `.mobiler/pending/`, and every run
  lists them. After resolving a conflict yourself, run `mobiler upgrade --resolved <file>`.
- **Plugins.** A plugin's native code is copied into your app. When a release updates a plugin you use,
  upgrade says so; `mobiler plugin add <name>` brings the new version in.

Commit `.mobiler/`, so the baseline, the version stamp and the pending reviews travel with the repository.

## When your code stops compiling

Upgrade never edits your Rust, so an API change shows up as a compile error. Most come from a struct that
gained a field. Writing literals with `..Default::default()` (or `..` in patterns) avoids nearly all of
them:

```rust
Theme { seed, corner: Corner::Large, ..Default::default() }
```

Each breaking change is listed, release by release, in the
[upgrade notes](https://github.com/mobiler/mobiler#breaking-changes-in-your-app-code), with what to change.
