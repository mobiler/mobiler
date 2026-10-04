//! Writes into the app tree that never follow a symlink at the destination: a link there may point
//! outside the app (a hostile or careless clone), and the CLI only ever means to write the app's own file.

use std::path::Path;

use anyhow::{Context, bail};

/// Whether `path` itself is a symlink (live or dangling).
pub(crate) fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink())
}

/// Refuse when `path`, or any folder between `root` (the app root) and it, is a symlink. A path outside
/// `root` is checked only itself.
pub(crate) fn check(root: &Path, path: &Path) -> anyhow::Result<()> {
    let link = match path.strip_prefix(root) {
        Ok(rel) => {
            let mut cur = root.to_path_buf();
            rel.components().find_map(|c| {
                cur.push(c);
                is_symlink(&cur).then(|| cur.clone())
            })
        }
        Err(_) => is_symlink(path).then(|| path.to_path_buf()),
    };
    if let Some(link) = link {
        bail!("{} is a symlink — not writing through it (replace it with a real file or folder)", link.display());
    }
    Ok(())
}

/// `fs::write` into the app at `root`: refused when the file or a folder on the way is a symlink (its
/// target may be outside the app); missing folders are created after that check.
pub(crate) fn write(root: &Path, path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> anyhow::Result<()> {
    let path = path.as_ref();
    check(root, path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
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
        write(&d, d.join("a.txt"), b"hi").unwrap();
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"hi");
        write(&d, d.join("sub/b.txt"), b"again").unwrap();
        assert_eq!(fs::read(d.join("sub/b.txt")).unwrap(), b"again", "missing folders are created");
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
        let err = write(&d, d.join("live"), b"x").unwrap_err();
        assert!(format!("{err:#}").contains("symlink"), "{err:#}");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "MINE");
        assert!(write(&d, d.join("dangling"), b"x").is_err());
        assert!(!d.join("nowhere").exists(), "a dangling link's target is never created");
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlinked_folder_on_the_way() {
        let d = dir("parent");
        let outside = dir("parent_outside");
        std::os::unix::fs::symlink(&outside, d.join(".mobiler")).unwrap();
        let err = write(&d, d.join(".mobiler/backup/x.txt"), b"x").unwrap_err();
        assert!(format!("{err:#}").contains(".mobiler is a symlink"), "{err:#}");
        assert!(!outside.join("backup").exists(), "nothing created through the link");
        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&outside);
    }
}
