use std::net::{ Ipv4Addr, IpAddr };
use crate::{ url, torrent, bencoded };

pub(crate) struct TrackerResponse {
    interval: u32,
    peers: Vec<u8>
}

impl TrackerResponse {
    pub(crate) fn get_peer_addresses(&self) -> Result<Vec<(IpAddr, u16)>, anyhow::Error> {
        if self.peers.len() % 6 == 0 {
            Ok(self.peers.chunks(6).into_iter().map(|peer| {
                let address = IpAddr::V4(Ipv4Addr::new(peer[0], peer[1], peer[2], peer[3]));
                let port = (peer[5] as u16) | (peer[4] as u16) << 8;
                (address, port)
            }).collect())
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Peers field size is not a multiple of 6, {:?}", self.peers)).into())
        }
    }
}

pub(crate) fn get_info_from_tracker(peer_id: &str, port: &u16, torrent: &torrent::Torrent) -> Result<TrackerResponse, anyhow::Error> {
    let client = reqwest::blocking::Client::new();

    let url = &torrent.announce;
    let torrent_hash = torrent.info.compute_hash();
    let info_hash: String = url::url_encode_bytes(&torrent_hash);
    //let info_hash: String = format("{}", urlencoding::encode_binary(&torrent_hash));
    println!("URL encoded info_hash {}", info_hash);
    let rest_of_params = [
        ("peer_id", peer_id.to_string()),
        ("port", port.to_string()),
        ("uploaded", "0".to_string()),
        ("downloaded", "0".to_string()),
        ("left", torrent.info.length.unwrap_or(0).to_string()),
        ("compact", "1".to_string())
    ];
    let url_encoded_rest_of_params = serde_urlencoded::to_string(rest_of_params)?;
    let url_with_params = format!("{}?{}&info_hash={}", url, url_encoded_rest_of_params, info_hash);
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