use std::env;

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
        let info = decoded.get_by_key("info").ok_or(
            std::io::Error::new(std::io::ErrorKind::Other, format!("Did not find 'announce' field in {:?}", decoded)))?;
        let name = info.get_by_key("name").ok_or(
            std::io::Error::new(std::io::ErrorKind::Other, format!("Did not find 'info -> name' field in {:?}", decoded)))?.as_string()
            .ok_or(std::io::Error::new(std::io::ErrorKind::Other, format!("field is not a String in {:?}", decoded)))?;
        let pieces = info.get_by_key("pieces").ok_or(
            std::io::Error::new(std::io::ErrorKind::Other, format!("Did not find 'pieces' field in {:?}", decoded)))?.as_bytes().ok_or(
                std::io::Error::new(std::io::ErrorKind::Other, format!("'pieces' field in {:?} is not an array of bytes", decoded)))?;
        let piece_length = info.get_by_key("piece length").ok_or(
            std::io::Error::new(std::io::ErrorKind::Other, format!("Did not find 'piece length' field in {:?}", decoded)))?.as_number().map(|x| x as usize).ok_or(
                std::io::Error::new(std::io::ErrorKind::Other, format!("'piece length' field in {:?} is not a number", decoded)))?;
        let length: Option<usize> = info.get_by_key("length").and_then(|x| x.as_number()).map(|x| x as usize);
        let mut torrent_file_infos: Vec<TorrentFileInfo> = Vec::new();
        if let Some(files) = info.get_by_key("files").and_then(|x| x.as_values()) { 
            for file in files {
                let file_length = file.get_by_key("length").ok_or(
                    std::io::Error::new(std::io::ErrorKind::Other, format!("length not find 'announce' field in {:?}", file)))?.as_number().map(|x| x as usize).ok_or(
                        std::io::Error::new(std::io::ErrorKind::Other, format!("'length' field in {:?} is not a number", file)))?;
                let mut path_parts: Vec<String> = Vec::new();
                let path_values = file.get_by_key("path").and_then(|x| x.as_values()).ok_or(
                    std::io::Error::new(std::io::ErrorKind::Other, format!("Did not find 'path' field in {:?}", file)))?;
                for path_value in path_values {
                    if let Some(path_part) = path_value.as_string() {
                        path_parts.push(path_part);
                    }
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
fn main() -> Result<(), std::io::Error> {
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
        //println!("Torrent file: {:?}", torrent);
        println!("Tracker URL: {}", torrent.announce);
        println!("Length: {}", torrent.info.length.unwrap_or(0));
        Ok(())
    } else {
        println!("unknown command: {}", args[1]);
        Ok(())
    }
}

//TODO: Implement tests for reading the torrent info