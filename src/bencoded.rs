use serde_json;
use serde_json::json;
use std::collections::HashMap;
use std::hash::Hash;
use core::fmt::Debug;

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Value {
    List(Vec<Value>),
    Number(i64),
    String(Vec<u8>),
    Object(Vec<(Value, Value)>)
}

impl Value {
    pub fn get_by_key(&self, key: &str) -> Option<&Value> {
        let wrapped_key = Value::String(key.chars().into_iter().map(|x| x as u8).collect());
        match self {
            Value::Object(values) => {
                values.iter().find(|(stored_key, _)| {
                    wrapped_key == *stored_key
                }).map(|(_, value)| value)
            },
            _ => None
        }
    }

    pub fn as_string(&self) -> Option<String> {
        match self {
            Value::String(bytes) => {
                String::from_utf8(bytes.clone()).ok()
            },
            _ => None
        }
    }

    pub fn as_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Value::String(bytes) => {
                Some(bytes.clone())
            },
            _ => None
        }
    }

    pub fn as_number(&self) -> Option<i64> {
        match self {
            Value::Number(value) => {
                Some(*value)
            },
            _ => None
        }
    }

    pub fn as_values(&self) -> Option<&Vec<Value>> {
        match self {
            Value::List(values) =>
                Some(values),
            _ => None
        }
    }
}

impl Value {
    pub(crate) fn as_json(&self) -> serde_json::Value {
        match self {
            Value::Number(number) => json!(number),
            Value::String(string_bytes) => {
                let string: String = string_bytes.into_iter().map(|x| *x as char).collect();
                json!(string)
            },
            Value::List(values) => {
                let encoded_values: Vec<serde_json::Value> = values.into_iter().map(|value| value.as_json()).collect();
                json!(serde_json::Value::Array(encoded_values))
            },
            Value::Object(pairs) => {
                let mut object: HashMap<String, serde_json::Value> = HashMap::new();
                pairs.iter().map(|(key, value)| {
                    (key.as_json(), value.as_json())
                }).for_each(|(key, value)| {
                    match key {
                        serde_json::Value::Number(key_number) => object.insert(key_number.to_string(), value),
                        serde_json::Value::String(key_string) => object.insert(key_string, value),
                        _ => None,
                    };
                });
                json!(object)
            }
        }
    }
}


pub(crate) fn decode_bencoded_from_str(input: &str) -> Result<Value, std::io::Error> {
    decode_bencoded(&input.chars().collect())
}

pub(crate) fn decode_bencoded(input: &Vec<char>) -> Result<Value, std::io::Error> {
    //TODO: Return a Result, rather than use unwrap
    //TODO: Handle the case when not all of the encoded_value input has been read
    decode_bencoded_at_position(input, 0).0.ok_or(std::io::Error::new(std::io::ErrorKind::Other, "Could not decode input"))
}

fn decode_bencoded_at_position(input: &Vec<char>, position: usize) -> (Option<Value>, usize) {
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

fn decode_list(input: &Vec<char>, position: usize) -> (Option<Value>, usize) {
    let mut current_position: usize = position + 1; // skip 'l'
    let mut array_values: Vec<Value> = Vec::new();
    while input[current_position] != 'e' && current_position < input.len() {
        let (current_decoded_value, updated_position) = decode_bencoded_at_position(input, current_position);
        current_position = updated_position;
        if let Some(decoded_value) = current_decoded_value {
            array_values.push(decoded_value);
        }
    }
    if input[current_position] != 'e' {
        panic!("Malformed input '{:?}' around position {:?}", input, current_position);
    }
    (Some(Value::List(array_values)), current_position + 1) // skip 'e'
}

fn decode_dictionary(input: &Vec<char>, position: usize) -> (Option<Value>, usize) {
    let mut current_position: usize = position + 1; // skip 'd'
    let mut object: HashMap<Value, Value> = HashMap::new();

    while input[current_position] != 'e' && current_position < input.len() {
        let (maybe_key, new_position) = decode_bencoded_at_position(input, current_position);
        current_position = new_position;
        if input[current_position] != 'e' {
            if let Some(key) = maybe_key {
                let (maybe_value, new_position) = decode_bencoded_at_position(input, current_position);
                current_position = new_position;
                if let Some(value) = maybe_value {
                    object.insert(key, value);
                }
            }
        }
    }
    if input[current_position] != 'e' {
        panic!("Malformed input '{:?}' around position {:?}", input, current_position);
    }
    (Some(Value::Object(object.into_iter().collect())), current_position + 1) // skip 'e'
}

fn decode_string(input: &Vec<char>, position: usize) -> (Option<Value>, usize) {
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
    let string_value: Vec<u8> = input[current_position..current_position + string_value_length].into_iter().map(|ch| *ch as u8).collect();

    (Some(Value::String(string_value)), current_position + string_value_length)
}

fn decode_number(input: &Vec<char>, position: usize) -> (Option<Value>, usize) {
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
    (Some(Value::Number(number)), current_position + 1) // skip the 'e' symbol
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decode_bencoded_string() {
        assert_eq!(decode_bencoded_from_str("4:spam").unwrap().as_json(), json!("spam".to_string()));
    }

    #[test]
    fn decode_bencoded_integers() {
        assert_eq!(decode_bencoded_from_str("i52e").unwrap().as_json(), json!(52));
    }

    #[test]
    fn decode_bencoded_lists() {
        assert_eq!(decode_bencoded_from_str("l5:helloi52ee").unwrap().as_json(), json!(["hello", 52]));
    }

    #[test]
    fn decode_bencoded_dictionary() {
        assert_eq!(decode_bencoded_from_str("d3:foo3:bar5:helloi52ee").unwrap().as_json(), json!({
            "foo": "bar",
            "hello": 52
        }));
    }

    #[test]
    fn decode_empty_bencoded_dictionary() {
        assert_eq!(decode_bencoded_from_str("de").unwrap().as_json(), json!({}));
    }

    #[test]
    fn decode_bencoded_dictionary_which_misses_value() {
        assert_eq!(decode_bencoded_from_str("d3:foo3:bar5:helloe").unwrap().as_json(), json!({
            "foo": "bar"
        }));
    }

    #[test]
    fn decode_bencoded_nested_object() {
        assert_eq!(decode_bencoded_from_str("d10:inner_dictd4:key16:value14:key2i42eee").unwrap().as_json(), json!({
            "inner_dict": {
                "key1":"value1",
                "key2":42
            }
        }));
    }

    #[test]
    fn decode_bencoded_objects_in_an_array() {
        assert_eq!(decode_bencoded_from_str("ld4:key16:value1ed4:key26:value2ed4:key36:value3ee").unwrap().as_json(), json!([
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