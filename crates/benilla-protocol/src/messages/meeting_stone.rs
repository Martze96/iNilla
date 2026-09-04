//! The meeting-stone queue's wire (decision 1963; wow-re `staticpopup-dialog-bindings.md` §8):
//! the server's queue state and the leave request `CancelMeetingStoneRequest` sends.

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

/// Body of `CMSG 0x293` (VERIFIED, `0x4ca120`): empty.
pub fn meeting_stone_leave() -> Vec<u8> {
    Vec::new()
}
