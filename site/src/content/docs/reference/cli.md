---
title: CLI
description: The mobiler command-line tool, command by command.
---

Install it with `cargo install mobiler`. Run the app commands from the app root (the folder with
`Android/`, `iOS/` and `shared/`). `mobiler <command> --help` shows every option.

| Command | What it does |
|---|---|
| `mobiler doctor` | Checks that this machine has what building needs, and says what is missing. |
| `mobiler new <name>` | Creates an app: the Rust core and the generic Android and iOS shells. `--package` sets the app id, `--display-name` the name users see, `--agentic` adds a guide for coding agents. |
| `mobiler dev` | Builds the core, generates the bindings, builds the Android app, installs it and launches it. `--device <serial>` picks a device. |
| `mobiler watch` | `dev`, again on every change. |
| `mobiler build [android\|ios]` | Builds the app without installing it. iOS needs a Mac. |
| `mobiler plugin list` | Lists the bundled [plugins](/reference/plugins/). |
| `mobiler plugin add <name>…` | Copies plugins into your shells and registers them; again to update one. |
| `mobiler upgrade` | Brings a new release's shells and framework pins into the app. See [Upgrading](/guides/upgrading/). |
| `mobiler display-name [name]` | Prints or sets the name users see. |
| `mobiler fonts sync` | Applies `mobiler.toml` `[fonts]`. |
| `mobiler splash sync` | Applies `[splash]`. |
| `mobiler deep-links sync` | Applies `[deep_links]`. |
| `mobiler local-network sync` | Applies `[ios] local_network`. |

`build`, `dev` and `watch` run the sync commands first, so you rarely call them yourself. See
[mobiler.toml options](/guides/configuration/).

The complete CLI documentation, including Android version support and every edge case of each option,
is the [CLI README](https://github.com/mobiler/mobiler/blob/main/mobiler/README.md).
