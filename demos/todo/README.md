# Todo — Mobiler demo

A todo / projects app built with [Mobiler](../../), and the original showcase for
the semantic widget set. The Rust core (Crux) owns all state and logic; the generic
Compose shell renders the `Widget` tree to native Material 3, dark mode included.

<p>
  <img src="screenshots/today.png" width="260" alt="Today — must-dos with project color dots">
  &nbsp;
  <img src="screenshots/projects.png" width="260" alt="Projects — list with the color picker">
</p>

## What it shows

- **Text** styles, **Card** styles, **Checkbox** / **Switch**, **Badge** (semantic tones), **IconButton**s, **TextField**
- A bottom-nav **Scaffold** (Today / Projects / Settings) with a project-detail screen
- Per-project identity colors via **ColorDot** and a **ColorSwatch** picker
- State persisted across cold restarts

## Run

```bash
cargo install mobiler        # or use the repo build: cargo build -p mobiler
cd demos/todo
mobiler dev                  # build the core, generate Kotlin types, build + launch on a device/emulator
```
