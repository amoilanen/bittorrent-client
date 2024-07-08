use std::io::Write;
use std::net::TcpStream;
use std::thread;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::cmp;
use anyhow::Result;
use peer::{ BlockState, Peer, PeerAddress, PeerConnectionState, PeerInterestedState, PeerChokedState, PeerMessage, PeerMessageId };
use sha1::digest::block_buffer::Block;
use std::collections::HashMap;

mod bencoded;
mod torrent;
mod format;
mod tracker;
mod url;
mod peer;

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
            println!("Piece length: {:?}", piece_length);
            println!("Total number of pieces to download: {:?}", number_of_pieces);

            let block_size_bytes = 16 * 1024; // 2^14
            let mut current_begin_in_piece = 0;
            let mut missing_piece_blocks = Vec::new();
            while current_begin_in_piece < piece_length {
                let current_block_length = cmp::min(block_size_bytes, piece_length - current_begin_in_piece);
                missing_piece_blocks.push([current_begin_in_piece, current_block_length]);
                current_begin_in_piece = current_begin_in_piece + current_block_length;
            }
            println!("Total number of blocks to download in the piece: {:?}", missing_piece_blocks.len());
            println!("{:?}", missing_piece_blocks);
            let missing_piece_blocks_length = missing_piece_blocks.len();
            let mut missing_piece_blocks_shared: Arc<Mutex<Vec<[usize; 2]>>> = Arc::new(Mutex::new(missing_piece_blocks));
            let mut piece_blocks: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
            let mut piece_blocks_downloaded: Arc<Mutex<Vec<peer::BlockState>>> = Arc::new(Mutex::new(vec![peer::BlockState::Absent; missing_piece_blocks_length]));

            for other_peer_address in response.get_peer_addresses()? {
                let (other_peer_handshake, other_peer_stream) = peer::Peer::handshake(&other_peer_address, &current_peer_handshake)?;
                println!("Established connection to peer {:?} peer address {:?}", other_peer_handshake.peer.clone(), &other_peer_address);
                let peer_thread = exchange_messages_with_peer(
                    &other_peer_handshake.peer,
                    other_peer_stream,
                    &missing_piece_blocks_shared,
                    &mut piece_blocks,
                    &mut piece_blocks_downloaded,
                    &mut peer_bitfields,
                    &mut peer_connection_states
                )?;
                peer_threads.insert(other_peer_handshake.peer.clone(), peer_thread);
                //Only connect to the first peer to ease the debugging
                //break;
            }
            //TODO: Add double-way communication handlers for every connected peer - OK
            //TODO: Handle the "bitfield" message from the peer - OK
            //TODO: Handle the "choke" message from the peer - OK
            //TODO: Handle the "unchoke" message from the peer - OK

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
        peer: &peer::Peer,
        mut other_peer_stream: TcpStream,
        missing_piece_blocks: &Arc<Mutex<Vec<[usize; 2]>>>,
        piece_blocks: &mut Arc<Mutex<Vec<Vec<u8>>>>,
        piece_blocks_downloaded: &mut Arc<Mutex<Vec<BlockState>>>,
        peer_bitfields: &mut Arc<Mutex<HashMap<Peer, Vec<u8>>>>,
        peer_connection_states: &mut Arc<Mutex<HashMap<Peer, PeerConnectionState>>>) -> Result<JoinHandle<i32>, anyhow::Error> {
    {
        peer_connection_states.lock().unwrap().insert(peer.clone(), PeerConnectionState::initial());
    }
    let missing_blocks = Arc::clone(&missing_piece_blocks);
    let bitfields = Arc::clone(&peer_bitfields);
    let blocks = Arc::clone(&piece_blocks);
    let blocks_downloaded = Arc::clone(&piece_blocks_downloaded);
    let connection_states = Arc::clone(&peer_connection_states);    let other_peer = peer.clone();
    let thread = thread::spawn(move || {
        loop {
            let current_state = {
                let states = connection_states.lock().unwrap();
                states.get(&other_peer).cloned()
            };
            if let Some(connection_state) = current_state {
                // Check the current connection state and send messages to the peer if the state of the connection requires it
                if connection_state.interested == PeerInterestedState::NotInterested {
                    println!("Sending: 'interested' to peer {:?}", other_peer.clone());
                    let interested_message = PeerMessage::with_id(PeerMessageId::Interested);
                    other_peer_stream.write_all(&interested_message.get_bytes()).unwrap();
                    connection_states.lock().unwrap().insert(other_peer.clone(), connection_state.update_interested(PeerInterestedState::Interested));
                    //println!("Sent: 'interested' to peer {:?}", other_peer.clone());
                // Receive messages from the peer
                } else {
                    if connection_state.choked == PeerChokedState::Choked {
                        let message = Peer::read_message(&mut other_peer_stream).unwrap();
                        if message.message_id == PeerMessageId::Bitfield {
                            println!("Received: 'bitfield' from peer {:?}", other_peer.clone());
                            bitfields.lock().unwrap().insert(other_peer.clone(), message.payload);
                            //println!("Inserted PeerConnectionState::ReceivedBitfield")
                        } else if message.message_id == PeerMessageId::Unchoke {
                            println!("Received: 'unchoke' from peer {:?}", other_peer.clone());
                            connection_states.lock().unwrap().insert(other_peer.clone(), connection_state.update_choked(PeerChokedState::Unchoked));
                        }
                        //TODO: Receive the "piece" message, it can also arrive in the "Choked" state
                    } else { // Interested and unchoked
                        let mut blocks_state = blocks_downloaded.lock().unwrap();
                        let missing = missing_blocks.lock().unwrap();
                        let next_blocks_to_ask: Vec<usize> = blocks_state
                            .iter().enumerate().filter_map(|(index, x)| if x == &BlockState::Absent {
                                Some(index)
                            } else {
                                None
                            }).take(5).collect();
                        next_blocks_to_ask.iter().for_each(|index|
                            blocks_state[*index] = BlockState::Requested
                        );
                        let next_block_requests: Vec<[usize; 2]> = next_blocks_to_ask.iter().map(|index| {
                            //TODO: Construct the body of the request from missing[*index] and bencode the parameters of the request
                            let request_body: Vec<u8> = Vec::new();
                            let block_request = PeerMessage::new(PeerMessageId::Request, request_body);
                            missing[*index]
                        }).collect();

                        //TODO: Periodically reset the BlockState::Requested state of the blocks to Absent to allow them to be requested from other peers: in case
                        //download is taking too much or the peer did not respond with a block

                        //TODO: Receive the "piece" message
                    }
                }
            }
        }
        0 // result
    });
    Ok(thread)
}