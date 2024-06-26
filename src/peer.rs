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

pub(crate) enum PeerConnectionState {
    Initial,
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