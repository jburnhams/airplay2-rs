use super::{CryptoError, lengths};
use aes::Aes128;
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockEncrypt, KeyInit};

/// AES-128-CTR stream cipher for audio encryption
pub struct Aes128Ctr {
    cipher: Aes128,
    // Initial counter block (IV) to support seek(0)
    base_counter_block: [u8; 16],
    // Current counter block
    counter_block: [u8; 16],
    // Keystream buffer
    keystream_buffer: [u8; 16],
    // Position in the current keystream buffer (0..16)
    keystream_pos: usize,
}

impl Aes128Ctr {
    /// Create cipher with 16-byte key and 16-byte IV
    pub fn new(key: &[u8], iv: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != lengths::AES_128_KEY {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::AES_128_KEY,
                actual: key.len(),
            });
        }
        if iv.len() != 16 {
            return Err(CryptoError::InvalidKeyLength {
                expected: 16,
                actual: iv.len(),
            });
        }

        let key_generic = GenericArray::from_slice(key);
        let cipher = Aes128::new(key_generic);

        Ok(Self::new_internal(cipher, iv))
    }

    /// Create cipher from existing block cipher instance and IV
    pub fn new_with_cipher(cipher: &Aes128, iv: &[u8]) -> Result<Self, CryptoError> {
        if iv.len() != 16 {
            return Err(CryptoError::InvalidKeyLength {
                expected: 16,
                actual: iv.len(),
            });
        }

        Ok(Self::new_internal(cipher.clone(), iv))
    }

    fn new_internal(cipher: Aes128, iv: &[u8]) -> Self {
        let mut block = [0u8; 16];
        block.copy_from_slice(iv);

        Self {
            cipher,
            base_counter_block: block,
            counter_block: block,
            keystream_buffer: [0u8; 16],
            keystream_pos: 16, // Force generation on first use
        }
    }

    /// Encrypt/decrypt in place (XOR with keystream)
    pub fn apply_keystream(&mut self, mut data: &mut [u8]) {
        // 1. Consume remaining bytes in the buffer
        if self.keystream_pos < 16 {
            let available = 16 - self.keystream_pos;
            let count = std::cmp::min(data.len(), available);
            for i in 0..count {
                data[i] ^= self.keystream_buffer[self.keystream_pos + i];
            }
            data = &mut data[count..];
            self.keystream_pos += count;
        }

        // 2. Process aligned blocks in parallel (8 blocks / 128 bytes at a time)
        const PAR_BLOCKS: usize = 8;
        const BLOCK_SIZE: usize = 16;
        const CHUNK_SIZE: usize = PAR_BLOCKS * BLOCK_SIZE;

        if data.len() >= CHUNK_SIZE {
            let mut blocks: [GenericArray<u8, aes::cipher::consts::U16>; PAR_BLOCKS] =
                [GenericArray::default(); PAR_BLOCKS];

            while data.len() >= CHUNK_SIZE {
                // Fill blocks with current counters
                for block in &mut blocks {
                    block.copy_from_slice(&self.counter_block);
                    self.increment_counter();
                }

                // Encrypt all blocks in parallel (utilizes AES-NI/pipelining)
                self.cipher.encrypt_blocks(&mut blocks);

                // XOR keystream with data
                for (i, block) in blocks.iter().enumerate() {
                    let offset = i * BLOCK_SIZE;
                    let chunk = &mut data[offset..offset + BLOCK_SIZE];
                    for j in 0..BLOCK_SIZE {
                        chunk[j] ^= block[j];
                    }
                }

                data = &mut data[CHUNK_SIZE..];
            }
        }

        // 3. Process remaining full blocks individually
        // (and partial end block)
        while !data.is_empty() {
            // Refill buffer
            self.keystream_buffer.copy_from_slice(&self.counter_block);
            let block = GenericArray::from_mut_slice(&mut self.keystream_buffer);
            self.cipher.encrypt_block(block);
            self.increment_counter();
            self.keystream_pos = 0;

            let count = std::cmp::min(data.len(), 16);
            for i in 0..count {
                data[i] ^= self.keystream_buffer[i];
            }
            data = &mut data[count..];
            self.keystream_pos += count;
        }
    }

    fn increment_counter(&mut self) {
        // Ctr64BE: Increment last 8 bytes as big-endian integer
        let mut ctr = u64::from_be_bytes(self.counter_block[8..16].try_into().unwrap());
        ctr = ctr.wrapping_add(1);
        self.counter_block[8..16].copy_from_slice(&ctr.to_be_bytes());
    }

    /// Encrypt/decrypt, returning new buffer
    pub fn process(&mut self, data: &[u8]) -> Vec<u8> {
        let mut output = data.to_vec();
        self.apply_keystream(&mut output);
        output
    }

    /// Seek to position in keystream
    pub fn seek(&mut self, position: u64) {
        let block_offset = position / 16;
        let byte_offset = (position % 16) as usize;

        // Reset to base and add offset
        let mut ctr = u64::from_be_bytes(self.base_counter_block[8..16].try_into().unwrap());
        ctr = ctr.wrapping_add(block_offset);

        self.counter_block = self.base_counter_block;
        self.counter_block[8..16].copy_from_slice(&ctr.to_be_bytes());

        self.keystream_pos = 16; // Force regeneration

        if byte_offset > 0 {
            // Generate the block for the current position
            self.keystream_buffer.copy_from_slice(&self.counter_block);
            let block = GenericArray::from_mut_slice(&mut self.keystream_buffer);
            self.cipher.encrypt_block(block);

            // Increment counter for next block (prepare for next iteration)
            self.increment_counter();

            self.keystream_pos = byte_offset;
        }
    }
}

/// AES-128-GCM AEAD cipher
pub struct Aes128Gcm {
    cipher: aes_gcm::Aes128Gcm,
}

impl Aes128Gcm {
    /// Create cipher with 16-byte key
    pub fn new(key: &[u8]) -> Result<Self, CryptoError> {
        use aes_gcm::KeyInit;

        if key.len() != lengths::AES_128_KEY {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::AES_128_KEY,
                actual: key.len(),
            });
        }

        let key_generic = aes_gcm::Key::<aes_gcm::Aes128Gcm>::try_from(key).map_err(|_| {
            CryptoError::InvalidKeyLength {
                expected: 16,
                actual: key.len(),
            }
        })?;
        let cipher = aes_gcm::Aes128Gcm::new(&key_generic);

        Ok(Self { cipher })
    }

    /// Encrypt with 12-byte nonce
    pub fn encrypt(&self, nonce: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use aes_gcm::aead::Aead;

        if nonce.len() != lengths::AES_GCM_NONCE {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::AES_GCM_NONCE,
                actual: nonce.len(),
            });
        }

        let nonce_generic =
            aes_gcm::Nonce::try_from(nonce).map_err(|_| CryptoError::InvalidKeyLength {
                expected: lengths::AES_GCM_NONCE,
                actual: nonce.len(),
            })?;

        self.cipher
            .encrypt(&nonce_generic, plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// Decrypt with 12-byte nonce
    pub fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use aes_gcm::aead::Aead;

        if nonce.len() != lengths::AES_GCM_NONCE {
            return Err(CryptoError::InvalidKeyLength {
                expected: lengths::AES_GCM_NONCE,
                actual: nonce.len(),
            });
        }

        let nonce_generic =
            aes_gcm::Nonce::try_from(nonce).map_err(|_| CryptoError::InvalidKeyLength {
                expected: lengths::AES_GCM_NONCE,
                actual: nonce.len(),
            })?;

        self.cipher
            .decrypt(&nonce_generic, ciphertext)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}
