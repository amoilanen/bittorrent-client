use std::env;
use std::fs::File;
use std::io::Write;
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::cmp::min;
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
    let args: Vec<String> = env::args().collect();
    let command = &args[1];
    //let command = "download_piece";
    //let args: Vec<String> = vec!["", "", "-o", "/tmp/test-piece-0", "sample.torrent", "0"].iter().map(|x| x.to_string()).collect();

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = bencoded::decode_bencoded_from_str(&encoded_value)?.as_json();
        println!("{}", decoded_value.to_string());
        Ok(())
    } else if command == "info" {
        let torrent_file_path = &args[2];
        //let torrent_file_path = "sample.torrent";
        println!("torrent_file_path: {}", torrent_file_path);
        let torrent_file_bytes = std::fs::read(torrent_file_path)?;
        let torrent = torrent::Torrent::from_bytes(&torrent_file_bytes)?;
        println!("Torrent file: {:?}", torrent);
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
        println!("torrent_file_path: {}", torrent_file_path);
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
            let output_file_path = &args[3];
            let torrent_file_path = &args[4];
            let piece_index = args[5].parse::<u32>()?;
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
            let mut peer_threads: HashMap<Peer, JoinHandle<i32>> = HashMap::new();

            let current_peer_handshake = peer::PeerHandshake {
                info_hash: torrent_hash,
                peer: peer::Peer {
                    id: current_peer_id.as_bytes().to_vec()
                }
            };

            let piece_length = torrent.info.piece_length;
            let number_of_pieces = (torrent.info.length.unwrap_or(0) + piece_length - 1) / piece_length;
            if piece_index >= number_of_pieces as u32 {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Invalid piece index {:?}, total number of pieces {:?}", piece_index, number_of_pieces)).into())
            }
            let last_piece_length = torrent.info.length.unwrap_or(0) - piece_length * (number_of_pieces - 1);
            println!("Piece length: {:?}", piece_length);
            println!("Last piece length: {:?}", last_piece_length);
            let piece_length_to_download = if piece_index + 1 == number_of_pieces as u32 {
                last_piece_length
            } else {
                piece_length
            };
            println!("Piece length to download: {:?}", piece_length_to_download);
            let piece = Piece {
                index: piece_index,
                piece_length: piece_length_to_download as u32
            };

            println!("Total number of pieces to download: {:?}", number_of_pieces);
            let piece_blocks = piece.get_blocks(piece.piece_length);
            println!("Piece blocks {:?}", piece_blocks);
            let piece_blocks_to_download: Arc<Mutex<Vec<PieceBlock>>> = Arc::new(Mutex::new(piece_blocks));
            let remaining_piece_bytes_to_download = Arc::new(Mutex::new(piece_length_to_download as u32));
            let mut piece_blocks: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(vec![0; piece_length_to_download]));
            let torrent_info: Arc<TorrentInfo>  = Arc::new(torrent.info);
            let shared_piece: Arc<Piece> = Arc::new(piece);
            let shared_output_file_path: Arc<String> = Arc::new(output_file_path.to_string());
            let mut peer_bitfields: Arc<Mutex<HashMap<Peer, Vec<u8>>>>= Arc::new(Mutex::new(HashMap::new()));
            let mut peer_connection_states: Arc<Mutex<HashMap<Peer, PeerConnectionState>>> = Arc::new(Mutex::new(HashMap::new()));
            let download_finished: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

            for other_peer_address in response.get_peer_addresses()? {
                let (other_peer_handshake, other_peer_stream) = peer::Peer::handshake(&other_peer_address, &current_peer_handshake)?;
                //other_peer_stream.set_read_timeout(Some(Duration::new(5, 0)))?;
                println!("Established connection to peer {:?} peer address {:?}", format::format_as_hex_string(&other_peer_handshake.peer.id), &other_peer_address);
                let peer_thread = exchange_messages_with_peer(
                    &download_finished,
                    &remaining_piece_bytes_to_download,
                    &shared_output_file_path,
                    &torrent_info,
                    &shared_piece,
                    piece_index,
                    &other_peer_handshake.peer,
                    other_peer_stream,
                    &piece_blocks_to_download,
                    &mut piece_blocks,
                    &mut peer_bitfields,
                    &mut peer_connection_states
                )?;
                peer_threads.insert(other_peer_handshake.peer.clone(), peer_thread);
                //TODO: comment out the following line. Only connect to the first peer to ease the debugging
                //Downloading from multiple peers should also work as expected, gracefully terminate the remaining threads
                break;
            }
            //TODO: Once downloading a piece is completed send a "have" message to the peers
            //TODO: Receive an interpret "have" messages from the peers
            //TODO: Re-factor and extract the function(s) for downloading the piece to the "peer" module
            //TODO: Send the "bitfield" message to the peers when connecting to them and when a download of each piece is finished
            //TODO: Download a piece from the peer only if the peer has it according to the bitfield message
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
        download_finished: &Arc<Mutex<bool>>,
        remaining_piece_bytes_to_download: &Arc<Mutex<u32>>,
        output_file_path: &Arc<String>,
        torrent_info: &Arc<TorrentInfo>,
        piece: &Arc<Piece>,
        piece_index: u32,
        peer: &peer::Peer,
        mut other_peer_stream: TcpStream,
        piece_blocks_to_download: &Arc<Mutex<Vec<PieceBlock>>>,
        piece_blocks: &mut Arc<Mutex<Vec<u8>>>,
        peer_bitfields: &mut Arc<Mutex<HashMap<Peer, Vec<u8>>>>,
        peer_connection_states: &mut Arc<Mutex<HashMap<Peer, PeerConnectionState>>>) -> Result<JoinHandle<i32>, anyhow::Error> {
    {
        peer_connection_states.lock().unwrap().insert(peer.clone(), PeerConnectionState::initial());
    }
    let download_finished_per_thread = Arc::clone(download_finished);
    let remaining_piece_bytes_to_download_per_thread = Arc::clone(remaining_piece_bytes_to_download);
    let torrent_info_per_thread = Arc::clone(torrent_info);
    let output_file_path_per_thread = Arc::clone(output_file_path);
    let piece_per_thread = Arc::clone(piece);
    let piece_blocks_to_download_per_thread = Arc::clone(&piece_blocks_to_download);
    let peer_bitfields_per_thread = Arc::clone(&peer_bitfields);
    let piece_blocks_per_thread = Arc::clone(&piece_blocks);
    let peer_connection_states_per_thread = Arc::clone(&peer_connection_states);
    let other_peer = peer.clone();
    let thread = thread::spawn(move || {
        let mut sending = true;
        const MAXIMUM_CONCURRENT_REQUEST_COUNT: usize = 5;
        let mut concurrent_request_count = 0;
        loop {
            {
                let download_finished = download_finished_per_thread.lock().unwrap();
                if *download_finished {
                    break;
                }
            }
            let current_state = {
                let states = peer_connection_states_per_thread.lock().unwrap();
                states.get(&other_peer).cloned()
            };
            if sending {
                if let Some(connection_state) = current_state {
                    if connection_state.interested == PeerInterestedState::NotInterested {
                        //println!("Sending: 'interested' to peer {:?}", format::format_as_hex_string(&other_peer.id));
                        let interested_message = PeerMessage::with_id(PeerMessageId::Interested);
                        other_peer_stream.write_all(&interested_message.get_bytes()).unwrap();
                        {
                            let mut peer_connection_states = peer_connection_states_per_thread.lock().unwrap();
                            peer_connection_states.insert(other_peer.clone(), connection_state.update_interested(PeerInterestedState::Interested));
                        }
                        println!("Sent: 'interested' to peer {:?}", format::format_as_hex_string(&other_peer.id));
                    } else if connection_state.choked == PeerChokedState::Unchoked {
                        let next_blocks_to_ask = {
                            let mut piece_blocks_to_download = piece_blocks_to_download_per_thread.lock().unwrap();
                            let remaining_piece_blocks_number = piece_blocks_to_download.len();
                            let remaining_concurrent_requests = if MAXIMUM_CONCURRENT_REQUEST_COUNT >= concurrent_request_count {
                                MAXIMUM_CONCURRENT_REQUEST_COUNT - concurrent_request_count
                            } else {
                                0
                            };
                            let next_piece_blocks_number = min(remaining_concurrent_requests, remaining_piece_blocks_number);
                            //let next_piece_blocks_number = min(1, remaining_piece_blocks_number); // Try not to issue more than one new request per one thread cycle
                            if next_piece_blocks_number == 0 {
                                Vec::new()
                            } else {
                                piece_blocks_to_download.drain(0..next_piece_blocks_number).collect()
                            }
                        };
                        concurrent_request_count = concurrent_request_count + next_blocks_to_ask.len();
                        if next_blocks_to_ask.len() > 0 {
                            let next_block_requests: Vec<PeerMessage> = next_blocks_to_ask.iter().map(|block| {
                                PeerMessage::new_request(piece_index, block.begin, block.length)
                            }).collect();
                            for request in next_block_requests {
                                //thread::sleep(std::time::Duration::from_millis(100));
                                println!("Sending: 'request' {:?} to peer {:?}", &request, format::format_as_hex_string(&other_peer.id));
                                let request_bytes = request.get_bytes();
                                println!("Request: {:?}", request_bytes);
                                other_peer_stream.write_all(&request_bytes).unwrap();
                            }
                        }
                    }
                }
            } else {
                if let Some(connection_state) = current_state {
                    if let Some(message) = Peer::read_message(&mut other_peer_stream).unwrap() {
                        if message.message_id == PeerMessageId::Bitfield {
                            println!("Received: 'bitfield' from peer {:?}", format::format_as_hex_string(&other_peer.id));
                            peer_bitfields_per_thread.lock().unwrap().insert(other_peer.clone(), message.payload);
                            //println!("Inserted PeerConnectionState::ReceivedBitfield")
                        } else if message.message_id == PeerMessageId::Unchoke {
                            println!("Received: 'unchoke' from peer {:?}", format::format_as_hex_string(&other_peer.id));
                            peer_connection_states_per_thread.lock().unwrap().insert(other_peer.clone(), connection_state.update_choked(PeerChokedState::Unchoked));
                        } else if message.message_id == PeerMessageId::Piece {
                            println!("Received: 'piece' from peer {:?}", format::format_as_hex_string(&other_peer.id));
                            concurrent_request_count = if concurrent_request_count > 1 {
                                concurrent_request_count - 1
                            } else {
                                0
                            };
                            //let piece_blocks_to_download = piece_blocks_to_download_per_thread.lock().unwrap();
                            let mut remaining_piece_bytes_to_download = remaining_piece_bytes_to_download_per_thread.lock().unwrap();
                            let mut piece_blocks = piece_blocks_per_thread.lock().unwrap();
                            let piece_message = message.as_piece().unwrap();
                            println!("Details about received 'piece': index={:?}  begin={:?} length={:?} from peer {:?}", piece_message.index, piece_message.begin, piece_message.block.len(), format::format_as_hex_string(&other_peer.id));
                            *remaining_piece_bytes_to_download = *remaining_piece_bytes_to_download - (piece_message.block.len() as u32);
                            //println!("Received: 'piece' from peer {:?}", format::format_as_hex_string(&other_peer.id));
                            //println!("piece_blocks.len() = {}", piece_blocks.len());
                            //println!("start_index = {}", piece_message.begin);
                            //println!("end_index = {}", piece_message.begin + piece_message.block.len());
                            //println!("block.len() = {}", piece_message.block.len());
                            piece_blocks[piece_message.begin..(piece_message.begin + piece_message.block.len())].copy_from_slice(&piece_message.block);
                            if *remaining_piece_bytes_to_download == 0 {
                                //println!("torrent_info_pieces.len() = {}", torrent_info_per_thread.pieces.len());
                                let expected_piece_hash = torrent_info_per_thread.pieces[(piece_index as usize) * 20..((piece_index as usize) + 1) * 20].to_vec();
                                let computed_piece_hash = hash::compute_hash(&piece_blocks);
                                println!("Finished download, expected and computed hashes:");
                                println!("Expected hash: {}", format::format_as_hex_string(&expected_piece_hash));
                                println!("Computed hash: {}", format::format_as_hex_string(&computed_piece_hash));
                                if expected_piece_hash == computed_piece_hash {
                                    let mut download_finished = download_finished_per_thread.lock().unwrap();
                                    //Finished downloading the piece successfully
                                    *download_finished = true;
                                    println!("Piece {} downloaded to {}.", piece_index, output_file_path_per_thread);
                                    let mut file = File::create(output_file_path_per_thread.as_str()).unwrap();
                                    file.write_all(piece_blocks.as_slice()).unwrap();
                                    break;
                                } else {
                                    //Restarting the download of the piece from scratch: something went wrong
                                    let mut piece_blocks_to_download = piece_blocks_to_download_per_thread.lock().unwrap();
                                    let mut all_piece_blocks = piece_per_thread.get_blocks(piece_per_thread.piece_length);
                                    piece_blocks_to_download.append(&mut all_piece_blocks);
                                    *remaining_piece_bytes_to_download = piece_per_thread.piece_length;
                                }
                            }
                        }
                    }
                }
            }
            //TODO: Periodically reset the BlockState::Requested state of the blocks to Absent to allow them to be requested from other peers: in case
            //download is taking too much or the peer did not respond with a block
            sending = !sending;
            let sleep_duration = Duration::from_millis(100);
            thread::sleep(sleep_duration);
        }
        0 // result
    });
    Ok(thread)
}