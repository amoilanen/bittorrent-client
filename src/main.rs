use std::env;
use anyhow::{ Result, Context, Error};

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
    fn from_bytes(torrent_bytes: Vec<u8>) -> Result<Torrent, anyhow::Error> {
        let chars: Vec<char> = torrent_bytes.iter().map(|b| *b as char).collect();
        let decoded = bencoded::decode_bencoded(&chars)?;
        let announce = decoded.get_by_key("announce")?.as_string()?;
        let info = decoded.get_by_key("info")?;
        let name = info.get_by_key("name")?.as_string()?;
        let pieces = info.get_by_key("pieces")?.as_bytes()?;
        let piece_length = info.get_by_key("piece length")?.as_number()? as usize;
        let length: Option<usize> = info.get_optional_by_key("length").and_then(|x| x.as_number().ok()).map(|x| x as usize);
        let mut torrent_file_infos: Vec<TorrentFileInfo> = Vec::new();
        if let Some(files) = info.get_optional_by_key("files").and_then(|x| x.as_values().ok()) { 
            for file in files {
                let file_length = file.get_by_key("length")?.as_number()? as usize;
                let mut path_parts: Vec<String> = Vec::new();
                let path_values = file.get_by_key("path")?.as_values()?;
                for path_value in path_values {
                    path_parts.push(path_value.as_string()?);
                }
                torrent_file_infos.push(TorrentFileInfo {
                    length: file_length,
                    path: path_parts
                })
            }
        }

        Ok(Torrent {
            announce: announce,
            info: TorrentInfo {
                name: name,
                pieces: pieces,
                piece_length: piece_length,
                length: length,
                files: if torrent_file_infos.is_empty() {
                    None
                } else {
                    Some(torrent_file_infos)
                }
            }
        })
    }
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> Result<(), anyhow::Error> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = bencoded::decode_bencoded_from_str(&encoded_value)?.as_json();
        println!("{}", decoded_value.to_string());
        Ok(())
    } else if command == "info" {
        let torrent_file_path = &args[2];
        //let torrent_file_path = "multifile.torrent";
        //println!("torrent_file_path: {}", torrent_file_path);
        let torrent_file_bytes = std::fs::read(torrent_file_path)?;
        let torrent = Torrent::from_bytes(torrent_file_bytes)?;
        println!("Torrent file: {:?}", torrent);
        println!("Tracker URL: {}", torrent.announce);
        println!("Length: {}", torrent.info.length.unwrap_or(0));
        Ok(())
    } else {
        println!("unknown command: {}", args[1]);
        Ok(())
    }
}

//TODO: Implement tests for reading the torrent info