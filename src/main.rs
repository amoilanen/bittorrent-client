use std::{env, io::Read, thread};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use anyhow::Result;
use peer::{ Peer, PeerAddress, PeerConnectionState, PeerMessageId };
use std::collections::HashMap;

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
            let piece_index = &args[5].parse::<usize>()?;
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
            let mut piece_blocks: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
            let mut peer_threads: HashMap<Peer, JoinHandle<i32>> = HashMap::new();

            let current_peer_handshake = peer::PeerHandshake {
                info_hash: torrent_hash,
                peer: peer::Peer {
                    id: current_peer_id.as_bytes().to_vec()
                }
            };
            for other_peer_address in response.get_peer_addresses()? {
                let (peer_thread, peer) = connect_to_peer(&current_peer_handshake, &other_peer_address, &mut peer_bitfields, &mut peer_connection_states)?;
                peer_threads.insert(peer, peer_thread);
            }
            //TODO: Add double-way communication handlers for every connected peer - OK
            //TODO: Handle the "bitfield" message from the peer - OK
            //TODO: Handle the "choke" message from the peer
            //TODO: Handle the "unchoke" message from the peer

            //TODO: While the piece has not been downloaded fully send "request" messages for the blocks of this piece to the peers which have the piece
            //TODO: Handle the incoming "piece" message from the peer
               //--If the piece has not been downloaded correctly/hash mismatches, restart the download
               //--If the piece has been downloaded correctly, stop the download, output the info in the format "Piece 0 downloaded to /tmp/test-piece-0."
            //TODO: Re-factor and extract the function(s) for downloading the piece to the "peer" module
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

fn connect_to_peer(
        current_peer_handshake: &peer::PeerHandshake,
        peer_address: &PeerAddress,
        peer_bitfields: &mut Arc<Mutex<HashMap<Peer, Vec<u8>>>>,
        peer_connection_states: &mut Arc<Mutex<HashMap<Peer, PeerConnectionState>>>) -> Result<(JoinHandle<i32>, Peer), anyhow::Error> {
    let (other_peer_handshake, mut other_peer_stream) = peer::Peer::handshake(&peer_address, &current_peer_handshake)?;

    let peer_bitfields_per_thread = Arc::clone(&peer_bitfields);
    let other_peer: Peer = (&other_peer_handshake).peer.clone();
    peer_connection_states.lock().unwrap().insert(other_peer.clone(), PeerConnectionState::Initial);
    let connection_states = Arc::clone(&peer_connection_states);
    println!("Established connection to the peer {:?} peer address {:?}", other_peer.clone(), peer_address);
    let thread = thread::spawn(move || {
        loop {
            let message = Peer::read_message(&mut other_peer_stream).unwrap();

            //"bitfield" message
            if message.message_id == PeerMessageId::Bitfield {
                println!("Received 'bitfield' message from the peer {:?}", other_peer.clone());
                peer_bitfields_per_thread.lock().unwrap().insert(other_peer.clone(), message.payload);
                connection_states.lock().unwrap().insert(other_peer.clone(), PeerConnectionState::ReceivedBitField);
            }
        }
        0 // result
    });
    Ok((thread, other_peer_handshake.peer.clone()))
}