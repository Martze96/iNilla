//! The battleground queue's sends (decision 1963).

use anyhow::Result;

use crate::messages::{self, opcode};

use super::WorldWriter;

impl WorldWriter {
    /// Answer a ready battleground (`CMSG_BATTLEFIELD_PORT`): the slot's map id and whether to
    /// enter — `AcceptBattlefieldPort(index, accept)`'s packet.
    pub fn battlefield_port(&mut self, map_id: u32, accept: bool) -> Result<()> {
        self.send(
            opcode::CMSG_BATTLEFIELD_PORT,
            &messages::battlefield_port(map_id, accept),
        )
    }

    /// Ask for the scoreboard (`MSG_PVP_LOG_DATA`, empty) — `RequestBattlefieldScoreData()`'s
    /// packet; the 5000 ms throttle is the caller's (decision 1972).
    pub fn request_battlefield_score_data(&mut self) -> Result<()> {
        self.send(opcode::MSG_PVP_LOG_DATA, &[])
    }

    /// Leave the battleground (`CMSG_LEAVE_BATTLEFIELD`): `LeaveBattlefield()`'s packet, sent
    /// only once the scoreboard's "ended" byte has arrived (decision 1972).
    pub fn leave_battlefield(&mut self, map_id: u32) -> Result<()> {
        self.send(
            opcode::CMSG_LEAVE_BATTLEFIELD,
            &messages::leave_battlefield(map_id),
        )
    }
}
