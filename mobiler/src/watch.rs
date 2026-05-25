use anyhow::{Context, Result};
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::dev::{self, Project};

const DEBOUNCE: Duration = Duration::from_millis(500);
/// After a rebuild completes, drain events for this long before returning to the
/// blocking recv. cargo and gradle touch source-file mtimes for fingerprint tracking,
/// which fire spurious notify events; we discard those rather than rebuild twice.
const SETTLE_AFTER_REBUILD: Duration = Duration::from_millis(800);

pub fn run(no_install: bool, no_run: bool) -> Result<()> {
    let project = Project::detect()?;
    let java_home = dev::resolve_java_home();

    println!("Mobiler watch");
    println!(
        "  Project:   {} ({})",
        project.root.display(),
        project.application_id
    );
    if let Some(jh) = &java_home {
        println!("  JAVA_HOME: {jh}");
    }
    println!("  Watching:  shared/ + Android/app/src/ + workspace config");
    println!("  Press Ctrl-C to stop.");
    println!();

    // Initial build.
    println!("--- initial build ---");
    if let Err(e) = dev::pipeline(&project, java_home.as_deref(), no_install, no_run) {
        eprintln!("initial build failed: {e:#}");
        // Continue into watch loop anyway — user can edit and we'll retry.
    }

    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(DEBOUNCE, None, tx)
        .context("creating file watcher")?;

    // Watch the directories we care about. Avoid watching anything under
    // target/, Android/build/, Android/.gradle/, Android/generated/ — those
    // are written by the build itself and would create a feedback loop.
    let watch_dirs = [
        project.root.join("shared/src"),
        project.root.join("Android/app/src"),
        project.root.join("Android/shared/src"),
        project.root.join("Android/gradle"),
    ];
    let watch_files = [
        project.root.join("Cargo.toml"),
        project.root.join("shared/Cargo.toml"),
        project.root.join("shared/uniffi.toml"),
        project.root.join("Android/settings.gradle.kts"),
        project.root.join("Android/build.gradle.kts"),
        project.root.join("Android/app/build.gradle.kts"),
        project.root.join("Android/shared/build.gradle.kts"),
    ];

    for dir in &watch_dirs {
        if dir.is_dir() {
            debouncer
                .watch(dir, RecursiveMode::Recursive)
                .with_context(|| format!("watching {}", dir.display()))?;
        }
    }
    for file in &watch_files {
        if file.is_file() {
            debouncer
                .watch(file, RecursiveMode::NonRecursive)
                .with_context(|| format!("watching {}", file.display()))?;
        }
    }

    println!();
    println!("--- watching for changes ---");

    loop {
        // Block for the next event batch (debounced).
        let first = match rx.recv() {
            Ok(batch) => batch,
            Err(_) => break, // sender dropped — debouncer died
        };

        let mut changed_paths: BTreeSet<PathBuf> = BTreeSet::new();
        collect_paths(first, &project.root, &mut changed_paths);

        // Drain anything else queued in case multiple batches piled up while
        // we were processing — keeps us from rebuilding twice in a row for
        // a burst of saves.
        while let Ok(more) = rx.try_recv() {
            collect_paths(more, &project.root, &mut changed_paths);
        }

        if changed_paths.is_empty() {
            // Debouncer can emit empty batches when events were all in excluded dirs.
            continue;
        }

        println!();
        println!("--- change detected ---");
        for p in &changed_paths {
            println!("  {}", p.display());
        }

        let started = Instant::now();
        match dev::pipeline(&project, java_home.as_deref(), no_install, no_run) {
            Ok(()) => println!(
                "--- rebuild ok in {:.1}s ---",
                started.elapsed().as_secs_f64()
            ),
            Err(e) => eprintln!("rebuild failed: {e:#}"),
        }

        // Swallow events fired while the build was running (mtimes touched by
        // cargo/gradle for their fingerprint tracking). Real user edits made
        // during this brief window are still picked up on the next iteration.
        let until = Instant::now() + SETTLE_AFTER_REBUILD;
        while let Some(remaining) = until.checked_duration_since(Instant::now()) {
            if rx.recv_timeout(remaining).is_err() {
                break;
            }
        }

        println!();
        println!("--- watching for changes ---");
    }

    Ok(())
}

/// Collect interesting changed paths from a notify-debouncer batch, filtering
/// out anything under build-output directories so we don't trigger ourselves.
fn collect_paths(
    batch: Result<Vec<notify_debouncer_full::DebouncedEvent>, Vec<notify::Error>>,
    project_root: &Path,
    out: &mut BTreeSet<PathBuf>,
) {
    let Ok(events) = batch else { return };
    for ev in events {
        for p in &ev.paths {
            if is_excluded(p, project_root) {
                continue;
            }
            // We only care about file changes. Skip events whose path is a directory
            // (notify reports these when child file ops bubble up to parent dirs, and
            // also during recursive-watch setup). If the path no longer exists, treat
            // it as a delete and keep it.
            if let Ok(meta) = std::fs::metadata(p) {
                if !meta.is_file() {
                    continue;
                }
            }
            // Normalize to project-relative when possible for tidy output.
            let display = p
                .strip_prefix(project_root)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| p.clone());
            out.insert(display);
        }
    }
}

fn is_excluded(p: &Path, project_root: &Path) -> bool {
    let Ok(rel) = p.strip_prefix(project_root) else {
        return false;
    };
    let s = rel.to_string_lossy();
    s.starts_with("target/")
        || s.starts_with("Android/build/")
        || s.starts_with("Android/.gradle/")
        || s.starts_with("Android/app/build/")
        || s.starts_with("Android/shared/build/")
        || s.starts_with("Android/generated/")
        || s.starts_with(".kotlin/")
        || s.ends_with(".swp")
        || s.ends_with("~")
}
