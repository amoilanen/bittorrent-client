use serde_json;
use serde_json::json;
use std::env;

// Available if you need it!
// use serde_bencode

#[allow(dead_code)]
fn decode_bencoded_value(encoded_value: &str) -> serde_json::Value {
    //TODO: Return a Result, rather than use unwrap
    //TODO: Handle the case when not all of the encoded_value input has been read
    decode_bencoded_at_position(&encoded_value.chars().collect(), 0).0.unwrap()
}

fn decode_bencoded_at_position(input: &Vec<char>, position: usize) -> (Option<serde_json::Value>, usize) {
    let mut current_position: usize = position;
    let mut next_symbol = input[current_position];
    if next_symbol.is_digit(10) {
        let mut number_chars: Vec<char> = Vec::new();
        while next_symbol != ':' {
            number_chars.push(next_symbol);
            current_position += 1;
            next_symbol = input[current_position];
        }
        current_position += 1;
        let string_value_length_input: String = number_chars.into_iter().collect();
        let string_value_length = string_value_length_input.parse::<usize>().unwrap();
        let string_value: String = input[current_position..current_position + string_value_length].into_iter().collect();

        (Some(json!(string_value)), current_position + string_value_length)
    } else if next_symbol == 'i' {
        current_position += 1;
        next_symbol = input[current_position];
        let mut number_chars: Vec<char> = Vec::new();
        while next_symbol != 'e' {
            number_chars.push(next_symbol);
            current_position += 1;
            next_symbol = input[current_position];
        }
        let number_input: String = number_chars.into_iter().collect();
        let number: i64 = number_input.parse::<i64>().unwrap();
        (Some(json!(number)), current_position + 1)
    } else if next_symbol == 'e' {
        (None, position + 1)
    } else if next_symbol == 'l' {
        let mut current_position: usize = position + 1;

        let mut array_values: Vec<serde_json::Value> = Vec::new();
        let mut decoded = decode_bencoded_at_position(input, current_position);
        let mut current_decoded_value: Option<serde_json::Value> = decoded.0;
        current_position = decoded.1;

        while current_decoded_value.is_some() {
            array_values.push(current_decoded_value.unwrap());
            decoded = decode_bencoded_at_position(input, current_position);
            current_decoded_value = decoded.0;
            current_position = decoded.1;
        }
        (Some(serde_json::Value::Array(array_values)), current_position)
    } else {
        let unparsed_input: String = input[position..].into_iter().collect();
        panic!("Unhandled encoded value: {}", unparsed_input)
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

    #[test]
    fn decode_bencoded_lists() {
        assert_eq!(decode_bencoded_value("l5:helloi52ee"), json!(["hello", 52]));
    }
}