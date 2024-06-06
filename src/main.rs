use serde_json;
use serde_json::json;
use std::env;

// Available if you need it!
// use serde_bencode

#[allow(dead_code)]
fn decode_bencoded_value(encoded_value: &str) -> serde_json::Value {
    decode_from_position(encoded_value, 0)
}

fn decode_from_position(encoded_value: &str, position: usize) -> serde_json::Value {
    // If encoded_value starts with a digit, it's a number
    let encoded_value_part = &encoded_value[position..];
    let next_symbol = encoded_value_part[position..].chars().next().unwrap();
    if next_symbol.is_digit(10) {
        // Example: "5:hello" -> "hello"
        let colon_index = encoded_value_part.find(':').unwrap();
        let number_string = &encoded_value_part[..colon_index];
        let number = number_string.parse::<i64>().unwrap();
        let string = &encoded_value_part[colon_index + 1..colon_index + 1 + number as usize];
        json!(string.to_string())
    } else if next_symbol == 'i' {
        let end_index = encoded_value_part.find('e').unwrap();
        let input_string = &encoded_value_part[position + 1..end_index];
        let number: i64 = input_string.parse().unwrap();
        json!(number)
    } else {
        panic!("Unhandled encoded value: {}", encoded_value_part)
    }
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decode_bencoded_string() {
        assert_eq!(decode_bencoded_value("4:spam"), json!("spam".to_string()));
    }

    #[test]
    fn decode_bencoded_integers() {
        assert_eq!(decode_bencoded_value("i52e"), json!(52));
    }
}