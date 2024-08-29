use std::net::UdpSocket;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs, SocketAddr};
use std::time::Duration;
use anyhow::ensure;
use crate::bencoded;
use crate::torrent;
use crate::tracker;
use crate::peer;
use crate::url_utils;
use crate::error::new_error;
use messages::ConnectRequest;
use messages::ConnectResponse;
use url::Url;

mod messages;

pub(crate) struct TrackerResponse {
    interval: u32,
    peers: Vec<u8>
}

pub(crate) struct Tracker {
    pub(crate) url: String
}

pub(crate) struct TrackerRequest {
    pub(crate) peer_id: String,
    pub(crate) info_hash: String,
    pub(crate) port: usize,
    pub(crate) uploaded: u64,
    pub(crate) downloaded: u64,
    pub(crate) left: u64,
    pub(crate) compact: bool
}

impl TrackerResponse {
    pub(crate) fn get_peer_addresses(&self) -> Result<Vec<peer::PeerAddress>, anyhow::Error> {
        if self.peers.len() % 6 == 0 {
            Ok(self.peers.chunks(6).into_iter().map(|peer| {
                let address = IpAddr::V4(Ipv4Addr::new(peer[0], peer[1], peer[2], peer[3]));
                let port = (peer[5] as u16) | (peer[4] as u16) << 8;
                peer::PeerAddress { address, port }
            }).collect())
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Peers field size is not a multiple of 6, {:?}", self.peers)).into())
        }
    }
}

impl Tracker {

    pub(crate) fn join_swarm(current_peer_id: &str, port: usize, torrent: &torrent::Torrent) -> Result<Vec<peer::PeerAddress>, anyhow::Error> {
        let torrent_hash = torrent.info.compute_hash();
        //TODO: Some trackers might refuse "Connect" requests or be offline, try several trackers associated with the torrent and find the one which works
        let tracker = tracker::Tracker { url: torrent.announce.clone() };
    
        let request = tracker::TrackerRequest {
            peer_id: current_peer_id.to_string(),
            info_hash: url_utils::url_encode_bytes(&torrent_hash),
            port,
            uploaded: 0,
            downloaded: 0,
            left: torrent.info.length.unwrap_or(0) as u64,
            compact: true
        };
        //TODO: Go over the list of announce-list and try getting peer addresses from every of the listed servers if announce-list is present
        let response = tracker.get(&tracker.url, &request)?;
        response.get_peer_addresses()
    }

    pub(crate) fn get(&self, url: &str, request: &TrackerRequest) -> Result<TrackerResponse, anyhow::Error> {
        if self.url.starts_with("http") {
            self.get_http(url, request)
        } else if self.url.starts_with("udp") {
            self.get_udp(url, request)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Unknown URL scheme, only HTTP and UDP are supported, {:?}", self.url)).into())
        }
    }

    fn get_udp(&self, url: &str, request: &TrackerRequest) -> Result<TrackerResponse, anyhow::Error> {
        //TODO: Some trackers just do not respond to the UDP "Connect" request, return an Error in this case
        let socket: UdpSocket = UdpSocket::bind("0.0.0.0:6969")?;
        socket.set_read_timeout(Some(Duration::new(2, 0)))?;

        let tracker_url = Url::parse(url)?;

        let tracker_host = tracker_url.host_str().ok_or(new_error(format!("Could not parse host from url {}", tracker_url)))?;
        let tracker_port: u16 = tracker_url.port().ok_or(new_error(format!("Could not parse port from url {}", tracker_url)))?;

        let tracker_address: SocketAddr = (tracker_host, tracker_port).to_socket_addrs()?.next()
            .ok_or(new_error(format!("Could not resolve as SocketAddr {}", tracker_url)))?;

        println!("tracker_address = {}", tracker_address);

        let connect_request = ConnectRequest::new(rand::random::<u32>());
        let bytes_sent = socket.send_to(&connect_request.get_bytes(), tracker_address)?;
        println!("Sent 'connect' request {:?}, bytes sent = {}", connect_request, bytes_sent);

        let mut response_buf: [u8; 16] = [0; 16];
        let max_connect_response_connect_attempts = 5;
        let mut current_connect_attempt = 1;
        let mut received_connect_response = false;
        while !received_connect_response && current_connect_attempt <= max_connect_response_connect_attempts {
            match socket.recv_from(&mut response_buf) {
                Ok((bytes_read, _)) => {
                    println!("Received {} bytes in Connect responst", bytes_read);
                    received_connect_response = true;
                }
                Err(_) => {
                    println!("Will try to connect again, failed so far... UDP tracker {}", &self.url);
                    socket.send_to(&connect_request.get_bytes(), tracker_address)?;
                    current_connect_attempt = current_connect_attempt + 1;
                }
            };
        }
        if received_connect_response {
            let connect_response = ConnectResponse::parse(&response_buf)?;
            println!("Received 'connect' response {:?}", connect_response);
            ensure!(connect_response.transaction_id == connect_request.transaction_id,
                "transaction_id in 'connect' request was {}, transaction_id in 'connect' response was {}, they should match but did not",
                connect_request.transaction_id,
                connect_response.transaction_id
            );
            //TODO: Check that the transaction id matches in the request and response
            let connection_id = connect_response.connection_id;
        } else {
            println!("Failed to connect to UDP tracker {}", &self.url);
        }

        //Following the spec https://www.bittorrent.org/beps/bep_0015.html
        //TODO: Send the "connect" request
        //TODO: Receive the response from the "connect" request, read and store the connection_id
        //TODO: Send the "announce" request
        //TODO: Receive the "announce" response and parse the list of peers from its bytes
        Err(std::io::Error::new(std::io::ErrorKind::Other, format!("UDP support is not yet implemented, url = {}", self.url)).into())
    }

    fn get_http(&self, url: &str, request: &TrackerRequest) -> Result<TrackerResponse, anyhow::Error> {
        let client = reqwest::blocking::Client::new();
        let rest_of_params = [
            ("peer_id", request.peer_id.to_string()),
            ("port", request.port.to_string()),
            ("uploaded", request.uploaded.to_string()),
            ("downloaded", request.downloaded.to_string()),
            ("left", request.left.to_string()),
            ("compact", (if request.compact { "1" } else { "0" }).to_string())
        ];
        let url_encoded_rest_of_params = serde_urlencoded::to_string(rest_of_params)?;
        let url_with_params = format!("{}?{}&info_hash={}", url, url_encoded_rest_of_params, request.info_hash);
        let response = client.get(url_with_params)
            .send()?;

        if response.status().is_success() {
            let response_chars = response.bytes()?.to_vec();
            let decoded = bencoded::decode_bencoded_from_bytes(&response_chars)?;
            let interval = decoded.get_by_key("interval")?.as_number()? as u32;
            let peers = decoded.get_by_key("peers")?.as_bytes()?;
            Ok(TrackerResponse {
                interval,
                peers
            })
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Got response {}", &response.status())).into())
        }
    }
}

//TODO: Add tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_send_connect_request_and_receive_connect_response() {
        //This tracker responds to the connection request as expected (as debugged with Wireshark and local Transmission)
        let tracker_url = "udp://93.158.213.92:1337";
        let tracker = Tracker {
            //url: "udp://tracker.openbittorrent.com:80/announce".to_string(),
            //url: "udp://tracker.coppersurfer.tk:6969".to_string(),
            url: tracker_url.to_string(),
            //url: "udp://107.150.14.110:6969".to_string()
        };
        let tracker_request = tracker::TrackerRequest {
            peer_id: "todo".to_string(),
            info_hash: "todo".to_string(),
            port: 6969,
            uploaded: 0,
            downloaded: 0,
            left: 9999,
            compact: true
        };
        let response = tracker.get_udp(&tracker.url,&tracker_request).unwrap();
    }
}