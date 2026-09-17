//! The meeting-stone queue's sends (decisions 1963/2283).

use anyhow::Result;

use crate::messages::{self, opcode};

use super::WorldWriter;

impl WorldWriter {
    /// Join the meeting-stone queue (`CMSG 0x292`, `u64 goGuid`) — what a right-click on a
    /// `GAMEOBJECT_TYPE_MEETINGSTONE` sends once that type's own use-slot validator has passed
    /// its four client-side refusals (decision 2283). Never `CMSG_GAMEOBJ_USE`: the server has no
    /// type-23 case for the shared opener.
    pub fn meeting_stone_join(&mut self, go_guid: u64) -> Result<()> {
        self.send(
            opcode::CMSG_MEETINGSTONE_JOIN,
            &messages::meeting_stone_join(go_guid),
        )
    }

    /// Leave the meeting-stone queue (`CMSG 0x293`, empty) — `CancelMeetingStoneRequest()`'s
    /// packet, sent by the party leader (or a player in no party).
    pub fn meeting_stone_leave(&mut self) -> Result<()> {
        self.send(
            opcode::CMSG_MEETINGSTONE_LEAVE,
            &messages::meeting_stone_leave(),
        )
    }

    /// Ask for the meeting-stone status (`CMSG 0x296`, empty) — the enter-world query the
    /// reference sends once per world session (decision 1974).
    pub fn meeting_stone_status_query(&mut self) -> Result<()> {
        self.send(opcode::CMSG_MEETINGSTONE_STATUS_QUERY, &[])
    }
}
