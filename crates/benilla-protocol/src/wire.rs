//! Low-level wire read/write helpers shared by the auth ([`crate::auth`]) and world
//! ([`crate::world`]) protocols. Everything is little endian unless noted; these mirror the exact
//! field encodings the 1.12 client uses.

use std::io::{self, Read, Write};

/// WoW packs an 8-byte GUID as a one-byte mask (which of the 8 bytes are non-zero) followed by only
/// those non-zero bytes, low to high — used throughout the world protocol for object references.
pub fn read_packed_guid(r: &mut impl Read) -> io::Result<u64> {
    let mask = read_u8(r)?;
    let mut guid = 0u64;
    for i in 0..8 {
        if mask & (1 << i) != 0 {
            guid |= (u64::from(read_u8(r)?)) << (i * 8);
        }
    }
    Ok(guid)
}

/// Write a packed GUID (see [`read_packed_guid`]).
pub fn write_packed_guid(guid: u64, w: &mut impl Write) -> io::Result<()> {
    let bytes = guid.to_le_bytes();
    let mut mask = 0u8;
    let mut out = [0u8; 9];
    let mut idx = 1;
    for (i, &b) in bytes.iter().enumerate() {
        if b != 0 {
            mask |= 1 << i;
            out[idx] = b;
            idx += 1;
        }
    }
    out[0] = mask;
    w.write_all(&out[..idx])
}

pub fn read_u8(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

pub fn read_u16_le(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

pub fn read_u32_le(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

pub fn read_u32_be(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}

pub fn read_i32_le(r: &mut impl Read) -> io::Result<i32> {
    Ok(read_u32_le(r)? as i32)
}

pub fn read_u64_le(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

pub fn read_f32_le(r: &mut impl Read) -> io::Result<f32> {
    Ok(f32::from_bits(read_u32_le(r)?))
}

/// Read a fixed-length byte array.
pub fn read_array<const N: usize>(r: &mut impl Read) -> io::Result<[u8; N]> {
    let mut b = [0u8; N];
    r.read_exact(&mut b)?;
    Ok(b)
}

/// Read a NUL-terminated string (the NUL is consumed). Invalid UTF-8 is lossily replaced.
pub fn read_cstring(r: &mut impl Read) -> io::Result<String> {
    let mut bytes = Vec::new();
    loop {
        let b = read_u8(r)?;
        if b == 0 {
            break;
        }
        bytes.push(b);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Bound a wire-derived element count before it becomes a `Vec::with_capacity` hint.
///
/// A count read off the wire is server- (and so bug- and attacker-) controlled, and
/// `Vec::with_capacity(count as usize)` on a raw `u32` is the one failure the decoders' `io::Result`
/// totality does not cover: an allocation that cannot be satisfied is `handle_alloc_error` →
/// `abort()` — no unwind, no [`crate::Poll::Skipped`], no tally (decision 2265 §B1). The returned
/// hint bounds only the **up-front** allocation: the `Vec` still grows past it when the body really
/// carries more rows, and a lying count still fails on the short read of the body, which is
/// `u16`-bounded and read whole before parse. `cap` is the protocol's own bound where one exists
/// (cited at the call site), or a generous sane one where none does.
///
/// Every `with_capacity` under `messages/` whose argument is a wire count goes through this; the
/// source-scan test beside it (`wire_count_capacity_hints_are_capped`) keeps that true.
pub fn capacity_hint(count: impl TryInto<u64>, cap: usize) -> usize {
    count
        .try_into()
        .ok()
        .and_then(|n| usize::try_from(n).ok())
        .map_or(cap, |n| n.min(cap))
}

/// A 3-float position/vector in raw WoW coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector3d {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3d {
    pub fn read(r: &mut impl Read) -> io::Result<Self> {
        Ok(Self {
            x: read_f32_le(r)?,
            y: read_f32_le(r)?,
            z: read_f32_le(r)?,
        })
    }

    pub fn write(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(&self.x.to_le_bytes())?;
        w.write_all(&self.y.to_le_bytes())?;
        w.write_all(&self.z.to_le_bytes())
    }
}

/// Decode a movement-spline packed point (`SMSG_MONSTER_MOVE` packs every point after the first this
/// way): **signed** two's-complement fields — 11 bits x, 11 bits y, 10 bits z — in quarter-yard
/// units (the server packs `round(v * 4)` per axis; vmangos `ByteBuffer::appendPackXYZ`). The
/// decoded vector is the offset from the spline's **destination** back to this waypoint
/// (`destination - waypoint`; vmangos `PacketBuilder::WriteLinearPath`), not an absolute position.
pub fn packed_to_vector3d(p: i32) -> Vector3d {
    // Sign-extend each field: shift it to the top of the i32, then arithmetic-shift back down.
    Vector3d {
        x: ((p << 21) >> 21) as f32 * 0.25,
        y: ((p << 10) >> 21) as f32 * 0.25,
        z: (p >> 22) as f32 * 0.25,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The producer's encoder, transcribed from vmangos `ByteBuffer::appendPackXYZ`:
    /// `packed |= ((int)lroundf(v * 4.0f) & mask) << shift` with masks 0x7FF/0x7FF/0x3FF.
    fn pack_xyz(x: f32, y: f32, z: f32) -> i32 {
        let mut packed = 0u32;
        packed |= ((x * 4.0).round() as i32 & 0x7FF) as u32;
        packed |= (((y * 4.0).round() as i32 & 0x7FF) as u32) << 11;
        packed |= (((z * 4.0).round() as i32 & 0x3FF) as u32) << 22;
        packed as i32
    }

    #[test]
    fn packed_point_roundtrips_signed_quarter_yards() {
        // Field ranges: x/y are 11-bit signed (−256 .. 255.75 yd), z 10-bit signed (−128 .. 127.75 yd).
        // Quarter-yard multiples are exact in f32, so the round-trip must be exact — including the
        // negative offsets the old unsigned decode destroyed.
        for &(x, y, z) in &[
            (0.0f32, 0.0f32, 0.0f32),
            (1.25, -2.5, 3.75),
            (-100.25, 100.0, -50.5),
            (255.75, -256.0, 127.75),
            (-0.25, 0.25, -0.25),
        ] {
            let v = packed_to_vector3d(pack_xyz(x, y, z));
            assert_eq!((v.x, v.y, v.z), (x, y, z));
        }
    }
    #[test]
    fn capacity_hint_is_the_smaller_of_count_and_cap() {
        assert_eq!(capacity_hint(3u8, 64), 3);
        assert_eq!(capacity_hint(0xFFFF_FFFFu32, 64), 64);
        assert_eq!(capacity_hint(0u32, 64), 0);
        assert_eq!(capacity_hint(usize::MAX, 8), 8);
        assert_eq!(capacity_hint(u64::MAX, 1024), 1024);
    }

    /// **The rule, enforced instead of remembered** (decision 2265 §B1).
    ///
    /// A `Vec::with_capacity` whose argument is a raw wire count aborts the process — not the
    /// packet — the day a server (or a decoder bug) puts a huge number there: an allocation
    /// failure never unwinds, so it never becomes `Poll::Skipped`. There is no runtime signal to
    /// test for, which is exactly why the check is structural: every `with_capacity(` under
    /// `messages/` whose argument is a bare variable (cast or not) must go through
    /// [`capacity_hint`] or carry a `.min(`, or be named in [`EXEMPT`] with the reason.
    #[test]
    fn wire_count_capacity_hints_are_capped() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/messages");
        let mut offenders = Vec::new();
        for file in rust_files(&root) {
            let text = std::fs::read_to_string(&file).expect("readable source");
            let rel = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            for (line_no, arg) in capacity_arguments(&text) {
                if uncapped_wire_count(&arg) && !EXEMPT.contains(&(rel.as_str(), arg.as_str())) {
                    offenders.push(format!("{rel}:{line_no}: `with_capacity({arg})`"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "these capacity hints are raw wire counts — an unsatisfiable allocation aborts the \
             process instead of skipping the packet. Bound each with \
             `crate::wire::capacity_hint(count, CAP)` (CAP = the protocol's own bound, cited at \
             the site, or a generous sane one), or add it to `EXEMPT` in `wire.rs` with the reason \
             it is not a wire count (decision 2265 §B1):\n  {}",
            offenders.join("\n  ")
        );
    }

    /// `with_capacity` arguments under `messages/` that look like bare variables but are **not**
    /// wire counts, each with the reason. Keyed `(path under src/messages/, argument text)`.
    const EXEMPT: &[(&str, &str)] = &[];

    /// The classifier, pinned on synthetic arguments so a regression in it cannot pass silently
    /// as "nothing to flag".
    #[test]
    fn the_scan_flags_a_bare_wire_count_and_passes_a_bounded_one() {
        // Flagged: a bare variable, with or without the cast, however it is wrapped.
        for arg in [
            "count as usize",
            "count",
            "(count as usize)",
            "usize::from(count)",
            "option_count as usize",
            "n as usize",
        ] {
            assert!(uncapped_wire_count(arg), "should flag `{arg}`");
        }
        // Passes: literals, length arithmetic, a constant, and the two bounded forms.
        for arg in [
            "12",
            "name.len() + 1",
            "NPC_TEXT_BLOCKS",
            "QUEST_OBJECTIVES_COUNT as usize",
            "capacity_hint(count, 64)",
            "(count as usize).min(64)",
            "count.min(0xFFFF) as usize",
            "8 + 8 * entries.len()",
            "addons.iter().map(|a| a.name.len() + 10).sum()",
        ] {
            assert!(!uncapped_wire_count(arg), "should pass `{arg}`");
        }
        // The extractor: the balanced argument of every call on the line, with its line number.
        let src = "let a = Vec::with_capacity(count as usize);\nlet b = Vec::with_capacity((n as usize).min(4));\n";
        assert_eq!(
            capacity_arguments(src),
            vec![
                (1, "count as usize".to_string()),
                (2, "(n as usize).min(4)".to_string())
            ]
        );
    }

    /// Is this `with_capacity` argument a raw wire count? A bare identifier (optionally cast
    /// `as usize`, optionally parenthesised, optionally `usize::from(...)`-wrapped) that is not a
    /// `const` (all caps) and carries neither `.min(` nor `capacity_hint(`.
    fn uncapped_wire_count(arg: &str) -> bool {
        if arg.contains(".min(") || arg.contains("capacity_hint(") {
            return false;
        }
        let mut inner = arg.trim();
        loop {
            let t = inner.trim();
            if let Some(rest) = t
                .strip_prefix("usize::from(")
                .and_then(|r| r.strip_suffix(')'))
            {
                inner = rest;
            } else if let Some(rest) = t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
                inner = rest;
            } else if let Some(rest) = t.strip_suffix("as usize") {
                inner = rest;
            } else {
                inner = t;
                break;
            }
        }
        let is_ident = !inner.is_empty()
            && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !inner.starts_with(|c: char| c.is_ascii_digit());
        // A `const` is SCREAMING_CASE; a wire count is a local.
        is_ident && inner.chars().any(|c| c.is_ascii_lowercase())
    }

    /// Every `with_capacity(` call's balanced argument text in `text`, with its 1-based line.
    fn capacity_arguments(text: &str) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        let needle = "with_capacity(";
        let mut from = 0;
        while let Some(at) = text[from..].find(needle) {
            let open = from + at + needle.len();
            let mut depth = 1usize;
            let mut end = open;
            for (i, c) in text[open..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = open + i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let line = text[..open].matches('\n').count() + 1;
            out.push((line, text[open..end].trim().to_string()));
            from = end.max(open);
        }
        out
    }

    /// Every `.rs` file under `root`, recursively.
    fn rust_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        out.sort();
        out
    }
}
