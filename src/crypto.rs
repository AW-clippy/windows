use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

#[derive(Clone)]
pub struct ClipboardCrypto {
    cipher: Aes256Gcm,
}

impl ClipboardCrypto {
    pub fn new(key: &[u8; 32]) -> Self {
        let cipher = Aes256Gcm::new(key.into());
        Self { cipher }
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, String> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Prepend 8-byte millisecond timestamp to plaintext for replay protection
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("System time error: {}", e))?
            .as_millis() as u64;

        let mut data_to_encrypt = now_ms.to_be_bytes().to_vec();
        data_to_encrypt.extend_from_slice(plaintext.as_bytes());

        let ciphertext = self.cipher
            .encrypt(nonce, data_to_encrypt.as_slice())
            .map_err(|e| format!("Encryption failed: {}", e))?;

        // Prepend nonce to ciphertext: [12 bytes nonce][ciphertext + tag]
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<String, String> {
        if data.len() < 12 {
            return Err("Data too short".into());
        }

        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let decrypted_bytes = self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failed: {}", e))?;

        if decrypted_bytes.len() < 8 {
            return Err("Decrypted data too short for timestamp".into());
        }

        // Verify timestamp within 60 seconds to prevent replay attacks
        let (ts_bytes, text_bytes) = decrypted_bytes.split_at(8);
        let packet_time_ms = u64::from_be_bytes(ts_bytes.try_into().unwrap());
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("System time error: {}", e))?
            .as_millis() as u64;

        let diff = if now_ms > packet_time_ms {
            now_ms - packet_time_ms
        } else {
            packet_time_ms - now_ms
        };

        if diff > 60_000 {
            return Err(format!("Replay attack or clock skew detected: packet diff is {}ms (limit 60000ms)", diff));
        }

        String::from_utf8(text_bytes.to_vec()).map_err(|e| format!("UTF-8 decode error: {}", e))
    }
}
