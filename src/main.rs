use serde_json;
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::fs::read;

mod bencoded;

#[derive(Debug)]
struct TorrentFileInfo {
    length: usize,
    path: Vec<String>
}

#[derive(Debug)]
struct TorrentInfo {
    name: String,
    pieces: Vec<u8>,
    piece_length: usize,
    length: Option<usize>,
    files: Option<Vec<TorrentFileInfo>>,
}

#[derive(Debug)]
struct Torrent {
    announce: String,
    info: TorrentInfo
}

impl Torrent {
    fn from_bytes(torrent_bytes: Vec<u8>) -> Result<Torrent, std::io::Error> {
        let chars: Vec<char> = torrent_bytes.iter().map(|b| *b as char).collect();
        let decoded = bencoded::decode_bencoded(&chars)?;
        let announce = decoded.get_by_key("announce").ok_or(
            std::io::Error::new(std::io::ErrorKind::Other, format!("Did not find 'announce' field in {:?}", decoded)))?.as_string()
            .ok_or(std::io::Error::new(std::io::ErrorKind::Other, format!("'announce' field is not a String in {:?}", decoded)))?;
        //TODO: Read the rest of the fields from decoded, pieces should be a Vec<u8>
        Ok(Torrent {
            announce: announce,
            info: TorrentInfo {
                name: "".to_string(),
                pieces: Vec::new(),
                piece_length: 256,
                length: Some(0),
                files: None
            }
        })
    }
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> Result<(), std::io::Error> {
    let args: Vec<String> = env::args().collect();
    //let command = &args[1];
    let command = "info";

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = bencoded::decode_bencoded_from_str(&encoded_value)?.as_json();
        println!("{}", decoded_value.to_string());
        Ok(())
    } else if command == "info" {
        //let torrent_file_path = &args[2];
        let torrent_file_path = "sample.torrent";
        let torrent_file_bytes = read(torrent_file_path)?;
        let torrent = Torrent::from_bytes(torrent_file_bytes);
        println!("Torrent file: {:?}", torrent);
        Ok(())
    } else {
        println!("unknown command: {}", args[1]);
        Ok(())
    }
}

//TODO: Implement tests for reading the torrent info