use std::io::Write;
use std::net::TcpStream;
use std::thread;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use anyhow::Result;
use peer::{ Peer, PeerAddress, PeerChokedState, PeerConnectionState, PeerInterestedState, PeerMessage, PeerMessageId, Piece, PieceBlock };
use torrent::TorrentInfo;
use std::collections::HashMap;

mod bencoded;
mod torrent;
mod format;
mod tracker;
mod url;
mod peer;
mod hash;

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> Result<(), anyhow::Error> {
    //let args: Vec<String> = env::args().collect();
    //let command = &args[1];
    let command = "download_piece";
    let args: Vec<String> = vec!["", "", "-o", "/tmp/test-piece-0", "sample.torrent", "0"].iter().map(|x| x.to_string()).collect();

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

        let current_peer_id = peer::random_peer_id();
        let torrent_hash = torrent.info.compute_hash();
        let port = 6881;
        let tracker = tracker::Tracker { url: torrent.announce };

        let request = tracker::TrackerRequest {
            peer_id: current_peer_id,
            info_hash: url::url_encode_bytes(&torrent_hash),
            port,
            uploaded: 0,
            downloaded: 0,
            left: torrent.info.length.unwrap_or(0) as u64,
            compact: true
        };
        let response = tracker.get(&request)?;
        for peer in response.get_peer_addresses()? {
            println!("{}:{}", peer.address, peer.port)
        }
        Ok(())
    } else if command == "handshake" {
        let torrent_file_path = &args[2];
        let other_peer_address = PeerAddress::from_str(&args[3])?;
        let torrent_file_bytes = std::fs::read(torrent_file_path)?;
        let torrent = torrent::Torrent::from_bytes(&torrent_file_bytes)?;

        let current_peer_id = peer::random_peer_id();
        let torrent_hash = torrent.info.compute_hash();

        let current_peer_handshake = peer::PeerHandshake {
            info_hash: torrent_hash,
            peer: peer::Peer {
                id: current_peer_id.as_bytes().to_vec()
            }
        };
        let (other_peer_handshake, _) = peer::Peer::handshake(&other_peer_address, &current_peer_handshake)?;
        println!("Peer ID: {}", format::format_as_hex_string(&other_peer_handshake.peer.id));
        Ok(())
    } else if command == "download_piece" {
        let option = &args[2];
        if option != "-o" {
            Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Unexpected arguments, did not find -o, {:?}", &args)).into())
        } else {
            let output_file_path =&args[3];
            let torrent_file_path = &args[4];
            let piece_index = args[5].parse::<usize>()?;
            println!("Downloading piece {:?} from torrent {:?} to file {:?}", piece_index, torrent_file_path, output_file_path);

            let torrent_file_bytes = std::fs::read(torrent_file_path)?;
            let torrent = torrent::Torrent::from_bytes(&torrent_file_bytes)?;

            let current_peer_id = peer::random_peer_id();
            let torrent_hash = torrent.info.compute_hash();
            let port = 6881;
            let tracker = tracker::Tracker { url: torrent.announce };

            let request = tracker::TrackerRequest {
                peer_id: current_peer_id.clone(),
                info_hash: url::url_encode_bytes(&torrent_hash),
                port,
                uploaded: 0,
                downloaded: 0,
                left: torrent.info.length.unwrap_or(0) as u64,
                compact: true
            };
            let response = tracker.get(&request)?;

            let mut peer_bitfields: Arc<Mutex<HashMap<Peer, Vec<u8>>>>= Arc::new(Mutex::new(HashMap::new()));
            let mut peer_connection_states: Arc<Mutex<HashMap<Peer, PeerConnectionState>>> = Arc::new(Mutex::new(HashMap::new()));

            let mut peer_threads: HashMap<Peer, JoinHandle<i32>> = HashMap::new();

            let current_peer_handshake = peer::PeerHandshake {
                info_hash: torrent_hash,
                peer: peer::Peer {
                    id: current_peer_id.as_bytes().to_vec()
                }
            };

            let piece_length = torrent.info.piece_length;
            let number_of_pieces = (torrent.info.length.unwrap_or(0) + piece_length - 1) / piece_length;
            if piece_index >= number_of_pieces {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Invalid piece index {:?}, total number of pieces {:?}", piece_index, number_of_pieces)).into())
            }
            let piece = Piece {
                index: piece_index
            };
            println!("Piece length: {:?}", piece_length);
            println!("Total number of pieces to download: {:?}", number_of_pieces);
            let piece_blocks = piece.get_blocks(piece_length);
            println!("Piece blocks {:?}", piece_blocks);
            let piece_blocks_to_download: Arc<Mutex<Vec<PieceBlock>>> = Arc::new(Mutex::new(piece_blocks));
            let mut piece_blocks: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::with_capacity(piece_length)));
            let torrent_info: Arc<Mutex<TorrentInfo>>  = Arc::new(Mutex::new(torrent.info));

            for other_peer_address in response.get_peer_addresses()? {
                let (other_peer_handshake, other_peer_stream) = peer::Peer::handshake(&other_peer_address, &current_peer_handshake)?;
                println!("Established connection to peer {:?} peer address {:?}", other_peer_handshake.peer.clone(), &other_peer_address);
                let peer_thread = exchange_messages_with_peer(
                    &torrent_info,
                    piece.index,
                    &other_peer_handshake.peer,
                    other_peer_stream,
                    &piece_blocks_to_download,
                    &mut piece_blocks,
                    &mut peer_bitfields,
                    &mut peer_connection_states
                )?;
                peer_threads.insert(other_peer_handshake.peer.clone(), peer_thread);
                //TODO: comment out the following line. Only connect to the first peer to ease the debugging
                break;
            }
            //TODO: While the piece has not been downloaded fully send "request" messages for the blocks of this piece to the peers which have the piece
            //TODO: Handle the incoming "piece" message from the peer
               //--If the piece has not been downloaded correctly/hash mismatches, restart the download
               //--If the piece has been downloaded correctly, stop the download, output the info in the format "Piece 0 downloaded to /tmp/test-piece-0."

            //TODO: Once downloading a piece is completed send a "have" message to the peers
            //TODO: Re-factor and extract the function(s) for downloading the piece to the "peer" module

            //TODO: Send the "bitfield" message to the peers when connecting to them and when a download of each piece is finished
            //TODO: Download pieces for the whole file in the random order
            for (_, thread) in peer_threads {
                thread.join().unwrap();
            }
            Ok(())
        }
    } else {
        println!("unknown command: {}", args[1]);
        Ok(())
    }
}

fn exchange_messages_with_peer(
        torrent_info: &Arc<Mutex<TorrentInfo>>,
        piece_index: usize,
        peer: &peer::Peer,
        mut other_peer_stream: TcpStream,
        piece_blocks_to_download: &Arc<Mutex<Vec<PieceBlock>>>,
        piece_blocks: &mut Arc<Mutex<Vec<u8>>>,
        peer_bitfields: &mut Arc<Mutex<HashMap<Peer, Vec<u8>>>>,
        peer_connection_states: &mut Arc<Mutex<HashMap<Peer, PeerConnectionState>>>) -> Result<JoinHandle<i32>, anyhow::Error> {
    {
        peer_connection_states.lock().unwrap().insert(peer.clone(), PeerConnectionState::initial());
    }
    let torrent_info_per_thread = Arc::clone(torrent_info);
    let piece_blocks_to_download_per_thread = Arc::clone(&piece_blocks_to_download);
    let peer_bitfields_per_thread = Arc::clone(&peer_bitfields);
    let piece_blocks_per_thread = Arc::clone(&piece_blocks);
    let peer_connection_states_per_thread = Arc::clone(&peer_connection_states);    let other_peer = peer.clone();
    let thread = thread::spawn(move || {
        let mut sending = true;
        loop {
            let current_state = {
                let states = peer_connection_states_per_thread.lock().unwrap();
                states.get(&other_peer).cloned()
            };
            if sending {
                if let Some(connection_state) = current_state {
                    if connection_state.interested == PeerInterestedState::NotInterested {
                        let mut peer_connection_states = peer_connection_states_per_thread.lock().unwrap();
                        //println!("Sending: 'interested' to peer {:?}", other_peer.clone());
                        let interested_message = PeerMessage::with_id(PeerMessageId::Interested);
                        other_peer_stream.write_all(&interested_message.get_bytes()).unwrap();
                        peer_connection_states.insert(other_peer.clone(), connection_state.update_interested(PeerInterestedState::Interested));
                        println!("Sent: 'interested' to peer {:?}", other_peer.clone());
                    } else if connection_state.choked == PeerChokedState::Unchoked {
                        let mut piece_blocks_to_download = piece_blocks_to_download_per_thread.lock().unwrap();
                        let next_blocks_to_ask: Vec<PieceBlock> = piece_blocks_to_download.drain(0..1).collect(); //take 5 ? request 5 blocks from the same peer for more optimal download?
                        if next_blocks_to_ask.len() > 0 {
                            let next_block_requests: Vec<PeerMessage> = next_blocks_to_ask.iter().map(|block| {
                                PeerMessage::new_request(piece_index, block.begin, block.length)
                            }).collect();
                            for request in next_block_requests {
                                println!("Sent: 'request' {:?} to peer {:?}", &request.to_string(), other_peer.clone());
                                other_peer_stream.write_all(&request.get_bytes()).unwrap();
                            }
                        }
                    }
                }
            } else {
                if let Some(connection_state) = current_state {
                    let message = Peer::read_message(&mut other_peer_stream).unwrap();
                    if message.message_id == PeerMessageId::Bitfield {
                        println!("Received: 'bitfield' from peer {:?}", other_peer.clone());
                        peer_bitfields_per_thread.lock().unwrap().insert(other_peer.clone(), message.payload);
                        //println!("Inserted PeerConnectionState::ReceivedBitfield")
                    } else if message.message_id == PeerMessageId::Unchoke {
                        println!("Received: 'unchoke' from peer {:?}", other_peer.clone());
                        peer_connection_states_per_thread.lock().unwrap().insert(other_peer.clone(), connection_state.update_choked(PeerChokedState::Unchoked));
                    } else if message.message_id == PeerMessageId::Piece {
                        let piece_blocks_to_download = piece_blocks_to_download_per_thread.lock().unwrap();
                        let mut piece_blocks = piece_blocks_per_thread.lock().unwrap();
                        println!("Received: 'piece' from peer {:?}", other_peer.clone());
                        let piece_message = message.as_piece().unwrap();

                        //TODO: Debug the indexes for the following line
                        piece_blocks[piece_message.begin..(piece_message.begin + piece_message.block.len())].copy_from_slice(&piece_message.block);
                        if piece_blocks_to_download.is_empty() {
                            let torrent_info = torrent_info_per_thread.lock().unwrap();
                            let expected_piece_hash = torrent_info.pieces[piece_index * 20..piece_index * 21].to_vec();
                            let computed_piece_hash = hash::compute_hash(&piece_blocks);
                            println!("Finished download, expected and computed hashes:");
                            println!("Expected hash: {}", format::format_as_hex_string(&expected_piece_hash));
                            println!("Computed hash: {}", format::format_as_hex_string(&computed_piece_hash));
                        }
                    }
                }
            }
            //TODO: Periodically reset the BlockState::Requested state of the blocks to Absent to allow them to be requested from other peers: in case
            //download is taking too much or the peer did not respond with a block
            sending = !sending;
        }
        0 // result
    });
    Ok(thread)
}