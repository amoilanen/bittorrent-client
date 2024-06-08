use serde_json;
use serde_json::json;
use std::collections::HashMap;
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
    let current_position: usize = position;
    let next_symbol = input[current_position];
    if next_symbol.is_digit(10) {
        decode_string(input, position)
    } else if next_symbol == 'i' {
        decode_number(input, position)
    } else if next_symbol == 'd' {
        decode_dictionary(input, position)
    } else if next_symbol == 'l' {
        decode_list(input, position)
    } else {
        let unparsed_input: String = input[position..].into_iter().collect();
        panic!("Unhandled encoded value: {}", unparsed_input)
    }
}

fn decode_list(input: &Vec<char>, position: usize) -> (Option<serde_json::Value>, usize) {
    let mut current_position: usize = position + 1; // skip 'l'
    let mut array_values: Vec<serde_json::Value> = Vec::new();
    while input[current_position] != 'e' && current_position < input.len() {
        let (current_decoded_value, updated_position) = decode_bencoded_at_position(input, current_position);
        current_position = updated_position;
        if current_decoded_value.is_some() {
            array_values.push(current_decoded_value.unwrap());
        }
    }
    if input[current_position] != 'e' {
        panic!("Malformed input '{:?}' around position {:?}", input, current_position);
    }
    (Some(serde_json::Value::Array(array_values)), current_position + 1) // skip 'e'
}

fn decode_dictionary(input: &Vec<char>, position: usize) -> (Option<serde_json::Value>, usize) {
    let mut current_position: usize = position + 1; // skip 'd'
    let mut object: HashMap<String, serde_json::Value> = HashMap::new();

    while input[current_position] != 'e' && current_position < input.len() {
        let (maybe_key, new_position) = decode_bencoded_at_position(input, current_position);
        current_position = new_position;
        if input[current_position] != 'e' {
            if let Some(key) = maybe_key {
                let (maybe_value, new_position) = decode_bencoded_at_position(input, current_position);
                current_position = new_position;
                if let Some(value) = maybe_value {
                    match key {
                        serde_json::Value::Number(key_number) => object.insert(key_number.to_string(), value),
                        serde_json::Value::String(key_string) => object.insert(key_string, value),
                        _ => None,
                    };
                }
            }
        }
    }
    if input[current_position] != 'e' {
        panic!("Malformed input '{:?}' around position {:?}", input, current_position);
    }
    (Some(json!(object)), current_position + 1) // skip 'e'
}

fn decode_string(input: &Vec<char>, position: usize) -> (Option<serde_json::Value>, usize) {
    let mut current_position: usize = position;
    let mut next_symbol = input[current_position];

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
}

fn decode_number(input: &Vec<char>, position: usize) -> (Option<serde_json::Value>, usize) {
    let mut current_position: usize = position + 1;
    let mut next_symbol = input[current_position];

    let mut number_chars: Vec<char> = Vec::new();
    while next_symbol != 'e' {
        number_chars.push(next_symbol);
        current_position += 1;
        next_symbol = input[current_position];
    }
    let number_input: String = number_chars.into_iter().collect();
    let number: i64 = number_input.parse::<i64>().unwrap();
    (Some(json!(number)), current_position + 1) // skip the 'e' symbol
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

    #[test]
    fn decode_bencoded_dictionary() {
        assert_eq!(decode_bencoded_value("d3:foo3:bar5:helloi52ee"), json!({
            "foo": "bar",
            "hello": 52
        }));
    }

    #[test]
    fn decode_empty_bencoded_dictionary() {
        assert_eq!(decode_bencoded_value("de"), json!({}));
    }

    #[test]
    fn decode_bencoded_dictionary_which_misses_value() {
        assert_eq!(decode_bencoded_value("d3:foo3:bar5:helloe"), json!({
            "foo": "bar"
        }));
    }

    #[test]
    fn decode_bencoded_nested_object() {
        assert_eq!(decode_bencoded_value("d10:inner_dictd4:key16:value14:key2i42eee"), json!({
            "inner_dict": {
                "key1":"value1",
                "key2":42
            }
        }));
    }

    #[test]
    fn decode_bencoded_objects_in_an_array() {
        assert_eq!(decode_bencoded_value("ld4:key16:value1ed4:key26:value2ed4:key36:value3ee"), json!([
            {
                "key1": "value1"
            },
            {
                "key2": "value2"
            },
            {
                "key3": "value3"
            }
        ]));
    }
}