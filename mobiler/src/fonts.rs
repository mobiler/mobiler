//! `mobiler.toml` `[fonts]`: the app's display + body font files, synced into the Android, iOS and web
//! shells so `FontFamily::Custom` can render them (see `sync`).

use serde::Deserialize;

/// `mobiler.toml` — today only `[fonts]`.
#[derive(Deserialize, Default, Debug)]
pub struct Manifest {
    pub fonts: Option<FontsSection>,
}

/// The two font roles: `display` (titles, subtitles, bar + sheet titles) and `body` (everything else).
#[derive(Deserialize, Default, Debug)]
pub struct FontsSection {
    pub display: Option<RoleSpec>,
    pub body: Option<RoleSpec>,
}

/// One role's files (paths relative to the app root) and an optional family name (else the first
/// file's own family name).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RoleSpec {
    pub family: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

/// What a font file says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontInfo {
    pub family: String,
    pub weight: u16,
}

fn be16(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes([s[0], s[1]])).ok_or_else(|| "truncated font file".to_string())
}

fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "truncated font file".to_string())
}

/// Read a TrueType/OpenType file's weight (OS/2 `usWeightClass`) and family (name ID 16, else 1).
/// Every read is bounds-checked: bad input is an `Err` with a human reason, never a panic.
pub fn read_font_info(bytes: &[u8]) -> Result<FontInfo, String> {
    match be32(bytes, 0) {
        Ok(0x0001_0000 | 0x7472_7565 /* true */ | 0x4F54_544F /* OTTO */) => {}
        _ => return Err("not a TrueType/OpenType file (woff/woff2 aren't supported natively)".into()),
    }
    let num_tables = usize::from(be16(bytes, 4)?);
    let (mut os2, mut name) = (None, None);
    for i in 0..num_tables {
        let rec = 12 + 16 * i;
        let tag = bytes.get(rec..rec + 4).ok_or("truncated font file")?;
        let (offset, length) = (be32(bytes, rec + 8)? as usize, be32(bytes, rec + 12)? as usize);
        let table = bytes.get(offset..offset.checked_add(length).ok_or("truncated font file")?).ok_or("truncated font file")?;
        match tag {
            b"OS/2" => os2 = Some(table),
            b"name" => name = Some(table),
            _ => {}
        }
    }
    let (Some(os2), Some(name)) = (os2, name) else { return Err("no OS/2/name table".into()) };
    let weight = be16(os2, 4)?;
    let family = family_name(name)?;
    Ok(FontInfo { family, weight })
}

/// The typographic family (name ID 16) if present, else the family (name ID 1); Windows UTF-16BE or
/// Mac Roman/ASCII records.
fn family_name(name: &[u8]) -> Result<String, String> {
    let count = usize::from(be16(name, 2)?);
    let strings = usize::from(be16(name, 4)?);
    let mut found: [Option<String>; 2] = [None, None]; // [id16, id1]
    for i in 0..count {
        let r = 6 + 12 * i;
        let (platform, name_id) = (be16(name, r)?, be16(name, r + 6)?);
        let slot = match name_id {
            16 => 0,
            1 => 1,
            _ => continue,
        };
        if found[slot].is_some() {
            continue;
        }
        let (len, off) = (usize::from(be16(name, r + 8)?), usize::from(be16(name, r + 10)?));
        let raw = name.get(strings + off..strings + off + len).ok_or("truncated font file")?;
        let text = match platform {
            0 | 3 => String::from_utf16_lossy(&raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()),
            1 => raw.iter().map(|&b| char::from(b)).collect(),
            _ => continue,
        };
        if !text.trim().is_empty() {
            found[slot] = Some(text.trim().to_string());
        }
    }
    let [id16, id1] = found;
    id16.or(id1).ok_or_else(|| "no family name in the font".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal sfnt with just OS/2 (weight) and name (family, nameID 1, Windows UTF-16BE).
    pub(super) fn tiny_font(weight: u16, family: &str) -> Vec<u8> {
        let name_str: Vec<u8> = family.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        let mut name = Vec::new();
        name.extend_from_slice(&0u16.to_be_bytes()); // format
        name.extend_from_slice(&1u16.to_be_bytes()); // count
        name.extend_from_slice(&(6u16 + 12).to_be_bytes()); // stringOffset
        for v in [3u16, 1, 0x409, 1, name_str.len() as u16, 0] {
            name.extend_from_slice(&v.to_be_bytes());
        }
        name.extend_from_slice(&name_str);
        let mut os2 = vec![0u8; 8];
        os2[4..6].copy_from_slice(&weight.to_be_bytes());
        let tables: [(&[u8; 4], &Vec<u8>); 2] = [(b"OS/2", &os2), (b"name", &name)];
        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0u8; 6]);
        let mut offset = 12 + 16 * tables.len();
        let mut body = Vec::new();
        for (tag, data) in tables {
            out.extend_from_slice(tag);
            out.extend_from_slice(&0u32.to_be_bytes());
            out.extend_from_slice(&(offset as u32).to_be_bytes());
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            offset += data.len();
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn sfnt_reads_weight_and_family() {
        let i = read_font_info(&tiny_font(600, "Space Grotesk")).unwrap();
        assert_eq!((i.family.as_str(), i.weight), ("Space Grotesk", 600));
    }

    #[test]
    fn sfnt_rejects_garbage_without_panicking() {
        assert!(read_font_info(b"wOF2\0\0\0\0garbage").is_err());
        assert!(read_font_info(&[]).is_err());
        let mut t = tiny_font(400, "X");
        t.truncate(20);
        assert!(read_font_info(&t).is_err());
    }

    #[test]
    fn manifest_parses_roles() {
        let m: Manifest = toml::from_str(
            r#"[fonts]
display = { family = "Space Grotesk", files = ["a.ttf"] }
body = { files = ["b.ttf", "c.ttf"] }"#,
        )
        .unwrap();
        let f = m.fonts.unwrap();
        assert_eq!(f.display.unwrap().family.as_deref(), Some("Space Grotesk"));
        assert_eq!(f.body.unwrap().files.len(), 2);
    }
}
