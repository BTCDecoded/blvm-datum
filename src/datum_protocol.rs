//! DATUM protocol implementation
//!
//! Implements the DATUM protocol for communication with DATUM pools (Ocean)
//!
//! Protocol uses:
//! - Ed25519 for message signing
//! - X25519 for encryption (ChaCha20Poly1305)
//! - Packed binary protocol header

use crate::error::DatumError;
use crate::handlers::{
    parse_block_notify, parse_client_config, parse_job_validation, parse_share_response,
    DefaultMessageHandler, MessageHandler,
};
use crate::messages::{DatumCommand, DatumMessageHeader};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use std::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream as AsyncTcpStream;
use tracing::{debug, info, warn};
use crypto_box::PublicKey as CryptoBoxPublicKey;
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};

/// DATUM protocol version
const DATUM_PROTOCOL_VERSION: &str = "v0.4.1-beta";
const DATUM_PROTOCOL_MAX_CMD_SIZE: usize = 4194304; // 2^22 bytes

/// DATUM protocol client
/// Uses interior mutability for thread-safe access
pub struct DatumProtocolClient {
    /// Pool URL (host:port)
    pool_url: String,
    /// Pool's long-term X25519 public key (for crypto_box_seal in handshake)
    pool_public_key: Option<[u8; 32]>,
    /// TCP connection (wrapped in RwLock for interior mutability)
    stream: tokio::sync::RwLock<Option<AsyncTcpStream>>,
    /// Local encryption keys
    local_keys: tokio::sync::RwLock<Option<DatumEncryptionKeys>>,
    /// Remote (pool) encryption keys
    remote_keys: tokio::sync::RwLock<Option<DatumEncryptionKeys>>,
    /// Session encryption cipher (after handshake)
    send_cipher: tokio::sync::RwLock<Option<ChaCha20Poly1305>>,
    recv_cipher: tokio::sync::RwLock<Option<ChaCha20Poly1305>>,
    /// Session nonces (wrapped in Mutex for atomic updates)
    send_nonce: tokio::sync::Mutex<u64>,
    recv_nonce: tokio::sync::Mutex<u64>,
    /// Header XOR key for obfuscation (updated with feedback mechanism)
    header_xor_key: tokio::sync::Mutex<u32>,
    /// Message handler for protocol messages
    message_handler: tokio::sync::RwLock<Box<dyn MessageHandler>>,
    /// Pool's Ed25519 public key (for signature verification)
    pool_ed25519_public: tokio::sync::RwLock<Option<VerifyingKey>>,
    /// Connection state (atomic for lock-free reads)
    connected: std::sync::atomic::AtomicBool,
}

/// DATUM encryption keys (Ed25519 for signing, X25519 for encryption)
struct DatumEncryptionKeys {
    /// Ed25519 signing key pair
    ed25519_signing: SigningKey,
    /// X25519 encryption key pair
    x25519_secret: EphemeralSecret,
    x25519_public: PublicKey,
}

impl DatumEncryptionKeys {
    /// Generate new encryption keys
    fn generate() -> Self {
        // Generate Ed25519 signing key
        let ed25519_signing = SigningKey::generate(&mut OsRng);

        // Generate X25519 encryption key
        let x25519_secret = EphemeralSecret::random_from_rng(&mut OsRng);
        let x25519_public = PublicKey::from(&x25519_secret);

        Self {
            ed25519_signing,
            x25519_secret,
            x25519_public,
        }
    }

    /// Get Ed25519 public key
    fn ed25519_public(&self) -> VerifyingKey {
        self.ed25519_signing.verifying_key()
    }

    /// Get X25519 public key bytes
    fn x25519_public_bytes(&self) -> [u8; 32] {
        self.x25519_public.as_bytes().to_owned()
    }
}

impl DatumProtocolClient {
    /// Create a new DATUM protocol client
    pub fn new(pool_url: String) -> Self {
        Self {
            pool_url,
            pool_public_key: None,
            stream: tokio::sync::RwLock::new(None),
            local_keys: tokio::sync::RwLock::new(None),
            remote_keys: tokio::sync::RwLock::new(None),
            send_cipher: tokio::sync::RwLock::new(None),
            recv_cipher: tokio::sync::RwLock::new(None),
            send_nonce: tokio::sync::Mutex::new(0),
            recv_nonce: tokio::sync::Mutex::new(0),
            header_xor_key: tokio::sync::Mutex::new(0),
            pool_ed25519_public: tokio::sync::RwLock::new(None),
            message_handler: tokio::sync::RwLock::new(Box::new(DefaultMessageHandler)),
            connected: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Create a new DATUM protocol client with pool public key
    pub fn new_with_pool_key(pool_url: String, pool_public_key: [u8; 32]) -> Self {
        Self {
            pool_url,
            pool_public_key: Some(pool_public_key),
            stream: tokio::sync::RwLock::new(None),
            local_keys: tokio::sync::RwLock::new(None),
            remote_keys: tokio::sync::RwLock::new(None),
            send_cipher: tokio::sync::RwLock::new(None),
            recv_cipher: tokio::sync::RwLock::new(None),
            send_nonce: tokio::sync::Mutex::new(0),
            recv_nonce: tokio::sync::Mutex::new(0),
            header_xor_key: tokio::sync::Mutex::new(0),
            pool_ed25519_public: tokio::sync::RwLock::new(None),
            message_handler: tokio::sync::RwLock::new(Box::new(DefaultMessageHandler)),
            connected: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Set pool public key (for crypto_box_seal)
    pub fn set_pool_public_key(&mut self, key: [u8; 32]) {
        self.pool_public_key = Some(key);
    }

    /// Set message handler
    pub async fn set_message_handler(&self, handler: Box<dyn MessageHandler>) {
        *self.message_handler.write().await = handler;
    }

    /// Connect to DATUM pool and perform handshake
    pub async fn connect(&self) -> Result<(), DatumError> {
        info!("Connecting to DATUM pool at {}", self.pool_url);

        // Parse pool URL (format: host:port or tcp://host:port)
        let addr = self
            .pool_url
            .strip_prefix("tcp://")
            .unwrap_or(&self.pool_url);

        // Connect to pool
        let stream = AsyncTcpStream::connect(addr)
            .await
            .map_err(|e| DatumError::PoolConnectionError(format!("Failed to connect: {}", e)))?;
        *self.stream.write().await = Some(stream);

        // Generate local encryption keys
        *self.local_keys.write().await = Some(DatumEncryptionKeys::generate());

        // Perform handshake
        self.perform_handshake().await?;

        self.connected
            .store(true, std::sync::atomic::Ordering::Release);
        info!("Successfully connected to DATUM pool");
        Ok(())
    }

    /// Perform DATUM protocol handshake
    async fn perform_handshake(&self) -> Result<(), DatumError> {
        let local_keys_guard = self.local_keys.read().await;
        let local_keys = local_keys_guard
            .as_ref()
            .ok_or_else(|| DatumError::ProtocolError("Local keys not generated".to_string()))?;

        debug!("Performing DATUM protocol handshake");

        // Generate session keys (separate from long-term keys)
        let session_keys = DatumEncryptionKeys::generate();

        // Build hello message:
        // [local_ed25519_pk(32)][local_x25519_pk(32)][session_ed25519_pk(32)][session_x25519_pk(32)][version][header_key(4)][padding][signature(64)]
        let mut hello_msg = Vec::new();

        // Local Ed25519 public key (32 bytes)
        hello_msg.extend_from_slice(&local_keys.ed25519_public().to_bytes());

        // Local X25519 public key (32 bytes)
        hello_msg.extend_from_slice(&local_keys.x25519_public_bytes());

        // Session Ed25519 public key (32 bytes)
        hello_msg.extend_from_slice(&session_keys.ed25519_public().to_bytes());

        // Session X25519 public key (32 bytes)
        hello_msg.extend_from_slice(&session_keys.x25519_public_bytes());

        // Version string
        hello_msg.extend_from_slice(DATUM_PROTOCOL_VERSION.as_bytes());
        hello_msg.push(0); // null terminator

        // Initial header XOR key (4 bytes, random)
        use rand::Rng;
        let mut rng = OsRng;
        let header_key = rng.gen::<u32>();
        hello_msg.extend_from_slice(&header_key.to_le_bytes());

        // Padding (random 1-200 bytes)
        let padding_len = rng.gen_range(1..=200);
        let padding: Vec<u8> = (0..padding_len).map(|_| rng.gen()).collect();
        hello_msg.extend_from_slice(&padding);

        // Sign the message with local Ed25519 key
        let signature = local_keys.ed25519_signing.sign(&hello_msg);
        hello_msg.extend_from_slice(&signature.to_bytes());

        // Encrypt with pool's X25519 public key using crypto_box_seal (NaCl sealed box)
        // Uses crypto_box crate (pure Rust) to avoid blake2b linker conflict with sparse-merkle-tree
        let encrypted_hello = if let Some(pool_pk_bytes) = self.pool_public_key {
            let pool_pk = CryptoBoxPublicKey::from_slice(&pool_pk_bytes).map_err(|e| {
                DatumError::EncryptionError(format!("Invalid pool public key: {}", e))
            })?;
            let mut rng = OsRng;
            pool_pk
                .seal(&mut rng, &hello_msg)
                .map_err(|e| DatumError::EncryptionError(e.to_string()))?
        } else {
            // If no pool public key, send unencrypted (for testing/development)
            warn!("No pool public key configured - sending handshake unencrypted");
            hello_msg
        };

        // Store initial header XOR key
        *self.header_xor_key.lock().await = header_key;

        // Send handshake message
        let mut stream_guard = self.stream.write().await;
        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| DatumError::PoolConnectionError("Stream not available".to_string()))?;

        // Build protocol header
        let header = DatumMessageHeader {
            cmd_len: encrypted_hello.len() as u32,
            reserved: 0,
            is_signed: true,
            is_encrypted_pubkey: self.pool_public_key.is_some(), // Handshake uses public key encryption if key available
            is_encrypted_channel: false,                         // Not using channel encryption yet
            proto_cmd: DatumCommand::Handshake as u8,
        };

        let header_bytes = self.serialize_header(&header)?;

        // XOR header with initial key (obfuscation)
        let header_xor = self.apply_header_xor(&header_bytes, header_key).await?;

        // Send header and message
        stream
            .write_all(&header_xor)
            .await
            .map_err(|e| DatumError::IoError(e))?;
        stream
            .write_all(&encrypted_hello)
            .await
            .map_err(|e| DatumError::IoError(e))?;
        stream.flush().await.map_err(|e| DatumError::IoError(e))?;

        drop(stream_guard);

        // Receive handshake response
        let (command, response_data) = self.receive_message_internal().await?;
        if command != DatumCommand::Handshake {
            return Err(DatumError::ProtocolError(
                "Expected handshake response".to_string(),
            ));
        }

        // Parse handshake response:
        // [echo_local_ed25519(32)][echo_local_x25519(32)][echo_session_ed25519(32)][echo_session_x25519(32)][pool_session_ed25519(32)][pool_session_x25519(32)][motd][signature(64)]
        if response_data.len() < 32 * 6 {
            return Err(DatumError::ProtocolError(
                "Handshake response too short".to_string(),
            ));
        }

        let mut offset = 0;

        // Verify echoed keys match
        let local_ed25519_pk = local_keys.ed25519_public().to_bytes();
        if &response_data[offset..offset + 32] != &local_ed25519_pk {
            return Err(DatumError::ProtocolError(
                "Echoed Ed25519 key mismatch".to_string(),
            ));
        }
        offset += 32;

        let local_x25519_pk = local_keys.x25519_public_bytes();
        if &response_data[offset..offset + 32] != &local_x25519_pk {
            return Err(DatumError::ProtocolError(
                "Echoed X25519 key mismatch".to_string(),
            ));
        }
        offset += 32;

        let session_ed25519_pk = session_keys.ed25519_public().to_bytes();
        if &response_data[offset..offset + 32] != &session_ed25519_pk {
            return Err(DatumError::ProtocolError(
                "Echoed session Ed25519 key mismatch".to_string(),
            ));
        }
        offset += 32;

        let session_x25519_pk = session_keys.x25519_public_bytes();
        if &response_data[offset..offset + 32] != &session_x25519_pk {
            return Err(DatumError::ProtocolError(
                "Echoed session X25519 key mismatch".to_string(),
            ));
        }
        offset += 32;

        // Extract pool's session keys
        let pool_session_ed25519_pk_bytes: [u8; 32] = response_data[offset..offset + 32]
            .try_into()
            .map_err(|_| DatumError::ProtocolError("Invalid pool Ed25519 key".to_string()))?;
        let pool_session_ed25519_pk = VerifyingKey::from_bytes(&pool_session_ed25519_pk_bytes)
            .map_err(|e| DatumError::ProtocolError(format!("Invalid Ed25519 key: {}", e)))?;
        offset += 32;

        // Store pool's Ed25519 public key for signature verification
        *self.pool_ed25519_public.write().await = Some(pool_session_ed25519_pk);

        let pool_session_x25519_pk_bytes: [u8; 32] = response_data[offset..offset + 32]
            .try_into()
            .map_err(|_| DatumError::ProtocolError("Invalid pool X25519 key".to_string()))?;
        let pool_session_x25519_pk = PublicKey::from(pool_session_x25519_pk_bytes);
        offset += 32;

        // Extract MOTD (null-terminated string)
        let motd_end = response_data[offset..]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(response_data.len() - offset);
        let motd = String::from_utf8_lossy(&response_data[offset..offset + motd_end]);
        info!("DATUM Server MOTD: {}", motd);
        offset += motd_end + 1;

        // Derive shared secret using X25519
        // x25519-dalek: EphemeralSecret * PublicKey = SharedSecret
        let shared_secret = session_keys
            .x25519_secret
            .diffie_hellman(&pool_session_x25519_pk);

        // Derive encryption keys from shared secret using HKDF
        // DATUM uses a simple key derivation (could be improved)
        // SharedSecret implements AsRef<[u8]>
        let shared_secret_bytes = shared_secret.as_ref();
        let send_key = self.derive_key_from_shared_secret(shared_secret_bytes, b"send");
        let recv_key = self.derive_key_from_shared_secret(shared_secret_bytes, b"recv");

        // Initialize ChaCha20Poly1305 ciphers
        *self.send_cipher.write().await = Some(ChaCha20Poly1305::new(&Key::from_slice(&send_key)));
        *self.recv_cipher.write().await = Some(ChaCha20Poly1305::new(&Key::from_slice(&recv_key)));

        // Store remote keys
        // Note: We only store the public keys from the pool, not the signing key
        // The pool's Ed25519 public key is used for verification, not signing
        // For the struct, we'll create a dummy signing key (not used for remote keys)
        let dummy_signing_key = SigningKey::generate(&mut OsRng);
        *self.remote_keys.write().await = Some(DatumEncryptionKeys {
            ed25519_signing: dummy_signing_key, // Not used for remote keys
            x25519_secret: EphemeralSecret::random_from_rng(&mut OsRng), // Not used, but needed for struct
            x25519_public: pool_session_x25519_pk,
        });

        // Initialize session nonces deterministically based on header key (DATUM protocol spec)
        let initial_nonce = self.initialize_session_nonce(header_key);
        *self.send_nonce.lock().await = initial_nonce;
        *self.recv_nonce.lock().await = initial_nonce;

        info!("DATUM handshake completed successfully");
        Ok(())
    }

    /// Derive encryption key from shared secret (simple HKDF-like derivation)
    fn derive_key_from_shared_secret(&self, shared_secret: &[u8], label: &[u8]) -> [u8; 32] {
        Self::derive_key_from_shared_secret_static(shared_secret, label)
    }

    /// Send encrypted message to pool
    pub async fn send_message(&self, command: DatumCommand, data: &[u8]) -> Result<(), DatumError> {
        if !self.connected.load(std::sync::atomic::Ordering::Acquire) {
            return Err(DatumError::PoolConnectionError(
                "Not connected to pool".to_string(),
            ));
        }

        let mut stream_guard = self.stream.write().await;
        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| DatumError::PoolConnectionError("Stream not available".to_string()))?;

        // Build protocol header
        let header = DatumMessageHeader {
            cmd_len: data.len() as u32,
            reserved: 0,
            is_signed: true,
            is_encrypted_pubkey: false,
            is_encrypted_channel: true,
            proto_cmd: command as u8,
        };

        // Serialize header (packed binary format)
        let header_bytes = self.serialize_header(&header)?;

        // Apply header XOR obfuscation with feedback
        let header_xor_key = *self.header_xor_key.lock().await;
        let header_xor = self.apply_header_xor(&header_bytes, header_xor_key).await?;

        // Get nonce and increment counter
        let mut send_nonce_guard = self.send_nonce.lock().await;
        let send_nonce_value = *send_nonce_guard;
        *send_nonce_guard += 1;
        drop(send_nonce_guard);

        // Encrypt data if cipher is available
        let encrypted_data = {
            let cipher_guard = self.send_cipher.read().await;
            if let Some(cipher) = cipher_guard.as_ref() {
                let nonce = self.create_nonce(send_nonce_value);
                cipher
                    .encrypt(&nonce, data)
                    .map_err(|e| DatumError::EncryptionError(format!("Encryption failed: {}", e)))?
            } else {
                data.to_vec()
            }
        };

        // Sign message if keys available
        // Note: Signature is over the original header (before XOR), not the obfuscated one
        let signature = {
            let keys_guard = self.local_keys.read().await;
            if let Some(keys) = keys_guard.as_ref() {
                let mut message = header_bytes.clone();
                message.extend_from_slice(&encrypted_data);
                keys.ed25519_signing.sign(&message).to_bytes().to_vec()
            } else {
                vec![]
            }
        };

        // Send: [header][signature][encrypted_data]
        stream
            .write_all(&header_xor)
            .await
            .map_err(|e| DatumError::IoError(e))?;
        if !signature.is_empty() {
            stream
                .write_all(&signature)
                .await
                .map_err(|e| DatumError::IoError(e))?;
        }
        stream
            .write_all(&encrypted_data)
            .await
            .map_err(|e| DatumError::IoError(e))?;

        Ok(())
    }

    /// Receive message from pool (internal, doesn't auto-handle)
    async fn receive_message_internal(&self) -> Result<(DatumCommand, Vec<u8>), DatumError> {
        let mut stream_guard = self.stream.write().await;
        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| DatumError::PoolConnectionError("Stream not available".to_string()))?;

        // Read header (4 bytes packed)
        let mut header_bytes = [0u8; 4];
        stream
            .read_exact(&mut header_bytes)
            .await
            .map_err(|e| DatumError::IoError(e))?;

        // Apply header XOR deobfuscation with feedback
        let header_xor_key = *self.header_xor_key.lock().await;
        let header_deobfuscated = self.apply_header_xor(&header_bytes, header_xor_key).await?;

        let header = self.deserialize_header(&header_deobfuscated)?;

        // Read signature if present (64 bytes for Ed25519)
        let signature_bytes = if header.is_signed {
            let mut sig = vec![0u8; 64];
            stream
                .read_exact(&mut sig)
                .await
                .map_err(|e| DatumError::IoError(e))?;
            Some(sig)
        } else {
            None
        };

        // Read encrypted data
        let mut encrypted_data = vec![0u8; header.cmd_len as usize];
        stream
            .read_exact(&mut encrypted_data)
            .await
            .map_err(|e| DatumError::IoError(e))?;

        // Verify signature if present and pool's public key is known
        if let Some(sig_bytes) = &signature_bytes {
            let pool_pk_guard = self.pool_ed25519_public.read().await;
            if let Some(pool_pk) = pool_pk_guard.as_ref() {
                // Reconstruct message for verification (header + encrypted_data)
                let mut message = header_deobfuscated.clone();
                message.extend_from_slice(&encrypted_data);

                // Verify signature
                let signature =
                    Signature::from_bytes(sig_bytes.as_slice().try_into().map_err(|_| {
                        DatumError::ProtocolError("Invalid signature format".to_string())
                    })?);

                if pool_pk.verify(&message, &signature).is_err() {
                    return Err(DatumError::ProtocolError(
                        "Signature verification failed".to_string(),
                    ));
                }
                debug!("Message signature verified successfully");
            }
        }

        // Get nonce and increment counter
        let mut recv_nonce_guard = self.recv_nonce.lock().await;
        let recv_nonce_value = *recv_nonce_guard;
        *recv_nonce_guard += 1;
        drop(recv_nonce_guard);

        // Decrypt data if cipher is available
        let data = {
            let cipher_guard = self.recv_cipher.read().await;
            if let Some(cipher) = cipher_guard.as_ref() {
                let nonce = self.create_nonce(recv_nonce_value);
                cipher
                    .decrypt(&nonce, encrypted_data.as_ref())
                    .map_err(|e| DatumError::EncryptionError(format!("Decryption failed: {}", e)))?
            } else {
                encrypted_data
            }
        };

        let command = DatumCommand::from_u8(header.proto_cmd).ok_or_else(|| {
            DatumError::ProtocolError(format!("Unknown command: {}", header.proto_cmd))
        })?;

        Ok((command, data))
    }

    /// Receive message from pool (public API, auto-handles protocol messages)
    pub async fn receive_message(&self) -> Result<(DatumCommand, Vec<u8>), DatumError> {
        let (command, data) = self.receive_message_internal().await?;

        // Handle protocol messages
        self.handle_protocol_message(command, &data).await?;

        Ok((command, data))
    }

    /// Handle incoming protocol messages
    async fn handle_protocol_message(
        &self,
        command: DatumCommand,
        data: &[u8],
    ) -> Result<(), DatumError> {
        let handler = self.message_handler.read().await;

        match command {
            DatumCommand::ClientConfig => {
                let config = parse_client_config(data)?;
                handler.handle_client_config(config)?;
            }
            DatumCommand::JobValidation => {
                let (job_id, result) = parse_job_validation(data)?;
                handler.handle_job_validation(job_id, result)?;
            }
            DatumCommand::ShareResponse => {
                let response = parse_share_response(data)?;
                handler.handle_share_response(response)?;
            }
            DatumCommand::BlockNotify => {
                let notify = parse_block_notify(data)?;
                handler.handle_block_notify(notify)?;
            }
            _ => {
                // Other commands handled elsewhere
            }
        }

        Ok(())
    }

    /// Serialize protocol header to packed binary format
    fn serialize_header(&self, header: &DatumMessageHeader) -> Result<Vec<u8>, DatumError> {
        Self::serialize_header_static(header)
    }

    /// Serialize protocol header to packed binary format (static for testing)
    pub fn serialize_header_static(header: &DatumMessageHeader) -> Result<Vec<u8>, DatumError> {
        // Packed format: [cmd_len:22][reserved:2][flags:3][proto_cmd:5]
        let mut bytes = vec![0u8; 4];

        // cmd_len (22 bits, little-endian)
        bytes[0] = (header.cmd_len & 0xFF) as u8;
        bytes[1] = ((header.cmd_len >> 8) & 0xFF) as u8;
        bytes[2] = ((header.cmd_len >> 16) & 0x3F) as u8; // Only 6 bits

        // reserved (2 bits) + flags (3 bits) + proto_cmd (5 bits) in byte 2 and 3
        bytes[2] |= (header.reserved & 0x3) << 6;
        bytes[3] = (header.is_signed as u8)
            | ((header.is_encrypted_pubkey as u8) << 1)
            | ((header.is_encrypted_channel as u8) << 2)
            | (header.proto_cmd << 3);

        Ok(bytes)
    }

    /// Deserialize protocol header from packed binary format
    fn deserialize_header(&self, bytes: &[u8]) -> Result<DatumMessageHeader, DatumError> {
        Self::deserialize_header_static(bytes)
    }

    /// Deserialize protocol header from packed binary format (static for testing)
    pub fn deserialize_header_static(bytes: &[u8]) -> Result<DatumMessageHeader, DatumError> {
        if bytes.len() < 4 {
            return Err(DatumError::ProtocolError("Header too short".to_string()));
        }

        // Unpack: [cmd_len:22][reserved:2][flags:3][proto_cmd:5]
        let cmd_len =
            (bytes[0] as u32) | ((bytes[1] as u32) << 8) | ((bytes[2] as u32 & 0x3F) << 16);

        let reserved = (bytes[2] >> 6) & 0x3;
        let flags_byte = bytes[3];
        let is_signed = (flags_byte & 0x1) != 0;
        let is_encrypted_pubkey = (flags_byte & 0x2) != 0;
        let is_encrypted_channel = (flags_byte & 0x4) != 0;
        let proto_cmd = flags_byte >> 3;

        Ok(DatumMessageHeader {
            cmd_len,
            reserved,
            is_signed,
            is_encrypted_pubkey,
            is_encrypted_channel,
            proto_cmd,
        })
    }

    /// Create nonce for encryption (12 bytes: 8-byte counter + 4-byte zero)
    fn create_nonce(&self, counter: u64) -> Nonce {
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..8].copy_from_slice(&counter.to_le_bytes());
        // Nonce::from_slice returns a reference, we need to clone the underlying array
        // Nonce is GenericArray which implements Clone
        Nonce::from_slice(&nonce_bytes).clone()
    }

    /// Apply header XOR obfuscation with feedback mechanism
    /// DATUM protocol uses a feedback mechanism where the XOR key is updated based on the header
    async fn apply_header_xor(
        &self,
        header_bytes: &[u8],
        xor_key: u32,
    ) -> Result<Vec<u8>, DatumError> {
        if header_bytes.len() < 4 {
            return Err(DatumError::ProtocolError(
                "Header too short for XOR".to_string(),
            ));
        }

        let mut result = header_bytes.to_vec();

        // Apply XOR to first 4 bytes
        for (i, byte) in result.iter_mut().take(4).enumerate() {
            *byte ^= ((xor_key >> (i * 8)) & 0xFF) as u8;
        }

        // Update header XOR key using feedback mechanism
        // Feedback: new_key = old_key XOR (header_bytes as u32)
        let header_as_u32 = u32::from_le_bytes([result[0], result[1], result[2], result[3]]);
        let new_key = xor_key ^ header_as_u32;
        *self.header_xor_key.lock().await = new_key;

        Ok(result)
    }

    /// Initialize session nonce deterministically based on header key (DATUM protocol spec)
    /// The nonce is derived from the header XOR key to ensure both sides start with the same value
    fn initialize_session_nonce(&self, header_key: u32) -> u64 {
        Self::initialize_session_nonce_static(header_key)
    }

    /// Initialize session nonce deterministically (static for testing)
    pub fn initialize_session_nonce_static(header_key: u32) -> u64 {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"datum_nonce_init");
        hasher.update(&header_key.to_le_bytes());
        hasher.update(DATUM_PROTOCOL_VERSION.as_bytes());
        let hash = hasher.finalize();

        // Use first 8 bytes of hash as initial nonce counter
        let mut nonce_bytes = [0u8; 8];
        nonce_bytes.copy_from_slice(&hash[..8]);
        u64::from_le_bytes(nonce_bytes)
    }

    /// Derive encryption key from shared secret (static for testing)
    pub fn derive_key_from_shared_secret_static(shared_secret: &[u8], label: &[u8]) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(shared_secret);
        hasher.update(label);
        hasher.update(b"datum_v0.4.1"); // Protocol version for key derivation
        let hash = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&hash);
        key
    }

    /// Fetch coinbase payout information from pool
    pub async fn fetch_coinbaser(&self, value: u64) -> Result<Vec<u8>, DatumError> {
        debug!("Fetching coinbaser from pool for value: {}", value);

        // Serialize request (value as u64, little-endian)
        let request_data = value.to_le_bytes().to_vec();

        // Send fetch coinbaser command
        self.send_message(DatumCommand::FetchCoinbaser, &request_data)
            .await?;

        // Receive response
        let (command, data) = self.receive_message().await?;
        if command != DatumCommand::FetchCoinbaser {
            return Err(DatumError::ProtocolError(
                "Unexpected response command".to_string(),
            ));
        }

        Ok(data)
    }

    /// Submit proof of work to pool
    pub async fn submit_pow(&self, pow_data: Vec<u8>) -> Result<bool, DatumError> {
        debug!("Submitting proof of work to pool");

        // Send POW submission command
        self.send_message(DatumCommand::SubmitPow, &pow_data)
            .await?;

        // Receive response
        let (command, data) = self.receive_message().await?;
        if command != DatumCommand::SubmitPow {
            return Err(DatumError::ProtocolError(
                "Unexpected response command".to_string(),
            ));
        }

        // Parse response (0x50 = accepted, 0x66 = rejected)
        if data.is_empty() {
            return Err(DatumError::ProtocolError("Empty response".to_string()));
        }

        let accepted = match data[0] {
            0x50 => true,  // Accepted
            0x55 => true,  // Accepted tentatively
            0x66 => false, // Rejected
            _ => {
                return Err(DatumError::ProtocolError(format!(
                    "Unknown response code: {}",
                    data[0]
                )))
            }
        };

        Ok(accepted)
    }
}
