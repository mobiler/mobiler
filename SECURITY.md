# Security policy

## Reporting a vulnerability

Please **don't open a public issue** for a security problem. Report it privately through
GitHub: [Report a vulnerability](https://github.com/mobiler/mobiler/security/advisories/new)
(the repository's **Security** tab → **Report a vulnerability**).

Include what you found, the affected crate or plugin and version, and steps to reproduce. You'll
get a reply within a week. Once a fix is released, the advisory is published with credit to you,
unless you'd rather stay anonymous.

## Supported versions

Mobiler is pre-1.0. Fixes land in the **latest release** of each crate (`mobiler`,
`mobiler-core`, `mobiler-ui`, `mobiler-web`); older versions are not patched. `mobiler upgrade`
moves an app to the latest release.

## Scope

In scope: the `mobiler` CLI and the code it writes into an app (templates, shells, bundled
plugins), and the published libraries. The demo apps and the mobiler.rs site are examples rather than
products; report issues in them the same way.
