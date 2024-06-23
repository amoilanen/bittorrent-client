use std::env;
use anyhow::Result;

mod bencoded;
mod torrent;
mod format;
mod tracker;
mod url;
mod peer;

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
        let peer_id = peer::random_peer_id();
        let port = 6881;
        let tracker_response = tracker::get_info_from_tracker(&peer_id, &port, &torrent)?;
        for peer in tracker_response.get_peer_addresses()? {
            println!("{}:{}", peer.0, peer.1)
        }
        Ok(())
    } else {
        println!("unknown command: {}", args[1]);
        Ok(())
    }
}