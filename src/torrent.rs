use anyhow::Result;
use crate::bencoded::BencodeEncoding;
use crate::peer;

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

    pub(crate) fn total_piece_number(&self) -> usize {
        let piece_length = self.piece_length;
        (self.length.unwrap_or(0) + piece_length - 1) / piece_length
    }

    pub(crate) fn get_all_pieces(&self) -> Vec<peer::Piece> {
        (0..self.total_piece_number()).map(|piece_index| {
            let piece_length_to_download = self.piece_length_at_index(piece_index as u32).unwrap();
            let piece = peer::Piece {
                index: piece_index as u32,
                piece_length: piece_length_to_download
            };
            piece
        }).collect()
    }

    pub(crate) fn piece_length_at_index(&self, piece_index: u32) -> Result<u32, anyhow::Error> {
        let total_piece_number = self.total_piece_number();
        if piece_index >= total_piece_number as u32 {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Invalid piece index {:?}, total number of pieces {:?}", piece_index, total_piece_number)).into())
        } else {
            if piece_index + 1 == total_piece_number as u32 {
                let last_piece_length = self.length.unwrap_or(0) - self.piece_length * (total_piece_number - 1);
                Ok(last_piece_length as u32)
            } else {
                Ok(self.piece_length as u32)
            }
        }
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

    pub fn parse_torrent(torrent_file_path: &str) -> Result<Torrent, anyhow::Error> {
        let torrent_file_bytes = std::fs::read(torrent_file_path)?;
        Torrent::from_bytes(&torrent_file_bytes)
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