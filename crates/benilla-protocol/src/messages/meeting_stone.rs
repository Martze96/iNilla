//! The meeting-stone queue's wire (decisions 1963/1974/2283; wow-re `staticpopup-dialog-bindings.md`
//! §8 and `meeting-stone-status.md` §8/§9/§10): the server's queue state, the JOIN a right-click on
//! the stone sends, and the leave request `CancelMeetingStoneRequest` sends.

use std::io::{self, Read};

use crate::wire::{read_u32_le, read_u8};

/// `SMSG 0x295` (VERIFIED at the bytes, handler `0x4ca230`): the queued area and a status byte
/// the client turns into one of five local messages, then `MEETINGSTONE_CHANGED`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeetingStoneSetQueue {
    pub area: u32,
    pub status: u8,
}

/// Parse it: `u32 areaId`, `u8 status`.
pub(super) fn read_meeting_stone_set_queue(r: &mut impl Read) -> io::Result<MeetingStoneSetQueue> {
    Ok(MeetingStoneSetQueue {
        area: read_u32_le(r)?,
        status: read_u8(r)?,
    })
}

/// Body of `CMSG 0x292` (VERIFIED, builder `0x4c9ff0`): one full little-endian `u64` — the guid of
/// the `GAMEOBJECT_TYPE_MEETINGSTONE` (23) the player right-clicked, written by the 8-byte guid
/// writer `0x418370` straight after `PutUInt32(0x292)`. Nothing else is in the packet: the area is
/// the SERVER's to resolve from the stone's `gameobject_template.data[2]`, which is why the join
/// names an object and the reply names an area.
pub fn meeting_stone_join(go_guid: u64) -> Vec<u8> {
    go_guid.to_le_bytes().to_vec()
}

/// Body of `CMSG 0x293` (VERIFIED, `0x4ca120`): empty.
pub fn meeting_stone_leave() -> Vec<u8> {
    Vec::new()
}

/// The four display-only replies one handler serves (`0x4ca3c0`, dispatched on the opcode; wow-re
/// `meeting-stone-status.md` §9, 1974). None touches the queue state, none fires an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeetingStoneNotice {
    /// `0x297`, empty.
    Success,
    /// `0x298`, empty.
    InProgress,
    /// `0x299`, `u64 guid` — the line waits for the name cache.
    MemberAdded { guid: u64 },
    /// `0x2BB`, `u8 code`.
    JoinFailed { code: u8 },
}

/// Parse `0x299`'s guid.
pub(super) fn read_meeting_stone_member_added(r: &mut impl Read) -> io::Result<MeetingStoneNotice> {
    Ok(MeetingStoneNotice::MemberAdded {
        guid: crate::wire::read_u64_le(r)?,
    })
}

/// Parse `0x2BB`'s code byte.
pub(super) fn read_meeting_stone_join_failed(r: &mut impl Read) -> io::Result<MeetingStoneNotice> {
    Ok(MeetingStoneNotice::JoinFailed { code: read_u8(r)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The join body is the bare guid, little-endian, eight bytes and no more — `0x4c9ff0` writes
    /// `PutUInt32(0x292)` into the header and then exactly one `0x418370` (the 8-byte guid writer).
    #[test]
    fn cmsg_meetingstone_join_body_golden() {
        assert_eq!(
            meeting_stone_join(0x1234_5678_9abc_def0),
            vec![0xf0, 0xde, 0xbc, 0x9a, 0x78, 0x56, 0x34, 0x12],
            "CMSG_MEETINGSTONE_JOIN body"
        );
    }

    /// Both of the other two sends in this family are empty bodies.
    #[test]
    fn the_leave_body_is_empty() {
        assert!(meeting_stone_leave().is_empty());
    }
}
