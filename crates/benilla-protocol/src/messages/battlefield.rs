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
    /// Status 2: the port deadline's delta, in ms (`+0x14 = now + Δ`, `GetBattlefieldPortExpiration`).
    pub time_ms: Option<u32>,
    /// Status 3 (`battlefield-verb-family.md` §4.2, 1972): `(Δ₁, Δ₂)` — the instance's expiration
    /// delta (`[0xb6ebb8] = now + Δ₁`, `GetBattlefieldInstanceExpiration`) and its elapsed run time
    /// (`[0xb6ebbc] = now − Δ₂`, `GetBattlefieldInstanceRunTime`).
    pub in_progress: Option<(u32, u32)>,
    /// Status 1 (§4.2): `(estimated wait ms, raw; Δ waited)` — `[slot+0x18]` and
    /// `[slot+0x1c] = now − Δ`, the `GetBattlefieldEstimatedWaitTime`/`GetBattlefieldTimeWaited` pair.
    /// 1963's reader dropped this tail; the reference reads it (1972).
    pub queued: Option<(u32, u32)>,
}

/// Parse `SMSG_BATTLEFIELD_STATUS`: `u32 slot`, `u32 mapId`, and — only for a non-zero map —
/// `u8 bracket`, `u32`, `u32 status`, then the status-conditional tail: two `u32` for status 1,
/// one for status 2, two for status 3.
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
            queued: None,
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
    let queued = if status == 1 {
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
        queued,
    })
}

/// One scoreboard row of `MSG_PVP_LOG_DATA` (VERIFIED at the bytes, handler `0x4aab30`; wow-re
/// `battlefield-verb-family.md` §2.3/§4.3, 1972). The client's 0x40-byte block, wire order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PvpLogRow {
    pub guid: u64,
    /// Wire field 2 — read before the kills, stored at `+0x1c`.
    pub rank: u32,
    pub killing_blows: u32,
    pub honorable_kills: u32,
    pub deaths: u32,
    pub honor_gained: u32,
    /// The extra-stat dwords, at most eight stored — the client consumes and discards the rest.
    pub stats: Vec<u32>,
}

/// `MSG_PVP_LOG_DATA` inbound: the whole scoreboard, rows in wire order (nothing here sorts).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PvpLogData {
    /// `u8 != 0` — the battleground has ended; `LeaveBattlefield` sends nothing until this is set.
    pub ended: bool,
    /// Read only when `ended`: `0` = Horde, `1` = Alliance (`GetBattlefieldWinner`).
    pub winner: Option<u8>,
    pub rows: Vec<PvpLogRow>,
}

/// Parse `MSG_PVP_LOG_DATA` (§4.3): `u8 ended`, `u8 winner` iff ended, `u32 count`, `count` rows of
/// `u64 guid, u32 rank, u32 kb, u32 hk, u32 deaths, u32 honor, u32 statCount, statCount × u32`.
/// The client stores no more than eight stats per row and clamps nothing else — a count past its
/// 80 blocks is its own anomaly (§10); ours keeps every row the wire carries.
pub(super) fn read_pvp_log_data(r: &mut impl Read) -> io::Result<PvpLogData> {
    let ended = read_u8(r)? != 0;
    let winner = if ended { Some(read_u8(r)?) } else { None };
    let count = read_u32_le(r)?;
    let mut rows = Vec::with_capacity(count.min(80) as usize);
    for _ in 0..count {
        let guid = crate::wire::read_u64_le(r)?;
        let rank = read_u32_le(r)?;
        let killing_blows = read_u32_le(r)?;
        let honorable_kills = read_u32_le(r)?;
        let deaths = read_u32_le(r)?;
        let honor_gained = read_u32_le(r)?;
        let stat_count = read_u32_le(r)?;
        let mut stats = Vec::with_capacity(stat_count.min(8) as usize);
        for i in 0..stat_count {
            let v = read_u32_le(r)?;
            if i < 8 {
                stats.push(v);
            }
        }
        rows.push(PvpLogRow {
            guid,
            rank,
            killing_blows,
            honorable_kills,
            deaths,
            honor_gained,
            stats,
        });
    }
    Ok(PvpLogData {
        ended,
        winner,
        rows,
    })
}

/// Body of `CMSG_LEAVE_BATTLEFIELD` (VERIFIED, `0x4abe60`): `u32 mapId` — the active slot's map,
/// or the literal 0 when no slot is active.
pub fn leave_battlefield(map_id: u32) -> Vec<u8> {
    map_id.to_le_bytes().to_vec()
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

#[cfg(test)]
mod pvp_log_tests {
    use super::*;

    /// The scoreboard reader: the winner byte only when ended, the rank BEFORE the kills, and no
    /// more than eight stats kept while every one is consumed.
    #[test]
    fn the_scoreboard_reads_its_conditional_winner_and_keeps_eight_stats() {
        let mut body = vec![1u8, 0u8, 1, 0, 0, 0];
        body.extend_from_slice(&7u64.to_le_bytes());
        for v in [5u32, 3, 9, 2, 120, 9] {
            body.extend_from_slice(&v.to_le_bytes());
        }
        for v in 1u32..=9 {
            body.extend_from_slice(&v.to_le_bytes());
        }
        let d = read_pvp_log_data(&mut body.as_slice()).unwrap();
        assert!(d.ended);
        assert_eq!(d.winner, Some(0));
        assert_eq!(d.rows.len(), 1);
        let r = &d.rows[0];
        assert_eq!(
            (
                r.guid,
                r.rank,
                r.killing_blows,
                r.honorable_kills,
                r.deaths,
                r.honor_gained
            ),
            (7, 5, 3, 9, 2, 120)
        );
        assert_eq!(r.stats, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        let body = vec![0u8, 0, 0, 0, 0];
        let d = read_pvp_log_data(&mut body.as_slice()).unwrap();
        assert!(!d.ended && d.winner.is_none() && d.rows.is_empty());
        assert_eq!(leave_battlefield(489), vec![0xE9, 1, 0, 0]);
    }

    /// Status 1 carries the two wait dwords 1963's reader dropped.
    #[test]
    fn a_queued_status_carries_its_wait_pair() {
        let mut body = vec![0u8, 0, 0, 0];
        body.extend_from_slice(&489u32.to_le_bytes());
        body.push(3);
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(&1u32.to_le_bytes());
        body.extend_from_slice(&30000u32.to_le_bytes());
        body.extend_from_slice(&5000u32.to_le_bytes());
        let s = read_battlefield_status(&mut body.as_slice()).unwrap();
        assert_eq!(s.status, 1);
        assert_eq!(s.queued, Some((30000, 5000)));
        assert!(s.time_ms.is_none() && s.in_progress.is_none());
    }
}
