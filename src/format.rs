pub(crate) fn format_as_hex_string(vec: &[u8]) -> String {
    vec.iter().map(|byte| format!("{:02x}", byte)).collect::<String>()
}

//TODO: Add tests