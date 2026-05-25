use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

use crate::dev::Project;

pub fn run(name: &str, raw_fields: &[String]) -> Result<()> {
    validate_pascal_case(name)?;
    let fields = parse_fields(raw_fields)?;
    let project = Project::detect()?;

    let app_rs = project.root.join("shared/src/app.rs");
    let main_kt = find_main_activity(&project.root)?;

    let app_rs_text = fs::read_to_string(&app_rs)
        .with_context(|| format!("reading {}", app_rs.display()))?;
    let main_kt_text = fs::read_to_string(&main_kt)
        .with_context(|| format!("reading {}", main_kt.display()))?;

    let new_app_rs = insert_rust_variant(&app_rs_text, name, &fields)
        .context("inserting into shared/src/app.rs")?;
    let new_main_kt = insert_kotlin_arm(&main_kt_text, name, &fields)
        .context("inserting into MainActivity.kt")?;

    fs::write(&app_rs, new_app_rs)?;
    fs::write(&main_kt, new_main_kt)?;

    println!("Added Widget::{name} variant:");
    println!("  {}  (Rust enum)", app_rs.display());
    println!("  {}  (Compose Render arm with TODO body)", main_kt.display());
    println!();
    println!("Next:");
    println!("  1. Implement the Compose body in MainActivity.kt (replace the TODO)");
    println!("  2. Use Widget::{name} in your view() function");
    println!("  3. mobiler dev   # rebuild + reinstall");
    Ok(())
}

// -------------------- field parsing --------------------

struct Field {
    name: String,
    ty: String,
}

fn parse_fields(raw: &[String]) -> Result<Vec<Field>> {
    raw.iter()
        .map(|s| {
            let (name, ty) = s
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("expected `name:Type`, got `{s}`"))?;
            let name = name.trim();
            let ty = ty.trim();
            if name.is_empty() || ty.is_empty() {
                bail!("empty name or type in field spec `{s}`");
            }
            // Rust field names: lowercase / snake_case.
            if !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                bail!(
                    "field name `{name}` should be snake_case (lowercase + underscores)"
                );
            }
            Ok(Field {
                name: name.to_string(),
                ty: ty.to_string(),
            })
        })
        .collect()
}

fn validate_pascal_case(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("variant name must not be empty");
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_uppercase() {
        bail!("variant name `{name}` must start with an uppercase letter (PascalCase)");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("variant name `{name}` may contain only [a-zA-Z0-9]");
    }
    Ok(())
}

// -------------------- Rust source mutation --------------------

/// Insert a new variant into the `pub enum Widget { ... }` block, before the
/// closing brace. Indentation matches existing variants (4 spaces).
fn insert_rust_variant(source: &str, name: &str, fields: &[Field]) -> Result<String> {
    let enum_start = source
        .find("pub enum Widget {")
        .ok_or_else(|| anyhow::anyhow!("could not find `pub enum Widget {{` in app.rs"))?;
    let after_open = enum_start
        + source[enum_start..]
            .find('{')
            .ok_or_else(|| anyhow::anyhow!("malformed enum header"))?
        + 1;
    let close = find_matching_brace(source, after_open)
        .ok_or_else(|| anyhow::anyhow!("could not find matching `}}` for Widget enum"))?;

    if source[enum_start..close].contains(&format!("\n    {name} ")) ||
       source[enum_start..close].contains(&format!("\n    {name},")) ||
       source[enum_start..close].contains(&format!("\n    {name} {{")) {
        bail!("Widget::{name} already exists");
    }

    let variant = render_rust_variant(name, fields);
    // Insert before the `}` line. Walk back to the start of that line.
    let line_start = source[..close]
        .rfind('\n')
        .map(|n| n + 1)
        .unwrap_or(close);
    let mut out = String::with_capacity(source.len() + variant.len());
    out.push_str(&source[..line_start]);
    out.push_str(&variant);
    out.push_str(&source[line_start..]);
    Ok(out)
}

fn render_rust_variant(name: &str, fields: &[Field]) -> String {
    if fields.is_empty() {
        return format!("    {name},\n");
    }
    // Always emit multi-line struct variant for readability.
    let mut s = format!("    {name} {{\n");
    for f in fields {
        s.push_str(&format!("        {}: {},\n", f.name, f.ty));
    }
    s.push_str("    },\n");
    s
}

// -------------------- Kotlin source mutation --------------------

/// Insert a new `is Widget.<Name> -> TODO(...)` arm into the first `when (widget) {` block,
/// which is the `Render` function (not the `RowItem` helper).
fn insert_kotlin_arm(source: &str, name: &str, fields: &[Field]) -> Result<String> {
    // Locate Render's when block. The first `when (widget) {` in the file is Render's
    // (RowItem's is later). To be defensive, prefer the one after `fun Render(`.
    let render_pos = source.find("fun Render(").ok_or_else(|| {
        anyhow::anyhow!(
            "could not find `fun Render(` in MainActivity.kt — \
             expected the generic Compose Render function"
        )
    })?;
    let when_pos_rel = source[render_pos..]
        .find("when (widget) {")
        .ok_or_else(|| anyhow::anyhow!("no `when (widget) {{` found after `fun Render(`"))?;
    let after_open = render_pos
        + when_pos_rel
        + source[render_pos + when_pos_rel..]
            .find('{')
            .ok_or_else(|| anyhow::anyhow!("malformed when block"))?
        + 1;
    let close = find_matching_brace_kotlin(source, after_open)
        .ok_or_else(|| anyhow::anyhow!("could not find matching `}}` for when block"))?;

    if source[..close].contains(&format!("is Widget.{name} ->")) {
        bail!("Widget.{name} arm already exists in Render");
    }

    let arm = render_kotlin_arm(name, fields);
    let line_start = source[..close]
        .rfind('\n')
        .map(|n| n + 1)
        .unwrap_or(close);
    let mut out = String::with_capacity(source.len() + arm.len());
    out.push_str(&source[..line_start]);
    out.push_str(&arm);
    out.push_str(&source[line_start..]);
    Ok(out)
}

fn render_kotlin_arm(name: &str, fields: &[Field]) -> String {
    // Render arms in the showcase are indented 8 spaces.
    let mut s = String::new();
    if !fields.is_empty() {
        // Spell out the field accessors as a comment so the user can see what's available
        // without consulting generated/Types.kt.
        s.push_str("        // Fields available: ");
        let names: Vec<String> = fields
            .iter()
            .map(|f| format!("widget.{}", camel_case(&f.name)))
            .collect();
        s.push_str(&names.join(", "));
        s.push('\n');
    }
    s.push_str(&format!(
        "        is Widget.{name} -> TODO(\"implement {name}\")\n"
    ));
    s
}

fn camel_case(snake: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = false;
    for c in snake.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            out.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

// -------------------- bracket counting --------------------

/// Find the index of the `}` that matches the `{` immediately before `start`.
/// `start` should be the byte index just AFTER the opening `{`.
/// Returns the byte index of the matching `}`, or None if unmatched.
fn find_matching_brace(source: &str, start: usize) -> Option<usize> {
    let mut depth = 1usize;
    let bytes = source.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            // Skip string and char literals to avoid counting braces inside them.
            b'"' => i = skip_string_lit(source, i)?,
            b'\'' => i = skip_char_lit(source, i),
            b'/' if bytes.get(i + 1) == Some(&b'/') => i = skip_line_comment(source, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(source, i)?,
            _ => {}
        }
        i += 1;
    }
    None
}

/// Kotlin variant — same algorithm, slightly different lexing details around triple-quoted
/// strings and `${ ... }` interpolation. For our hand-written Render we don't need those,
/// so the same routine works.
fn find_matching_brace_kotlin(source: &str, start: usize) -> Option<usize> {
    find_matching_brace(source, start)
}

fn skip_string_lit(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2, // escape sequence — skip next byte
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn skip_char_lit(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = start + 1;
    // Rust char literals are short: `'x'`, `'\n'`, `'\u{...}'`. Be defensive.
    while i < bytes.len() && bytes[i] != b'\'' {
        if bytes[i] == b'\\' {
            i += 1;
        }
        i += 1;
        // Don't run past end-of-line — a stray `'` (e.g. in `Milan's`) shouldn't eat the rest of the file.
        if bytes[i.min(bytes.len() - 1)] == b'\n' {
            // Probably wasn't a char literal — back up and treat as a regular char.
            return start;
        }
    }
    i
}

fn skip_line_comment(source: &str, start: usize) -> usize {
    source[start..]
        .find('\n')
        .map(|n| start + n)
        .unwrap_or(source.len() - 1)
}

fn skip_block_comment(source: &str, start: usize) -> Option<usize> {
    source[start + 2..].find("*/").map(|n| start + 2 + n + 1)
}

fn find_main_activity(root: &Path) -> Result<std::path::PathBuf> {
    // The Kotlin source lives at Android/app/src/main/java/<package_path>/MainActivity.kt
    // We don't know the package path here without parsing app/build.gradle.kts.
    // Walk the java tree to find any file named MainActivity.kt.
    let java_root = root.join("Android/app/src/main/java");
    if !java_root.is_dir() {
        bail!("missing {}", java_root.display());
    }
    walk_find(&java_root, "MainActivity.kt").ok_or_else(|| {
        anyhow::anyhow!("could not find MainActivity.kt under {}", java_root.display())
    })
}

fn walk_find(dir: &Path, target: &str) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = walk_find(&p, target) {
                return Some(found);
            }
        } else if p.file_name().and_then(|n| n.to_str()) == Some(target) {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn camel_case_works() {
        assert_eq!(camel_case("id"), "id");
        assert_eq!(camel_case("on_press"), "onPress");
        assert_eq!(camel_case("dark_mode"), "darkMode");
        assert_eq!(camel_case("foo_bar_baz"), "fooBarBaz");
    }

    #[test]
    fn parses_field_specs() {
        let f = parse_fields(&["id:String".into(), "value:f32".into()]).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].name, "id");
        assert_eq!(f[0].ty, "String");
        assert_eq!(f[1].name, "value");
        assert_eq!(f[1].ty, "f32");
    }

    #[test]
    fn rejects_bad_field_spec() {
        assert!(parse_fields(&["nocolon".into()]).is_err());
        assert!(parse_fields(&["Bad:String".into()]).is_err()); // not snake_case
    }

    #[test]
    fn rejects_bad_variant_name() {
        assert!(validate_pascal_case("slider").is_err());
        assert!(validate_pascal_case("Foo_Bar").is_err());
        assert!(validate_pascal_case("").is_err());
        assert!(validate_pascal_case("Slider").is_ok());
        assert!(validate_pascal_case("MyWidget").is_ok());
    }

    #[test]
    fn inserts_unit_variant() {
        let src = "pub enum Widget {\n    Text { content: String },\n    Button,\n}\n";
        let out = insert_rust_variant(src, "Spacer", &[]).unwrap();
        assert!(out.contains("    Button,\n    Spacer,\n}"));
    }

    #[test]
    fn inserts_struct_variant() {
        let src = "pub enum Widget {\n    Text { content: String },\n}\n";
        let f = parse_fields(&["id:String".into(), "value:f32".into()]).unwrap();
        let out = insert_rust_variant(src, "Slider", &f).unwrap();
        assert!(out.contains("    Slider {\n        id: String,\n        value: f32,\n    },\n}"));
    }

    #[test]
    fn rejects_duplicate_variant() {
        let src = "pub enum Widget {\n    Slider { id: String },\n}\n";
        assert!(insert_rust_variant(src, "Slider", &[]).is_err());
    }

    #[test]
    fn insert_kotlin_arm_finds_render_when() {
        let src = r#"
fun Render(widget: Widget, send: (Event) -> Unit) {
    when (widget) {
        is Widget.Text -> Text(text = widget.content)
        is Widget.Button -> Button(onClick = {}) { Text(widget.label) }
    }
}
"#;
        let f = parse_fields(&["id:String".into(), "on_press:Event".into()]).unwrap();
        let out = insert_kotlin_arm(src, "Slider", &f).unwrap();
        assert!(out.contains("// Fields available: widget.id, widget.onPress"));
        assert!(out.contains("is Widget.Slider -> TODO(\"implement Slider\")"));
        // The arm landed before the closing `}` of the when, not at end of file.
        let arm_pos = out.find("is Widget.Slider").unwrap();
        let when_close = out.rfind("}\n}\n").unwrap();
        assert!(arm_pos < when_close);
    }
}
