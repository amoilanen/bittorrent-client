use serde_json;
use serde_json::json;
use std::collections::HashMap;
use std::env;

mod bencoded;

struct Torrent {

}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = bencoded::decode_bencoded_from_str(encoded_value).as_json();
        println!("{}", decoded_value.to_string());
    } else if command == "info" {
        let torrent_file = &args[2];
    } else {
        println!("unknown command: {}", args[1])
    }
}