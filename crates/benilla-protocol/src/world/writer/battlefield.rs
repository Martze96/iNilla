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
}
