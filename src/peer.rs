use std::cmp;
use std::io::Read;
use std::io::Write;
use std::net::IpAddr;
use std::net::TcpStream;
use std::str::FromStr;
use rand::Rng;

use crate::torrent;
use crate::peer;

fn generate_random_number_string(length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| rng.gen_range(0..10))
        .map(|num| std::char::from_digit(num, 10).unwrap())
        .collect()
}

pub(crate) fn random_peer_id() -> String {
    generate_random_number_string(20)
}

#[derive(PartialEq, Debug)]
pub(crate) enum PeerMessageId {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield = 5,
    Request = 6,
    Piece = 7,
    Cancel = 8
}

impl PeerMessageId {
    fn lookup(value: u8) -> Result<PeerMessageId, anyhow::Error> {
        match value {
            0 => Ok(PeerMessageId::Choke),
            1 => Ok(PeerMessageId::Unchoke),
            2 => Ok(PeerMessageId::Interested),
            3 => Ok(PeerMessageId::NotInterested),
            4 => Ok(PeerMessageId::Have),
            5 => Ok(PeerMessageId::Bitfield),
            6 => Ok(PeerMessageId::Request),
            7 => Ok(PeerMessageId::Piece),
            8 => Ok(PeerMessageId::Cancel),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Unknown peer message id {:?}", value)).into())
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct PeerMessage {
    pub(crate) message_id: PeerMessageId,
    pub(crate) payload: Vec<u8>
}

#[derive(Debug, PartialEq)]
pub(crate) struct PiecePeerMessage {
    pub(crate)  message_id: PeerMessageId,
    pub(crate)  index: usize,
    pub(crate)  begin: usize,
    pub(crate)  block: Vec<u8>
}

impl PeerMessage {
    pub(crate) fn with_id(message_id: PeerMessageId) -> PeerMessage {
        PeerMessage {
            message_id,
            payload: Vec::new()
        }
    }

    pub(crate) fn new(message_id: PeerMessageId, payload: Vec<u8>) -> PeerMessage {
        PeerMessage {
            message_id,
            payload
        }
    }

    pub(crate) fn get_bytes(self) -> Vec<u8> {
        let mut result: Vec<u8> = Vec::new();
        let message_length = 1 + self.payload.len() as u32; // message_id takes one byte
        result.extend(message_length.to_be_bytes());
        result.push(self.message_id as u8);
        result.extend(self.payload);
        return result;
    }

    pub(crate) fn to_string(&self) -> String {
        format!("PeerMessage[{:?}, {:?}]", self.message_id, String::from_utf8(self.payload.clone()))
    }

    pub(crate) fn new_request(index: u32, begin: u32, length: u32) -> PeerMessage {
        let mut payload: Vec<u8> = Vec::new();
        payload.extend(index.to_be_bytes());
        payload.extend(begin.to_be_bytes());
        payload.extend(length.to_be_bytes());
        PeerMessage {
            message_id: PeerMessageId::Request,
            payload
        }
    }

    pub(crate) fn parse_as_piece(&self) -> Result<PiecePeerMessage, anyhow::Error> {
        if self.payload.len() < 8 {
            Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Not enough bytes in the 'piece' message {:?}", self.payload)).into())
        } else {
            let index = as_usize(&self.payload[0..4].try_into()?);
            let begin = as_usize(&self.payload[4..8].try_into()?);
            let block: Vec<u8> = self.payload[8..].to_vec();
            Ok(PiecePeerMessage {
                message_id: PeerMessageId::Piece,
                index,
                begin,
                block
            })
        }
    }
    //TODO: "new_piece"
    //TODO: "new_cancel"
}

fn as_usize(bytes: &[u8; 4]) -> usize {
    (bytes[0] as usize) << 24| (bytes[1] as usize) << 16 | (bytes[2] as usize) << 8 | (bytes[3] as usize)
}

#[derive(PartialEq, Clone)]
pub(crate) struct PeerConnectionState {
    pub(crate) choked: PeerChokedState,
    pub(crate) interested: PeerInterestedState
}

impl PeerConnectionState {
    pub(crate) fn initial() -> PeerConnectionState {
        PeerConnectionState {
            choked: PeerChokedState::Choked,
            interested: PeerInterestedState::NotInterested
        }
    }

    pub(crate) fn update_choked(&self, choked: PeerChokedState) -> PeerConnectionState {
        PeerConnectionState {
            choked,
            interested: self.interested
        }
    }

    pub(crate) fn update_interested(&self, interested: PeerInterestedState) -> PeerConnectionState {
        PeerConnectionState {
            choked: self.choked,
            interested
        }
    }
}

pub(crate) struct Piece {
    pub(crate) index: u32,
    pub(crate) piece_length: u32
}

impl Piece {
    pub(crate) fn get_blocks(&self, piece_length: u32) -> Vec<PieceBlock> {
        let block_size_bytes = 16 * 1024; // 2^14
        let mut current_begin_in_piece: u32 = 0;
        let mut piece_blocks = Vec::new();
        while current_begin_in_piece < piece_length {
            let current_block_length = cmp::min(block_size_bytes, piece_length - current_begin_in_piece);
            piece_blocks.push(PieceBlock {
                begin: current_begin_in_piece,
                length: current_block_length
            });
            current_begin_in_piece = current_begin_in_piece + current_block_length;
        }
        piece_blocks
    }
}


#[derive(Debug)]
pub(crate) struct PieceBlock {
    pub(crate) begin: u32,
    pub(crate) length: u32
}

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum PeerChokedState {
    Choked,
    Unchoked
}

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum PeerInterestedState {
    Interested,
    NotInterested
}

#[derive(Debug)]
pub(crate) struct PeerAddress {
    pub(crate) address: IpAddr,
    pub(crate) port: u16
}

impl PeerAddress {
    pub(crate) fn from_str(input: &str) -> Result<PeerAddress, anyhow::Error> {
        let (address_input, port_input) = input.split_once(':')
            .ok_or(std::io::Error::new(std::io::ErrorKind::Other, format!("Could not create peer address from {:?}", input)))?;
        let address = IpAddr::from_str(address_input)?;
        let port = port_input.parse::<u16>()?;
        Ok(PeerAddress { address, port })
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(crate) struct Peer {
    pub(crate) id: Vec<u8>
}

impl Peer {

    pub(crate) fn handshake_for_peer(peer_address: &PeerAddress, torrent_info: &torrent::TorrentInfo, current_peer_id: &str) -> Result<(PeerHandshake, TcpStream), anyhow::Error> {
        let torrent_hash = torrent_info.compute_hash();
        let current_peer_handshake = peer::PeerHandshake {
            info_hash: torrent_hash,
            peer: peer::Peer {
                id: current_peer_id.as_bytes().to_vec()
            }
        };
        Peer::handshake(peer_address, &current_peer_handshake)
    }

    fn handshake(peer_address: &PeerAddress, request: &PeerHandshake) -> Result<(PeerHandshake, TcpStream), anyhow::Error> {
        let mut stream = TcpStream::connect(format!("{}:{}", peer_address.address.to_string(), peer_address.port.to_string()))?;
        stream.write_all(&request.get_bytes())?;

        let mut response_buffer: [u8; 68] = [0; 68]; //1 + 19 + 8 + 20 + 20
        stream.read(&mut response_buffer)?;

        let info_hash: Vec<u8> = response_buffer[28..48].to_vec();
        let peer_id = response_buffer[48..].to_vec();
        Ok((PeerHandshake {
            info_hash,
            peer: Peer { id: peer_id }
        }, stream))
    }

    pub(crate) fn read_message(stream: &mut impl Read) -> Result<Option<PeerMessage>, anyhow::Error> {
        let mut message_length_buffer: [u8; 4] = [0u8; 4];
        let read_bytes = stream.read(&mut message_length_buffer)?;

        if read_bytes < 4 {
            //println!("Peer did not send a message");
            if read_bytes > 0 {
                println!("Received too few bytes from the peer: {} bytes", read_bytes)
            }
            Ok(None)
        } else {
            let message_length = u32::from_be_bytes(message_length_buffer) - 1;

            let mut message_id_buffer: [u8; 1] = [0u8; 1];
            stream.read_exact(&mut message_id_buffer)?;
            let message_type = message_id_buffer[0];

            let mut message_content: Vec<u8> = vec![0u8; message_length as usize];
            if message_length > 0 {
                stream.read_exact(&mut message_content)?;
            }
            Ok(Some(PeerMessage {
                message_id: PeerMessageId::lookup(message_type)?,
                payload: message_content
            }))
        }
    }
}

pub(crate) struct PeerHandshake {
    pub(crate) info_hash: Vec<u8>,
    pub(crate) peer: Peer
}

impl PeerHandshake {
    pub(crate) fn get_bytes(&self) -> Vec<u8> {
        let mut message: Vec<u8> = Vec::new();
        message.push(19);
        message.extend_from_slice("BitTorrent protocol".as_bytes());
        message.extend_from_slice(&[0; 8]);
        message.extend_from_slice(&self.info_hash);
        message.extend_from_slice(&self.peer.id);
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Read};

    struct InMemoryTcpStream {
        bytes: Vec<u8>,
        length: usize,
        cursor: usize,
    }

    impl InMemoryTcpStream {
        fn from_bytes (bytes: Vec<u8>) -> Self {
            let length = bytes.len();
            InMemoryTcpStream { bytes, length, cursor: 0 }
        }
    }

    impl Read for InMemoryTcpStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let bytes_to_read = buf.len().min(self.length - self.cursor);
            if bytes_to_read > 0 {
                buf[0..bytes_to_read].copy_from_slice(&self.bytes[self.cursor..(self.cursor + bytes_to_read)]);
                self.cursor = self.cursor + bytes_to_read;
            }
            Ok(bytes_to_read)
        }
    }

    #[test]
    fn should_read_bitfield_message_from_peer() {
        let mut stream = InMemoryTcpStream::from_bytes(vec![
            0, 0, 0, 4,   // message length prefix - 4
            5,             // message id byte - 5 “bitfield”
            255, 248, 128
        ]);
        let message = Peer::read_message(&mut stream).unwrap();
        assert_eq!(message, Some(PeerMessage::new(PeerMessageId::Bitfield, vec![255, 248, 128])))
    }

    #[test]
    fn should_read_unchoke_message_from_peer() {
        let mut stream = InMemoryTcpStream::from_bytes(vec![
            0, 0, 0, 1,   // message length prefix - 1
            1,            // message id byte - 1 “unchoke”
        ]);
        let message = Peer::read_message(&mut stream).unwrap();
        assert_eq!(message, Some(PeerMessage::new(PeerMessageId::Unchoke, Vec::new())))
    }

    #[test]
    fn should_read_piece_message_from_peer() {
        let mut stream = InMemoryTcpStream::from_bytes(vec![
            0, 0, 0, 21,    // message length prefix - 1
            7,             // message id byte - 1 “unchoke”
            0, 0, 0, 1,    // piece index - 1
            0, 2, 128, 0,  // begin - 163840
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12 // piece block content
        ]);
        let message = Peer::read_message(&mut stream).unwrap().unwrap();
        let piece_message = message.parse_as_piece().unwrap();
        assert_eq!(piece_message, PiecePeerMessage {
            message_id: PeerMessageId::Piece,
            index: 1,
            begin: 163840,
            block: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
        })
    }

    #[test]
    fn should_serialize_peer_handshake_message_correctly() {
        let peer_handshake = PeerHandshake {
            info_hash: vec![1, 2, 3, 4],
            peer: Peer {
                id: vec![5, 6, 7, 8]
            }
        };
        assert_eq!(peer_handshake.get_bytes(), vec![
            19, // length of the protocol string which follows - 19
            66, 105, 116, 84, 111, 114, 114, 101, 110, 116, 32, 112, 114, 111, 116, 111, 99, 111, 108, // protocol string:"BitTorrent protocol"
            0, 0, 0, 0, 0, 0, 0, 0, // reserved 8 bytes
            1, 2, 3, 4, // info_hash bytes
            5, 6, 7, 8 // peer id bytes
        ]);
    }

    #[test]
    fn should_serialize_request_message_correctly() {
        let request = PeerMessage::new_request(11, 163840, 16384);
        assert_eq!(request.get_bytes(), vec![
            0, 0, 0, 13,   // message length prefix - 13
            6,             // message id byte - 6 “request”
            0, 0, 0, 11,   // piece index - 11
            0, 2, 128, 0,  // begin - 163840
            0, 0, 64, 0    // length - 16384
        ]);
    }

    //TODO: parse_as_piece
    //TODO: PeerAddress::from_str
    //TODO: Piece::get_blocks
}