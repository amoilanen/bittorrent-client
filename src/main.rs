use std::env;
use std::io::Write;
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::cmp::min;
use std::path::Path;
use anyhow::Result;
use peer::{ Peer, PeerChokedState, PeerConnectionState, PeerInterestedState, PeerMessage, PeerMessageId, Piece, PieceBlock };
use torrent::TorrentInfo;
use std::collections::HashMap;

mod bencoded;
mod torrent;
mod format;
mod tracker;
mod url;
mod peer;
mod hash;
mod file;

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> Result<(), anyhow::Error> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];
    //let command = "download_piece";
    //let args: Vec<String> = vec!["", "", "-o", "/tmp/test-piece-0", "sample.torrent", "0"].iter().map(|x| x.to_string()).collect();
    //let command = "download";
    //let args: Vec<String> = vec!["", "", "-o", "/tmp/test.txt", "sample.torrent"].iter().map(|x| x.to_string()).collect();

    if command == "download" {
        let option = &args[2];
        if option != "-o" {
            Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Unexpected arguments, did not find -o, {:?}", &args)).into())
        } else {
            let output_file_path = &args[3];
            let torrent_file_path = &args[4];
            println!("Downloading from torrent {:?} to file {:?}", torrent_file_path, output_file_path);

            let torrent = torrent::Torrent::parse_torrent(torrent_file_path)?;
            let current_peer_id = peer::random_peer_id();
            let port = 6881;
            let peer_addresses = tracker::Tracker::join_swarm(&current_peer_id, port,&torrent)?;
            let mut peer_threads: HashMap<Peer, JoinHandle<i32>> = HashMap::new();

            let all_pieces: Vec<Piece> = torrent.info.get_all_pieces();
            if !Path::new(output_file_path).exists() {
                file::touch_and_fill_with_zeros(&output_file_path, torrent.info.length.unwrap_or(0))?;
            }
            //TODO: Decide which pieces are missing and still need to be downloaded by checking the hashes of the pieces of the file which has been downloaded so far

            let pieces_to_download: Arc<Mutex<Vec<Piece>>> = Arc::new(Mutex::new(all_pieces));
            let shared_output_file_path: Arc<String> = Arc::new(output_file_path.to_string());
            for other_peer_address in peer_addresses {
                let (other_peer_handshake, other_peer_stream) = peer::Peer::handshake_for_peer(&other_peer_address, &torrent.info, &current_peer_id)?;

                let shared_torrent_info: Arc<TorrentInfo>  = Arc::new(torrent.info);
                //other_peer_stream.set_read_timeout(Some(Duration::new(5, 0)))?;
                println!("Established connection to peer {:?} peer address {:?}", format::format_as_hex_string(&other_peer_handshake.peer.id), &other_peer_address);
                let peer_thread = exchange_messages_with_peer(
                    &pieces_to_download,
                    &shared_output_file_path,
                    &shared_torrent_info,
                    &other_peer_handshake.peer,
                    other_peer_stream
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
        pieces_to_download: &Arc<Mutex<Vec<Piece>>>,
        output_file_path: &Arc<String>,
        torrent_info: &Arc<TorrentInfo>,
        peer: &peer::Peer,
        mut peer_stream: TcpStream) -> Result<JoinHandle<i32>, anyhow::Error> {
    const MAXIMUM_CONCURRENT_REQUEST_COUNT: usize = 5;

    let pieces_to_download_per_thread = Arc::clone(pieces_to_download);
    let torrent_info_per_thread = Arc::clone(torrent_info);
    let output_file_path_per_thread = Arc::clone(output_file_path);
    let peer_in_this_thread = peer.clone();
    let thread = thread::spawn(move || {
        let mut sending = true;
        let mut concurrent_request_count = 0;
        let mut downloading_piece: bool = false;
        let mut current_piece: Option<Piece> = None;
        let mut piece_blocks_to_download: Vec<PieceBlock> = Vec::new();
        let mut remaining_piece_bytes_to_download = 0;
        let mut ready_piece_blocks = Vec::new();
        let mut peer_bitfield: Option<Vec<u8>> = None;
        let mut connection_state = PeerConnectionState::initial();

        loop {
            if !downloading_piece {
                let piece_indexes_present_on_peer = {
                    if let Some(bitfield) = &peer_bitfield {
                        Peer::get_piece_indices(bitfield)
                    } else {
                        Vec::new()
                    }
                };
                {
                    let mut pieces = pieces_to_download_per_thread.lock().unwrap();
                    let index_of_next_piece_to_download = pieces.iter().position(|piece| {
                        let piece_index: usize = piece.index as usize;
                        piece_indexes_present_on_peer.contains(&piece_index)
                    });
                    if let Some(piece_index) = index_of_next_piece_to_download {
                        let piece = pieces[piece_index].clone();
                        pieces.remove(piece_index);
                        piece_blocks_to_download = piece.get_blocks(piece.piece_length);
                        remaining_piece_bytes_to_download = piece.piece_length;
                        ready_piece_blocks = vec![0; piece.piece_length as usize];
                        downloading_piece = true;
                        current_piece = Some(piece.clone());
                    }
                }
            }

            {
                let pieces = pieces_to_download_per_thread.lock().unwrap();
                if !downloading_piece && pieces.is_empty() {
                    println!("Finished downloading the file");
                    break;
                }
            }
            if sending {
                /* 
                 * Current assumption: it makes sense to send messages to a peer only if we are downloading a piece from it
                 * This assumption might change when we send the "bitfield" and "have" messages to the peer in the future
                 */
                if downloading_piece {
                    let piece = &current_piece.clone().unwrap();
                    if connection_state.interested == PeerInterestedState::NotInterested {
                        let interested_message = PeerMessage::with_id(PeerMessageId::Interested);
                        peer_stream.write_all(&interested_message.get_bytes()).unwrap();
                        connection_state = connection_state.update_interested(PeerInterestedState::Interested);
                        println!("Sent: 'interested' to peer {:?}", format::format_as_hex_string(&peer_in_this_thread.id));
                    } else if connection_state.choked == PeerChokedState::Unchoked {
                        let next_blocks_to_ask = {
                            let remaining_piece_blocks_number = piece_blocks_to_download.len();
                            let remaining_concurrent_requests = if MAXIMUM_CONCURRENT_REQUEST_COUNT >= concurrent_request_count {
                                MAXIMUM_CONCURRENT_REQUEST_COUNT - concurrent_request_count
                            } else {
                                0
                            };
                            let next_piece_blocks_number = min(remaining_concurrent_requests, remaining_piece_blocks_number);
                            if next_piece_blocks_number == 0 {
                                Vec::new()
                            } else {
                                piece_blocks_to_download.drain(0..next_piece_blocks_number).collect()
                            }
                        };
                        concurrent_request_count = concurrent_request_count + next_blocks_to_ask.len();
                        if next_blocks_to_ask.len() > 0 {
                            let next_block_requests: Vec<PeerMessage> = next_blocks_to_ask.iter().map(|block| {
                                PeerMessage::new_request(piece.index, block.begin, block.length)
                            }).collect();
                            for request in next_block_requests {
                                //thread::sleep(std::time::Duration::from_millis(100));
                                println!("Sent: 'request' {:?} to peer {:?}", &request, format::format_as_hex_string(&peer_in_this_thread.id));
                                let request_bytes = request.get_bytes();
                                peer_stream.write_all(&request_bytes).unwrap();
                                println!("Request: {:?}", request_bytes);
                            }
                        }
                    }
                }
            } else {
                if let Some(message) = Peer::read_message(&mut peer_stream).unwrap() {
                    if message.message_id == PeerMessageId::Bitfield {
                        println!("Received: 'bitfield' from peer {:?}", format::format_as_hex_string(&peer_in_this_thread.id));
                        peer_bitfield = Some(message.payload);
                    } else if message.message_id == PeerMessageId::Unchoke {
                        println!("Received: 'unchoke' from peer {:?}", format::format_as_hex_string(&peer_in_this_thread.id));
                        connection_state = connection_state.update_choked(PeerChokedState::Unchoked);
                    } else if message.message_id == PeerMessageId::Piece {
                        println!("Received: 'piece' from peer {:?}", format::format_as_hex_string(&peer_in_this_thread.id));
                        if downloading_piece {
                            let piece = &current_piece.clone().unwrap();
                            concurrent_request_count = if concurrent_request_count > 1 {
                                concurrent_request_count - 1
                            } else {
                                0
                            };
                            let piece_message = message.parse_as_piece().unwrap();
                            println!("Details about received 'piece': index={:?}  begin={:?} length={:?} from peer {:?}", piece_message.index, piece_message.begin, piece_message.block.len(), format::format_as_hex_string(&peer_in_this_thread.id));
                            remaining_piece_bytes_to_download = remaining_piece_bytes_to_download - (piece_message.block.len() as u32);
                            ready_piece_blocks[piece_message.begin..(piece_message.begin + piece_message.block.len())].copy_from_slice(&piece_message.block);
                            if remaining_piece_bytes_to_download == 0 {
                                //println!("torrent_info_pieces.len() = {}", torrent_info_per_thread.pieces.len());
                                let expected_piece_hash = torrent_info_per_thread.pieces[(piece.index as usize) * 20..((piece.index as usize) + 1) * 20].to_vec();
                                let computed_piece_hash = hash::compute_hash(&ready_piece_blocks);
                                println!("Finished download, expected and computed hashes:");
                                println!("Expected hash: {}", format::format_as_hex_string(&expected_piece_hash));
                                println!("Computed hash: {}", format::format_as_hex_string(&computed_piece_hash));
                                if expected_piece_hash == computed_piece_hash {
                                    println!("Piece {} downloaded to {}.", piece.index, output_file_path_per_thread);
                                    let begin_in_file = torrent_info_per_thread.piece_length * (piece.index as usize);
                                    file::write_piece_to(&output_file_path_per_thread.as_str(), begin_in_file, ready_piece_blocks.as_slice());
                                    //Finished downloading the piece and is ready to pick up the next piece
                                    downloading_piece = false;
                                } else {
                                    //Restarting the download of the piece from scratch: something went wrong
                                    piece_blocks_to_download = piece.get_blocks(piece.piece_length);
                                    remaining_piece_bytes_to_download = piece.piece_length;
                                    ready_piece_blocks = vec![0; piece.piece_length as usize];
                                }
                            }
                        } else {
                            println!("Ignoring 'piece' since the piece is no longer being downloaded...")
                        }
                    }
                }
            }
            //TODO: In case download is taking too much or the peer did not respond with the blocks of the piece, return the piece to the queue
            sending = !sending;
            let sleep_duration = Duration::from_millis(100);
            thread::sleep(sleep_duration);
        }
        0 // result
    });
    Ok(thread)
}