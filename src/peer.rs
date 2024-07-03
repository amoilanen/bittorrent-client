use std::io::Read;
use std::io::Write;
use std::net::IpAddr;
use std::net::TcpStream;
use std::str::FromStr;
use rand::Rng;

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

#[derive(PartialEq)]
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

pub(crate) struct PeerMessage {
    pub(crate) message_id: PeerMessageId,
    pub(crate) payload: Vec<u8>
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
        let message_length = 1 + self.payload.len(); // message_id takes one byte
        result.extend((message_length as u32).to_be_bytes());
        result.push(self.message_id as u8);
        result.extend(self.payload);
        return result;
    }
}

#[derive(PartialEq, Clone)]
pub(crate) enum PeerConnectionState {
    Initial,
    ReceivedBitfield,
    Interested,
    Choked,
    Unchoked
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
    pub(crate) fn handshake(peer_address: &PeerAddress, request: &PeerHandshake) -> Result<(PeerHandshake, TcpStream), anyhow::Error> {
        let mut stream = TcpStream::connect(format!("{}:{}", peer_address.address.to_string(), peer_address.port.to_string()))?;
        stream.write_all(&request.to_message())?;

        let mut response_buffer: [u8; 68] = [0; 68]; //1 + 19 + 8 + 20 + 20
        stream.read(&mut response_buffer)?;

        let info_hash: Vec<u8> = response_buffer[28..48].to_vec();
        let peer_id = response_buffer[48..].to_vec();
        Ok((PeerHandshake {
            info_hash,
            peer: Peer { id: peer_id }
        }, stream))
    }

    pub(crate) fn read_message(stream: &mut TcpStream) -> Result<PeerMessage, anyhow::Error> {
        let mut buffer: [u8; 512] = [0; 512];
        let mut total_bytes_read: usize = 0;
        let mut message_content: Vec<u8> = Vec::new();
        let mut message_length: usize = 0;
        let mut message_type: u8 = 0;

        while message_length == 0 || total_bytes_read < message_length + 4 { //size of the length prefix
            let bytes_read = stream.read(&mut buffer)?;
            if total_bytes_read == 0 {
                if bytes_read < 5 {
                    continue;
                }
                message_length = (buffer[0] as usize) << 24| (buffer[1] as usize) << 16 | (buffer[2] as usize) << 8 | (buffer[3] as usize);
                message_type = buffer[4];
            }
            message_content.extend(buffer[4..bytes_read].to_vec());
            //println!("bytes_read = {}, total_bytes_read = {}, message_length_bytes = {}, message_type = {}", bytes_read, total_bytes_read, message_length_bytes, message_type);
            total_bytes_read = total_bytes_read + bytes_read;
        }
        Ok(PeerMessage {
            message_id: PeerMessageId::lookup(message_type)?,
            payload: message_content
        })
    }
}

pub(crate) struct PeerHandshake {
    pub(crate) info_hash: Vec<u8>,
    pub(crate) peer: Peer
}

impl PeerHandshake {
    pub(crate) fn to_message(&self) -> Vec<u8> {
        let mut message: Vec<u8> = Vec::new();
        message.push(19);
        message.extend_from_slice("BitTorrent protocol".as_bytes());
        message.extend_from_slice(&[0; 8]);
        message.extend_from_slice(&self.info_hash);
        message.extend_from_slice(&self.peer.id);
        message
    }
}