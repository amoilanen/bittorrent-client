use std::net::{ Ipv4Addr, IpAddr };
use crate::bencoded;
use crate::torrent;
use crate::tracker;
use crate::peer;
use crate::url;

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
        let tracker = tracker::Tracker { url: torrent.announce.clone() };
    
        let request = tracker::TrackerRequest {
            peer_id: current_peer_id.to_string(),
            info_hash: url::url_encode_bytes(&torrent_hash),
            port,
            uploaded: 0,
            downloaded: 0,
            left: torrent.info.length.unwrap_or(0) as u64,
            compact: true
        };
        let response = tracker.get(&request)?;
        response.get_peer_addresses()
    }

    pub(crate) fn get(&self, request: &TrackerRequest) -> Result<TrackerResponse, anyhow::Error> {
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
        let url_with_params = format!("{}?{}&info_hash={}", self.url, url_encoded_rest_of_params, request.info_hash);
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