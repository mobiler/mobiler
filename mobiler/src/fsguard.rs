//! Writes into the app tree that never follow a symlink at the destination: a link there may point
//! outside the app (a hostile or careless clone), and the CLI only ever means to write the app's own file.

use std::path::Path;

use anyhow::{Context, bail};

/// Whether `path` itself is a symlink (live or dangling).
pub(crate) fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink())
}

/// `fs::write`, refusing a destination that is a symlink: its target may be outside the app.
pub(crate) fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> anyhow::Result<()> {
    let path = path.as_ref();
    if is_symlink(path) {
        bail!("{} is a symlink — not writing through it (replace it with a real file)", path.display());
    }
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mob_fsguard_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn writes_a_plain_file() {
        let d = dir("plain");
        write(d.join("a.txt"), b"hi").unwrap();
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"hi");
        write(d.join("a.txt"), b"again").unwrap();
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"again");
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_write_through_a_symlink() {
        let d = dir("link");
        let outside = d.join("outside.txt");
        fs::write(&outside, "MINE").unwrap();
        std::os::unix::fs::symlink(&outside, d.join("live")).unwrap();
        std::os::unix::fs::symlink(d.join("nowhere"), d.join("dangling")).unwrap();
        let err = write(d.join("live"), b"x").unwrap_err();
        assert!(format!("{err:#}").contains("symlink"), "{err:#}");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "MINE");
        assert!(write(d.join("dangling"), b"x").is_err());
        assert!(!d.join("nowhere").exists(), "a dangling link's target is never created");
        let _ = fs::remove_dir_all(&d);
    }
}
