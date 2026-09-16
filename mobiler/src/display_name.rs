//! `mobiler display-name` — the user-visible app name, separate from the project identifier.
//!
//! `mobiler new` derives one PascalCase `{{NAME}}` and uses it both as an identifier (Gradle root
//! project, theme, Swift app struct, Xcode target/scheme) and as the name users see. Only the latter
//! should change after scaffolding, and it lives in exactly two places:
//!   - Android: `app_name` in `Android/app/src/main/res/values/strings.xml` (the launcher label and the
//!     name in system dialogs such as the notification-permission prompt);
//!   - iOS: `CFBundleDisplayName` under the target's `info.properties` in `iOS/project.yml`. XcodeGen
//!     regenerates `Sources/Info.plist` from those properties on every build, so the committed plist
//!     is not the place.
//!
//! strings.xml is app-owned (never touched by `upgrade`); project.yml is 3-way merged, and the line is
//! inserted right under `properties:` (away from the `# mobiler:info-plist` plugin anchor) so it
//! survives upgrades as an ordinary user edit.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

const STRINGS_REL: &str = "Android/app/src/main/res/values/strings.xml";
const PROJECT_YML_REL: &str = "iOS/project.yml";
const PLIST_KEY: &str = "CFBundleDisplayName:";

pub fn run(name: Option<&str>) -> Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    if !root.join("Android").is_dir() || !root.join("iOS").is_dir() {
        bail!("run `mobiler display-name` from a Mobiler app root (the dir with Android/ and iOS/)");
    }
    match name {
        None => {
            let (android, ios) = current(&root);
            println!("Android: {}", android.as_deref().unwrap_or("(not found)"));
            println!("iOS:     {}", ios.as_deref().unwrap_or("(not set; falls back to the Xcode target name)"));
        }
        Some(name) => {
            set(&root, name)?;
            println!("Display name set to \"{}\" (Android app_name + iOS CFBundleDisplayName).", name.trim());
            println!("Rebuild the app to see it; project identifiers are unchanged.");
        }
    }
    Ok(())
}

/// Set the display name on both platforms.
pub(crate) fn set(root: &Path, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("display name must not be empty");
    }
    if name.contains(['\n', '\r']) {
        bail!("display name must be a single line");
    }
    set_android(&root.join(STRINGS_REL), name)?;
    set_ios(&root.join(PROJECT_YML_REL), name)?;
    Ok(())
}

/// The current (Android, iOS) display names, unescaped.
fn current(root: &Path) -> (Option<String>, Option<String>) {
    let android = fs::read_to_string(root.join(STRINGS_REL)).ok().and_then(|t| {
        let line = t.lines().find(|l| l.contains("name=\"app_name\""))?;
        let start = line.find('>')? + 1;
        let end = line.rfind("</string>")?;
        Some(unescape_android(&line[start..end]))
    });
    let ios = fs::read_to_string(root.join(PROJECT_YML_REL)).ok().and_then(|t| {
        let line = t.lines().find(|l| l.trim_start().starts_with(PLIST_KEY))?;
        let value = line.trim_start()[PLIST_KEY.len()..].trim();
        Some(value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).map(unescape_yaml).unwrap_or(value.to_string()))
    });
    (android, ios)
}

fn set_android(path: &Path, name: &str) -> Result<()> {
    let content = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let Some(line) = content.lines().find(|l| l.contains("name=\"app_name\"")) else {
        bail!("no <string name=\"app_name\"> in {}", path.display());
    };
    let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let new_line = format!("{indent}<string name=\"app_name\">{}</string>", escape_android(name));
    fs::write(path, content.replacen(line, &new_line, 1)).with_context(|| format!("writing {}", path.display()))
}

fn set_ios(path: &Path, name: &str) -> Result<()> {
    let content = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value = format!("\"{}\"", escape_yaml(name));
    let updated = if let Some(line) = content.lines().find(|l| l.trim_start().starts_with(PLIST_KEY)) {
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        content.replacen(line, &format!("{indent}{PLIST_KEY} {value}"), 1)
    } else {
        // Insert as the first key under the app target's `info:` → `properties:` block.
        let lines: Vec<&str> = content.lines().collect();
        let info = lines.iter().position(|l| l.trim() == "info:");
        let props = info.and_then(|i| lines[i..].iter().position(|l| l.trim() == "properties:").map(|p| i + p));
        let Some(props) = props else {
            bail!("no `info:` → `properties:` block in {}", path.display());
        };
        let indent: String = lines[props].chars().take_while(|c| c.is_whitespace()).collect();
        let mut out: Vec<String> = lines.iter().map(ToString::to_string).collect();
        out.insert(props + 1, format!("{indent}  {PLIST_KEY} {value}"));
        let mut joined = out.join("\n");
        if content.ends_with('\n') {
            joined.push('\n');
        }
        joined
    };
    fs::write(path, updated).with_context(|| format!("writing {}", path.display()))
}

/// Android string resource escaping: XML entities plus aapt's own `'`/`"`/leading `@`/`?` rules.
fn escape_android(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '@' | '?' if i == 0 => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn unescape_android(s: &str) -> String {
    let s = s.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// YAML double-quoted scalar escaping.
fn escape_yaml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn unescape_yaml(s: &str) -> String {
    unescape_android(s) // `\x` → `x` covers the two escapes `escape_yaml` emits.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    const PROJECT_YML: &str = "name: MobileScaffold\ntargets:\n  MobileScaffold:\n    info:\n      path: Sources/Info.plist\n      properties:\n        PRODUCT_BUNDLE_IDENTIFIER: dev.x\n        # mobiler:info-plist\n";

    fn app() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("mob_display_name_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Android/app/src/main/res/values")).unwrap();
        fs::create_dir_all(root.join("iOS")).unwrap();
        fs::write(root.join(STRINGS_REL), "<resources>\n    <string name=\"app_name\">MobileScaffold</string>\n</resources>\n").unwrap();
        fs::write(root.join(PROJECT_YML_REL), PROJECT_YML).unwrap();
        root
    }

    #[test]
    fn sets_both_platforms_and_leaves_identifiers_alone() {
        let root = app();
        set(&root, "Appointments Admin").unwrap();

        let strings = fs::read_to_string(root.join(STRINGS_REL)).unwrap();
        assert!(strings.contains("    <string name=\"app_name\">Appointments Admin</string>\n"), "{strings}");
        let yml = fs::read_to_string(root.join(PROJECT_YML_REL)).unwrap();
        assert!(
            yml.contains("      properties:\n        CFBundleDisplayName: \"Appointments Admin\"\n        PRODUCT_BUNDLE_IDENTIFIER"),
            "inserted as the first property: {yml}"
        );
        assert!(yml.starts_with("name: MobileScaffold\n") && yml.contains("  MobileScaffold:\n"), "identifiers untouched");
        assert_eq!(current(&root), (Some("Appointments Admin".into()), Some("Appointments Admin".into())));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn setting_again_replaces_instead_of_duplicating() {
        let root = app();
        set(&root, "First").unwrap();
        set(&root, "Second").unwrap();
        let yml = fs::read_to_string(root.join(PROJECT_YML_REL)).unwrap();
        assert_eq!(yml.matches(PLIST_KEY).count(), 1, "{yml}");
        assert_eq!(current(&root), (Some("Second".into()), Some("Second".into())));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn escapes_special_characters_and_round_trips() {
        let root = app();
        let name = r#"Tom's "Cuts" & <Co> \ @home"#;
        set(&root, name).unwrap();
        let strings = fs::read_to_string(root.join(STRINGS_REL)).unwrap();
        assert!(strings.contains(r#">Tom\'s \"Cuts\" &amp; &lt;Co&gt; \\ @home</string>"#), "{strings}");
        assert_eq!(current(&root), (Some(name.into()), Some(name.into())));

        set(&root, "@Leading").unwrap();
        assert!(fs::read_to_string(root.join(STRINGS_REL)).unwrap().contains(r">\@Leading<"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_empty_and_multiline_names() {
        let root = app();
        assert!(set(&root, "   ").is_err());
        assert!(set(&root, "a\nb").is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
