use std::{env, io::Read};
use anyhow::Result;
use std::net::{ IpAddr, Ipv4Addr };
use rand::Rng;
use std::collections::HashMap;
use serde_urlencoded;

mod bencoded;
mod torrent;
mod format;

struct TrackerResponse {
    interval: u32,
    peers: Vec<u8>
}

impl TrackerResponse {
    fn get_peer_addresses(&self) -> Vec<(IpAddr, u16)>  {
        self.peers.chunks(6).into_iter().map(|peer| {
            let address = IpAddr::V4(Ipv4Addr::new(peer[0], peer[1], peer[2], peer[3]));
            let port = (peer[5] as u16) | (peer[4] as u16) << 8;
            (address, port)
        }).collect()
    }
}

fn generate_random_number_string(length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| rng.gen_range(0..10))
        .map(|num| std::char::from_digit(num, 10).unwrap())
        .collect()
}

fn url_encode_bytes(bytes: &[u8]) -> String {
    let encoded_bytes: Vec<String> = bytes.iter()
        .map(|b: &u8| format!("%{:02X}", b)) // Convert each byte to hex string
        .collect();
    encoded_bytes.join("")
}

fn generate_peer_id() -> String {
    generate_random_number_string(20)
}

fn get_info_from_tracker(peer_id: &str, port: &u16, torrent: &torrent::Torrent) -> Result<TrackerResponse, anyhow::Error> {
    let client = reqwest::blocking::Client::new();

    let url = &torrent.announce;
    let torrent_hash = torrent.info.compute_hash();
    let info_hash: String = url_encode_bytes(&torrent_hash);
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
    //println!("url_encoded_rest_of_params = {}", url_encoded_rest_of_params);
    //println!("url_with_params = {}", url_with_params);
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

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> Result<(), anyhow::Error> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];
    //let command = "peers";

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = bencoded::decode_bencoded_from_str(&encoded_value)?.as_json();
        println!("{}", decoded_value.to_string());
        Ok(())
    } else if command == "info" {
        let torrent_file_path = &args[2];
        //let torrent_file_path = "sample.torrent";
        //println!("torrent_file_path: {}", torrent_file_path);
        let torrent_file_bytes = std::fs::read(torrent_file_path)?;
        let torrent = torrent::Torrent::from_bytes(&torrent_file_bytes)?;
        //println!("Torrent file: {:?}", torrent);
        println!("Tracker URL: {}", torrent.announce);
        println!("Length: {}", torrent.info.length.unwrap_or(0));
        println!("Info Hash: {}", format::format_as_hex_string(&torrent.info.compute_hash()));
        println!("Piece Length: {}", torrent.info.piece_length);
        println!("Piece Hashes:");
        for piece_hash in torrent.info.piece_hashes() {
            println!("{}", format::format_as_hex_string(piece_hash))
        }
        Ok(())
    } else if command == "peers" {
        let torrent_file_path = &args[2];
        //let torrent_file_path = "sample.torrent";
        //println!("torrent_file_path: {}", torrent_file_path);
        let torrent_file_bytes = std::fs::read(torrent_file_path)?;
        let torrent = torrent::Torrent::from_bytes(&torrent_file_bytes)?;
        let peer_id = generate_peer_id();
        let port = 6881;
        let tracker_response = get_info_from_tracker(&peer_id, &port, &torrent)?;
        for peer in tracker_response.get_peer_addresses() {
            println!("{}:{}", peer.0, peer.1)
        }
        Ok(())
    } else {
        println!("unknown command: {}", args[1]);
        Ok(())
    }
}