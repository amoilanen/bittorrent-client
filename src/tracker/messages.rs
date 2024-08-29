// Following https://www.bittorrent.org/beps/bep_0015.html
use crate::error::new_error;
use anyhow::ensure;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Action {
    Connect = 0,
    Announce = 1,
    Scrape = 2,
    Error = 3
}

impl Action {
    fn from(action_id: u32) -> Result<Action, anyhow::Error> {
        match action_id {
            0 => Ok(Action::Connect),
            1 => Ok(Action::Announce),
            2 => Ok(Action::Scrape),
            3 => Ok(Action::Error),
            _ => Err(new_error(format!("Action with the id {} does not exist", action_id)))
        }
    }
}

#[derive(Debug)]
pub(crate) struct ConnectRequest {
    pub(crate) protocol_id: u64,
    pub(crate) action: Action,
    pub(crate) transaction_id: u32
}

impl ConnectRequest {

    pub(crate) fn new(transaction_id: u32) -> ConnectRequest {
        ConnectRequest {
            protocol_id: 0x41727101980u64,
            action: Action::Connect,
            transaction_id
        }
    }

    pub(crate) fn get_bytes(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend(self.protocol_id.to_be_bytes().to_vec());
        bytes.extend((self.action as u32).to_be_bytes().to_vec());
        bytes.extend(self.transaction_id.to_be_bytes().to_vec());
        bytes
    }
}

#[derive(Debug)]
pub(crate) struct ConnectResponse {
    pub(crate) action: Action,
    pub(crate) transaction_id: u32,
    pub(crate) connection_id: u64
}

impl ConnectResponse {
    pub(crate) fn parse(bytes: &[u8]) -> Result<ConnectResponse, anyhow::Error> {
        let message_bytes: [u8; 16] = bytes[0..16].try_into()?;
        let action: Action = Action::from(u32::from_be_bytes(message_bytes[0..4].try_into()?))?;
        ensure!(action == Action::Connect, "Expected 'connect' action 0 but got {:?}", action);
        let transaction_id: u32 = u32::from_be_bytes(message_bytes[4..8].try_into()?);
        let connection_id: u64 = u64::from_be_bytes(message_bytes[8..16].try_into()?);
        Ok(ConnectResponse {
            action,
            transaction_id,
            connection_id
        })
    }
}
