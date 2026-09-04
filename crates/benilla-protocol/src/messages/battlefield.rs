//! The battleground queue's wire (decision 1963; wow-re `staticpopup-dialog-bindings.md` §7):
//! the server's per-slot status and the port answer `AcceptBattlefieldPort` sends. Three queue
//! slots exist in the client (`0xb6e9d0`, stride `0x20`), addressed 1-based from Lua.

use std::io::{self, Read};

use crate::wire::{read_u32_le, read_u8};

/// `SMSG_BATTLEFIELD_STATUS`, one slot's update (VERIFIED at the bytes, handler `0x4aa850`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BattlefieldStatus {
    /// Which of the three slots, 0-based on the wire.
    pub slot: u32,
    /// The battleground's Map.dbc row id; zero clears the slot.
    pub map_id: u32,
    pub bracket: u8,
    /// Read into `+0x10`; no Lua reader was found for it (parked by the carve).
    pub unknown: u32,
    pub status: u32,
    /// Status 2: the queue wait so far, in ms (`+0x14 = now + v`).
    pub time_ms: Option<u32>,
    /// Status 3: the two dwords the client parks beside the selected battlefield.
    pub in_progress: Option<(u32, u32)>,
}

/// Parse `SMSG_BATTLEFIELD_STATUS`: `u32 slot`, `u32 mapId`, and — only for a non-zero map —
/// `u8 bracket`, `u32`, `u32 status`, then the status-conditional tail.
pub(super) fn read_battlefield_status(r: &mut impl Read) -> io::Result<BattlefieldStatus> {
    let slot = read_u32_le(r)?;
    let map_id = read_u32_le(r)?;
    if map_id == 0 {
        return Ok(BattlefieldStatus {
            slot,
            map_id,
            bracket: 0,
            unknown: 0,
            status: 0,
            time_ms: None,
            in_progress: None,
        });
    }
    let bracket = read_u8(r)?;
    let unknown = read_u32_le(r)?;
    let status = read_u32_le(r)?;
    let time_ms = if status == 2 {
        Some(read_u32_le(r)?)
    } else {
        None
    };
    let in_progress = if status == 3 {
        Some((read_u32_le(r)?, read_u32_le(r)?))
    } else {
        None
    };
    Ok(BattlefieldStatus {
        slot,
        map_id,
        bracket,
        unknown,
        status,
        time_ms,
        in_progress,
    })
}

/// Body of `CMSG_BATTLEFIELD_PORT` (VERIFIED, `0x4ab3b0`): `u32 mapId` then a genuinely
/// one-byte `accept`, normalised to 0/1 before it reaches the wire.
pub fn battlefield_port(map_id: u32, accept: bool) -> Vec<u8> {
    let mut body = map_id.to_le_bytes().to_vec();
    body.push(u8::from(accept));
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reads_the_conditional_tails() {
        let mut body = vec![
            1u8, 0, 0, 0, 30, 0, 0, 0, 5, 7, 0, 0, 0, 2, 0, 0, 0, 100, 0, 0, 0,
        ];
        let s = read_battlefield_status(&mut body.as_slice()).unwrap();
        assert_eq!(
            (s.slot, s.map_id, s.bracket, s.unknown, s.status),
            (1, 30, 5, 7, 2)
        );
        assert_eq!(s.time_ms, Some(100));
        body = vec![0u8, 0, 0, 0, 0, 0, 0, 0];
        let s = read_battlefield_status(&mut body.as_slice()).unwrap();
        assert_eq!(
            s.map_id, 0,
            "a zero map clears the slot and ends the packet"
        );
    }

    #[test]
    fn port_is_a_map_id_and_one_byte() {
        assert_eq!(battlefield_port(489, true), vec![0xE9, 1, 0, 0, 1]);
        assert_eq!(battlefield_port(489, false), vec![0xE9, 1, 0, 0, 0]);
    }
}
