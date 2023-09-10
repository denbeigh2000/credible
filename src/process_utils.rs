pub fn process_msg(process_name: &str, raw: Vec<u8>) -> String {
    String::from_utf8(raw).unwrap_or_else(|e| {
        log::warn!("{process_name} returned non-utf8 stderr ({e})");
        "<Unknown>".to_string()
    })
}
