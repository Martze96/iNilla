//! iNilla: **a localized client's text, where the readers look for it.**
//!
//! Every localized string in a 1.12 DBC is a block of nine fields: one string per client locale
//! (`enUS, koKR, frFR, deDE, zhCN, zhTW, esES, esMX`) and a flags word. A client fills only its own
//! language's column, so a German client's `Spell.dbc` has its names in the fourth column of each
//! block and nothing in the first. This workspace's readers name the first (`COL_NAME_ENUS`, …),
//! which on a German client reads empty strings everywhere.
//!
//! [`data_locale`] reads an install's language off `ChrRaces.dbc`, whose race names sit at a known
//! place (fields 17–24, flags 25), the way vmangos tells it (`World::DetectDBCLang`). For any
//! language but English, [`localize`] then finds each table's blocks and copies that language's
//! column into the first, so every reader gets the client's own text without knowing about
//! locales. An English install is never touched: it is the one the readers were written for, and a
//! run of unused integer fields before a name block can look like a block of its own.

/// The eight 1.12 client locales, in their DBC column order (vmangos' `LocaleConstant`).
pub const LOCALES: [&str; 8] = [
    "enUS", "koKR", "frFR", "deDE", "zhCN", "zhTW", "esES", "esMX",
];

/// `ChrRaces.dbc`'s race-name block: its first field.
const CHR_RACES_NAME: usize = 17;

/// A parsed WDBC whose fields are all 32-bit: `(records, fields, string block, field reader)`.
struct Table<'a> {
    bytes: &'a [u8],
    records: usize,
    fields: usize,
    strings: &'a [u8],
}

impl<'a> Table<'a> {
    fn new(bytes: &'a [u8]) -> Option<Self> {
        let u32_at = |off: usize| -> Option<usize> {
            bytes
                .get(off..off + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        };
        if bytes.len() < 20 || &bytes[0..4] != b"WDBC" {
            return None;
        }
        let (records, fields, record_size, string_size) =
            (u32_at(4)?, u32_at(8)?, u32_at(12)?, u32_at(16)?);
        if records == 0 || record_size != fields.checked_mul(4)? {
            return None;
        }
        let strings_at = 20usize.checked_add(records.checked_mul(record_size)?)?;
        let strings = bytes.get(strings_at..strings_at.checked_add(string_size)?)?;
        Some(Table {
            bytes,
            records,
            fields,
            strings,
        })
    }

    fn at(&self, r: usize, c: usize) -> usize {
        let o = 20 + (r * self.fields + c) * 4;
        u32::from_le_bytes([
            self.bytes[o],
            self.bytes[o + 1],
            self.bytes[o + 2],
            self.bytes[o + 3],
        ]) as usize
    }

    /// A non-empty string starts at `v`: inside the block, just past a NUL, not itself a NUL.
    fn is_string(&self, v: usize) -> bool {
        v != 0 && v < self.strings.len() && self.strings[v - 1] == 0 && self.strings[v] != 0
    }
}

/// The locale index (into [`LOCALES`]) of the install a `ChrRaces.dbc` came from: the one column
/// of the Human's name that is filled. `None` when the file is not the 1.12 table.
pub fn data_locale(chr_races: &[u8]) -> Option<usize> {
    let t = Table::new(chr_races)?;
    if t.fields < CHR_RACES_NAME + 9 {
        return None;
    }
    (0..8).find(|&k| t.is_string(t.at(0, CHR_RACES_NAME + k)))
}

/// `bytes` with every localized block's text moved from locale `loc`'s column into the first;
/// `None` when there is nothing to move (no block, not a DBC, or `loc` is English's own column).
///
/// A block is recognized where, in every record, the first column and the other six languages'
/// columns are empty, `loc`'s column holds only strings (and at least one), and the ninth field is
/// a flags word — set, and not a string.
pub fn localize(bytes: &[u8], loc: usize) -> Option<Vec<u8>> {
    if loc == 0 || loc >= 8 {
        return None;
    }
    let t = Table::new(bytes)?;
    if t.fields < 9 {
        return None;
    }
    let mut empty = vec![true; t.fields];
    let mut strings = vec![true; t.fields];
    for r in 0..t.records {
        for c in 0..t.fields {
            let v = t.at(r, c);
            if v != 0 {
                empty[c] = false;
                if !t.is_string(v) {
                    strings[c] = false;
                }
            }
        }
    }
    let blocks: Vec<usize> = (0..=t.fields - 9)
        .filter(|&c| {
            (0..8).all(|k| {
                if k == loc {
                    !empty[c + k] && strings[c + k]
                } else {
                    empty[c + k]
                }
            }) && !empty[c + 8]
                && !strings[c + 8]
        })
        .collect();
    if blocks.is_empty() {
        return None;
    }
    let mut out = bytes.to_vec();
    for r in 0..t.records {
        for &c in &blocks {
            let from = 20 + (r * t.fields + c + loc) * 4;
            let to = 20 + (r * t.fields + c) * 4;
            let v = [out[from], out[from + 1], out[from + 2], out[from + 3]];
            out[to..to + 4].copy_from_slice(&v);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DBC of `rows` of u32 fields and a string block.
    fn dbc(rows: &[Vec<u32>], strings: &[u8]) -> Vec<u8> {
        let fields = rows[0].len() as u32;
        let mut b = b"WDBC".to_vec();
        for v in [rows.len() as u32, fields, fields * 4, strings.len() as u32] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for r in rows {
            for v in r {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(strings);
        b
    }

    fn field(b: &[u8], row: usize, col: usize, fields: usize) -> u32 {
        let o = 20 + row * fields * 4 + col * 4;
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }

    // "\0Mensch\0Zwerg\0" — strings at 1 and 8.
    const TEXT: &[u8] = b"\0Mensch\0Zwerg\0";
    const FLAGS: u32 = 0x007F_007F;
    const DE: usize = 3;

    #[test]
    fn a_german_block_moves_into_the_first_column() {
        // ID, a name block with the German text in its fourth column, the flags, an int.
        let rows = vec![
            vec![1, 0, 0, 0, 1, 0, 0, 0, 0, FLAGS, 42],
            vec![2, 0, 0, 0, 8, 0, 0, 0, 0, FLAGS, 43],
        ];
        let out = localize(&dbc(&rows, TEXT), DE).unwrap();
        assert_eq!((field(&out, 0, 1, 11), field(&out, 1, 1, 11)), (1, 8));
        assert_eq!(field(&out, 0, 0, 11), 1, "the ID stays");
        assert_eq!(field(&out, 1, 10, 11), 43, "the int stays");
        assert_eq!(field(&out, 0, 4, 11), 1, "the German column stays too");
    }

    #[test]
    fn unused_integers_before_a_name_are_not_a_block() {
        // ItemRandomProperties' shape: enchant fields that are 0 in every row, then a name block
        // filled in some other column. Without a flags word at the ninth field, it is no block.
        let rows = vec![
            vec![1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, FLAGS],
            vec![2, 0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, FLAGS],
        ];
        assert_eq!(localize(&dbc(&rows, TEXT), 4), None);
        assert_eq!(localize(&dbc(&rows, TEXT), 7), None);
    }

    #[test]
    fn integers_that_look_like_offsets_are_not_strings() {
        // 2 and 3 land inside "Mensch", not at the start of a string.
        let rows = vec![
            vec![1, 0, 0, 0, 2, 0, 0, 0, 0, FLAGS, 42],
            vec![2, 0, 0, 0, 3, 0, 0, 0, 0, FLAGS, 43],
        ];
        assert_eq!(localize(&dbc(&rows, TEXT), DE), None);
    }

    #[test]
    fn english_and_other_files_are_left_alone() {
        let rows = vec![vec![1, 0, 0, 0, 1, 0, 0, 0, 0, FLAGS, 42]];
        assert_eq!(localize(&dbc(&rows, TEXT), 0), None);
        assert_eq!(localize(b"not a dbc", DE), None);
        assert_eq!(localize(&dbc(&[vec![1, 1]], TEXT), DE), None);
    }

    #[test]
    fn the_install_language_comes_from_the_race_names() {
        let mut row = vec![0u32; 29];
        row[0] = 1;
        row[CHR_RACES_NAME + DE] = 1;
        row[CHR_RACES_NAME + 8] = FLAGS;
        assert_eq!(data_locale(&dbc(&[row.clone()], TEXT)), Some(DE));
        row[CHR_RACES_NAME + DE] = 0;
        row[CHR_RACES_NAME] = 8;
        assert_eq!(data_locale(&dbc(&[row], TEXT)), Some(0));
        assert_eq!(data_locale(b"nope"), None);
    }
}
