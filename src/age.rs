use age::armor::ArmoredReader;
use age::cli_common::file_io::InputReader;
use age::Decryptor;

use std::path::Path;

pub async fn decrypt_bytes(encrypted_bytes: &[u8], decryptor: ) -> Result<Vec<u8>, age::DecryptError> {
    let key_path_str = file_path.to_str().unwrap().to_string();
    let reader = InputReader::new(Some(key_path_str));
}
