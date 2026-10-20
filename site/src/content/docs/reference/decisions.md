---
title: Design decisions
description: Mobiler's architecture decision records, and why they matter to an app developer.
---

Every choice that shapes Mobiler (the wire format, what a shell may hold, how upgrades merge, what each
platform guarantees) is written down as an **architecture decision record**: the problem, the options
considered, what was chosen and why, and what it costs. A record is never edited after it's accepted; a
changed decision gets a new record that replaces the old one, so the history stays readable.

Many records come with a test in the repository that fails if the code stops matching the decision.

Read them in the [ADR index on GitHub](https://github.com/mobiler/mobiler/blob/main/docs/adr/index.md).
A few that explain what an app can rely on:

- **ADR-0001:** the shells are generic, so an app has no native code of its own.
- **ADR-0057, ADR-0058, ADR-0062:** a tab keeps its screen (Android, iOS) or its scroll position (web).
- **ADR-0055, ADR-0056:** what `mobiler upgrade` changes in your dependencies, and what it only reports.
- **ADR-0061:** app data stays in the platform's backups, and what that exposes.
