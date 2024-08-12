pub(crate) fn url_encode_bytes(bytes: &[u8]) -> String {
    let encoded_bytes: Vec<String> = bytes.iter()
        .map(|b: &u8| format!("%{:02X}", b))
        .collect();
    encoded_bytes.join("")
}

//TODO: Add tests