use anyhow::Result;
use sha1::{Sha1, Digest};
use crate::bencoded::BencodeEncoding;

#[derive(Debug, PartialEq)]
pub struct TorrentFileInfo {
    pub length: usize,
    pub path: Vec<String>
}

#[derive(Debug, PartialEq)]
pub struct TorrentInfo {
    pub name: String,
    pub pieces: Vec<u8>,
    pub piece_length: usize,
    pub length: Option<usize>,
    pub files: Option<Vec<TorrentFileInfo>>,
}

impl TorrentInfo {
    const PIECE_HASH_SIZE: usize = 20;

    pub(crate) fn piece_hashes(&self) -> Vec<&[u8]> {
        self.pieces.chunks(TorrentInfo::PIECE_HASH_SIZE).collect()
    }

    pub(crate) fn bencode(&self) -> Vec<u8> {
        let mut bencoded: Vec<u8> = Vec::new();
        bencoded.push(b'd');
        if let Some(length) = self.length {
            bencoded.encode_str("length");
            bencoded.encode_usize(&length);
        }
        bencoded.encode_str("name");
        bencoded.encode_str(&self.name);
        bencoded.encode_str("piece length");
        bencoded.encode_usize(&self.piece_length);
        bencoded.encode_str("pieces");
        bencoded.encode_bytes(&self.pieces);
        bencoded.push(b'e');
        bencoded
    }

    pub(crate) fn compute_hash(&self) -> Vec<u8> {
        crate::hash::compute_hash(&self.bencode())
    }
}

#[derive(Debug, PartialEq)]
pub struct Torrent {
    pub announce: String,
    pub info: TorrentInfo
}

impl Torrent {
    pub fn from_bytes(torrent_bytes: &Vec<u8>) -> Result<Torrent, anyhow::Error> {
        let chars: Vec<char> = torrent_bytes.iter().map(|b| *b as char).collect();
        let decoded = crate::bencoded::decode_bencoded(&chars)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format;

    #[test]
    fn read_torrent_from_bytes() {
        let input = "d8:announce55:http://bittorrent-test-tracker.codecrafters.io/announce10:created by13:mktorrent 1.14:infod6:lengthi92063e4:name10:sample.txt12:piece lengthi32768e6:pieces20:00000000000000000000ee";
        let torrent = Torrent::from_bytes(&input.as_bytes().to_vec()).unwrap();
        assert_eq!(torrent.announce, "http://bittorrent-test-tracker.codecrafters.io/announce");
        assert_eq!(torrent.info.name, "sample.txt");
        assert_eq!(torrent.info.length, Some(92063));
        assert_eq!(torrent.info.piece_length, 32768);
        assert_eq!(torrent.info.files, None);
        assert_eq!(torrent.info.pieces, "00000000000000000000".as_bytes());
    }

    #[test]
    fn bencode_torrent_info() {
        let input = "d8:announce55:http://bittorrent-test-tracker.codecrafters.io/announce10:created by13:mktorrent 1.14:infod6:lengthi92063e4:name10:sample.txt12:piece lengthi32768e6:pieces20:00000000000000000000ee";
        let torrent = Torrent::from_bytes(&input.as_bytes().to_vec()).unwrap();
        let expected_torrent_info = "d6:lengthi92063e4:name10:sample.txt12:piece lengthi32768e6:pieces20:00000000000000000000e";
        assert_eq!(String::from_utf8(torrent.info.bencode()).unwrap(), expected_torrent_info)
    }

    #[test]
    fn compute_hash() {
        let input = "d8:announce55:http://bittorrent-test-tracker.codecrafters.io/announce10:created by13:mktorrent 1.14:infod6:lengthi92063e4:name10:sample.txt12:piece lengthi32768e6:pieces20:00000000000000000000ee";
        let torrent = Torrent::from_bytes(&input.as_bytes().to_vec()).unwrap();
        assert_eq!(format::format_as_hex_string(&torrent.info.compute_hash()), "e68d67c4b84274f741d7293fc0657102a36e7e3b");
    }
}