use super::CryptoError;
use num_bigint::{BigUint, RandomBits};
use num_traits::One;
use rand::Rng;
use sha2::{Digest, Sha512};
use zeroize::Zeroize;

/// SRP Parameters
#[derive(Debug, Clone)]
pub struct SrpParams {
    pub n: BigUint,
    pub g: BigUint,
    pub k: BigUint,
}

impl SrpParams {
    /// RFC 5054 3072-bit group
    ///
    /// The constants are parsed on first access or when constructing the static.
    /// Since BigUint cannot be const, we provide a function or use `once_cell` if available,
    /// but here we'll just reconstruct it or rely on the fact that `SrpClient` parses it.
    ///
    /// However, `SrpClient::new` currently parses it every time.
    /// Let's optimize slightly by having a dedicated function or just keeping the parsing logic.
    /// For now, to match the requested API `&SRP_PARAMS`, we can't easily have a const static `BigUint`.
    ///
    /// We'll define a struct with `&'static str` and methods to get `BigUint`.
    pub const RFC5054_3072: SrpGroup = SrpGroup {
        n_hex: "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E08\
              8A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B\
              302B0A6DF25F14374FE1356D6D51C245E485B576625E7EC6F44C42E9\
              A637ED6B0BFF5CB6F406B7EDEE386BFB5A899FA5AE9F24117C4B1FE6\
              49286651ECE45B3DC2007CB8A163BF0598DA48361C55D39A69163FA8\
              FD24CF5F83655D23DCA3AD961C62F356208552BB9ED529077096966D\
              670C354E4ABC9804F1746C08CA18217C32905E462E36CE3BE39E772C\
              180E86039B2783A2EC07A28FB5C55DF06F4C52C9DE2BCBF695581718\
              3995497CEA956AE515D2261898FA051015728E5A8AAAC42DAD33170D\
              04507A33A85521ABDF1CBA64ECFB850458DBEF0A8AEA71575D060C7D\
              B3970F85A6E1E4C7ABF5AE8CDB0933D71E8C94E04A25619DCEE3D226\
              1AD2EE6BF12FFA06D98A0864D87602733EC86A64521F2B18177B200C\
              BBE117577A615D6C770988C0BAD946E208E24FA074E5AB3143DB5BFC\
              E0FD108E4B82D120A93AD2CAFFFFFFFFFFFFFFFF",
        g: 5,
    };
}

/// Helper struct for static parameters
#[derive(Debug, Clone, Copy)]
pub struct SrpGroup {
    pub n_hex: &'static str,
    pub g: u32,
}

impl SrpGroup {
    fn to_params(&self) -> Result<SrpParams, CryptoError> {
        let n = BigUint::parse_bytes(self.n_hex.as_bytes(), 16)
            .ok_or_else(|| CryptoError::SrpError("Failed to parse N".to_string()))?;
        let g = BigUint::from(self.g);

        // k = H(N, pad(g))
        let k = {
            let mut hasher = Sha512::new();
            hasher.update(n.to_bytes_be());
            let g_bytes = g.to_bytes_be();
            let mut g_padded = vec![0u8; 384];
            g_padded[384 - g_bytes.len()..].copy_from_slice(&g_bytes);
            hasher.update(&g_padded);
            BigUint::from_bytes_be(&hasher.finalize())
        };

        Ok(SrpParams { n, g, k })
    }
}

/// Apple SRP-6a implementation matching HomeKit/AirPlay 2 requirements
pub struct SrpClient {
    params: SrpParams,
    a: BigUint,
    public_key: Vec<u8>,
}

impl Drop for SrpClient {
    fn drop(&mut self) {
        // BigUint doesn't implement Zeroize easily, but we can clear the internal Vec if we had access.
        // For now, we just let it be dropped.
    }
}

impl SrpClient {
    /// Create a new SRP client with default parameters
    pub fn new() -> Result<Self, CryptoError> {
        Self::with_params(&SrpParams::RFC5054_3072)
    }

    /// Create a new SRP client with specified parameters
    pub fn with_params(group: &SrpGroup) -> Result<Self, CryptoError> {
        let params = group.to_params()?;

        let mut rng = rand::thread_rng();
        let a: BigUint = rng.sample(RandomBits::new(256));
        let a = a % &params.n;

        // A = g^a % n
        let a_pub = params.g.modpow(&a, &params.n);
        let mut public_key = a_pub.to_bytes_be();
        // Pad to 384 bytes
        if public_key.len() < 384 {
            let mut padded = vec![0u8; 384];
            padded[384 - public_key.len()..].copy_from_slice(&public_key);
            public_key = padded;
        }

        Ok(Self {
            params,
            a,
            public_key,
        })
    }

    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    pub fn process_challenge(
        &self,
        username: &[u8],
        password: &[u8],
        salt: &[u8],
        server_public: &[u8],
    ) -> Result<SrpVerifier, CryptoError> {
        let b_pub = BigUint::from_bytes_be(server_public);
        if &b_pub % &self.params.n == BigUint::from(0u32) {
            return Err(CryptoError::SrpError(
                "Invalid server public key".to_string(),
            ));
        }

        let a_pub = BigUint::from_bytes_be(&self.public_key);

        // u = H(pad(A), pad(B))
        let u = {
            let mut hasher = Sha512::new();
            hasher.update(&self.public_key);
            let mut b_padded = vec![0u8; 384];
            let b_bytes = b_pub.to_bytes_be();
            b_padded[384 - b_bytes.len()..].copy_from_slice(&b_bytes);
            hasher.update(&b_padded);
            BigUint::from_bytes_be(&hasher.finalize())
        };

        // x = H(salt, H(username, ":", password))
        let x = {
            let mut inner = Sha512::new();
            inner.update(username);
            inner.update(b":");
            inner.update(password);
            let h_up = inner.finalize();

            let mut outer = Sha512::new();
            outer.update(salt);
            outer.update(h_up);
            BigUint::from_bytes_be(&outer.finalize())
        };

        // S = (B - k * g^x) ^ (a + u * x) % n
        // Note: BigUint doesn't support negative results, so we do (B + n * k - k * g^x)
        let g_x = self.params.g.modpow(&x, &self.params.n);
        let k_g_x = (&self.params.k * g_x) % &self.params.n;

        // Ensure result is positive within modulo n
        let base = if b_pub >= k_g_x {
            (&b_pub - &k_g_x) % &self.params.n
        } else {
            (&self.params.n - (&k_g_x - &b_pub) % &self.params.n) % &self.params.n
        };

        let exp = &self.a + (&u * x);
        let s_shared = base.modpow(&exp, &self.params.n);

        // K = H(S)
        let k_session = {
            let mut hasher = Sha512::new();
            hasher.update(s_shared.to_bytes_be());
            hasher.finalize().to_vec()
        };

        // M1 = H(H(N) ^ H(g), H(username), salt, A, B, K)
        let m1 = {
            let hn = Sha512::digest(self.params.n.to_bytes_be());
            let hg = Sha512::digest(self.params.g.to_bytes_be());
            let mut hn_xor_hg = [0u8; 64];
            for i in 0..64 {
                hn_xor_hg[i] = hn[i] ^ hg[i];
            }

            let h_user = Sha512::digest(username);

            let mut hasher = Sha512::new();
            hasher.update(&hn_xor_hg);
            hasher.update(&h_user);
            hasher.update(salt);
            // Use minimal-bytes representation of A (not padded) to match Python's to_bytes()
            hasher.update(a_pub.to_bytes_be());
            // Use minimal-bytes representation of B (not padded)
            hasher.update(b_pub.to_bytes_be());
            hasher.update(&k_session);
            hasher.finalize().to_vec()
        };

        Ok(SrpVerifier {
            // n: self.params.n.clone(), // SrpVerifier doesn't seem to need params if it just verifies M2 against K
            // Wait, SrpVerifier needs a_pub and m1 to compute M2.
            a_pub,
            m1,
            k_session,
        })
    }
}

pub struct SrpVerifier {
    a_pub: BigUint,
    m1: Vec<u8>,
    k_session: Vec<u8>,
}

impl SrpVerifier {
    pub fn client_proof(&self) -> &[u8] {
        &self.m1
    }

    pub fn verify_server(&self, server_proof: &[u8]) -> Result<SessionKey, CryptoError> {
        // M2 = H(A, M1, K)
        let mut hasher = Sha512::new();
        hasher.update(self.a_pub.to_bytes_be());
        hasher.update(&self.m1);
        hasher.update(&self.k_session);
        let expected_m2 = hasher.finalize();

        if expected_m2.as_slice() != server_proof {
            return Err(CryptoError::SrpError(
                "Server proof verification failed".to_string(),
            ));
        }

        Ok(SessionKey {
            key: self.k_session.clone(),
        })
    }
}

/// SRP Server Implementation
pub struct SrpServer {
    params: SrpParams,
    b: BigUint,
    v: BigUint,
    public_key: Vec<u8>,
    username: Option<Vec<u8>>, // To verify username if needed, but not strictly required by verify_client
    salt: Option<Vec<u8>>,
}

impl SrpServer {
    /// Compute verifier from password
    pub fn compute_verifier(
        username: &[u8],
        password: &[u8],
        salt: &[u8],
        group: &SrpGroup,
    ) -> Result<Vec<u8>, CryptoError> {
        // This is costly to re-parse params every time, but necessary if SrpParams isn't cached
        // For optimization, one should reuse parsed params.
        // We'll parse here.
        let params = group.to_params()?;

        // x = H(salt, H(username, ":", password))
        let x = {
            let mut inner = Sha512::new();
            inner.update(username);
            inner.update(b":");
            inner.update(password);
            let h_up = inner.finalize();

            let mut outer = Sha512::new();
            outer.update(salt);
            outer.update(h_up);
            BigUint::from_bytes_be(&outer.finalize())
        };

        // v = g^x % n
        let v = params.g.modpow(&x, &params.n);

        // Pad to 384 bytes
        let mut verifier = v.to_bytes_be();
        if verifier.len() < 384 {
            let mut padded = vec![0u8; 384];
            padded[384 - verifier.len()..].copy_from_slice(&verifier);
            verifier = padded;
        }
        Ok(verifier)
    }

    /// Create new SRP server instance
    pub fn new(verifier: &[u8], group: &SrpGroup) -> Result<Self, CryptoError> {
        let params = group.to_params()?;
        let v = BigUint::from_bytes_be(verifier);

        // Generate random b
        let mut rng = rand::thread_rng();
        let b: BigUint = rng.sample(RandomBits::new(256));
        let b = b % &params.n;

        // B = (k*v + g^b) % n
        let g_b = params.g.modpow(&b, &params.n);
        let k_v = (&params.k * &v) % &params.n;
        let b_pub = (k_v + g_b) % &params.n;

        let mut public_key = b_pub.to_bytes_be();
        // Pad to 384 bytes
        if public_key.len() < 384 {
            let mut padded = vec![0u8; 384];
            padded[384 - public_key.len()..].copy_from_slice(&public_key);
            public_key = padded;
        }

        Ok(Self {
            params,
            b,
            v,
            public_key,
            username: None, // Can be set if we want to verify M1 with username
            salt: None,
        })
    }

    /// Set context (username, salt) for M1 verification
    ///
    /// This is needed because M1 calculation depends on username and salt.
    /// In AirPlay 2 pairing, the server knows the salt it sent, and username is "Pair-Setup".
    pub fn set_context(&mut self, username: &[u8], salt: &[u8]) {
        self.username = Some(username.to_vec());
        self.salt = Some(salt.to_vec());
    }

    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// Verify client proof (M1) and generate server proof (M2)
    ///
    /// Returns (SessionKey, ServerProof)
    pub fn verify_client(
        &self,
        client_public: &[u8],
        client_proof: &[u8],
    ) -> Result<(SessionKey, Vec<u8>), CryptoError> {
        let a_pub = BigUint::from_bytes_be(client_public);
        if &a_pub % &self.params.n == BigUint::from(0u32) {
            return Err(CryptoError::SrpError(
                "Invalid client public key".to_string(),
            ));
        }

        // u = H(pad(A), pad(B))
        let u = {
            let mut hasher = Sha512::new();
            // Pad A
            let mut a_padded = vec![0u8; 384];
            let a_bytes = a_pub.to_bytes_be();
            a_padded[384 - a_bytes.len()..].copy_from_slice(&a_bytes);
            hasher.update(&a_padded);

            // Pad B (self.public_key is already padded)
            hasher.update(&self.public_key);
            BigUint::from_bytes_be(&hasher.finalize())
        };

        // S = (A * v^u) ^ b % n
        let v_u = self.v.modpow(&u, &self.params.n);
        let base = (&a_pub * v_u) % &self.params.n;
        let s_shared = base.modpow(&self.b, &self.params.n);

        // K = H(S)
        let k_session = {
            let mut hasher = Sha512::new();
            hasher.update(s_shared.to_bytes_be());
            hasher.finalize().to_vec()
        };

        // Verify M1
        // We need username and salt. Default to "Pair-Setup" if not set?
        // AirPlay 2 uses "Pair-Setup".
        let username = self.username.as_deref().unwrap_or(b"Pair-Setup");
        // We really need the salt used.
        // If not set, we can't verify properly unless we assume caller ensures it matches.
        // But logic requires salt.
        // We'll assume salt is provided or we can't verify?
        // Actually, `verify_client` shouldn't fail if salt isn't set IF we don't verify M1?
        // No, we MUST verify M1.
        // The calling code creates SrpServer. It should have the salt.
        // Let's assume the caller sets context or we error.
        // Or we can modify `new` or `verify_client` signature.
        // The prompt code snippet for `PairingServer` has `srp_server = SrpServer::new(&verifier, &SRP_PARAMS)`.
        // And `srp_server.verify_client(client_public, client_proof)`.
        // It DOES NOT call `set_context`.
        // So `SrpServer` must either store salt from creation (it doesn't in snippet) or assume defaults?
        // But salt is random per session.
        // Wait, `PairingServer` generates `srp_salt`.
        // It calls `SrpServer::new` with verifier.
        // It DOES NOT pass salt to `new`.
        // How can `SrpServer` verify M1 without salt?
        // M1 = H(H(N) ^ H(g), H(username), salt, A, B, K)
        // It NEEDS salt.

        // I should add salt to `SrpServer::new`.
        // The snippet `PairingServer::handle_pair_setup_m1` says:
        // `let srp_server = SrpServer::new(&verifier, &SRP_PARAMS);`
        // It does NOT pass salt.
        // This suggests the snippet might be incomplete or `SrpServer` in the snippet's mind works differently.
        // But SRP requires salt for M1 verification.

        // I will update `SrpServer::new` to take `salt` and `username`.
        // `pub fn new(username: &[u8], password_verifier: &[u8], salt: &[u8], group: &SrpGroup) -> Self`
        // Or just modify `PairingServer` logic to pass salt.

        // I will update `SrpServer` to take `salt` and `username` in `new`.
        // I'll assume "Pair-Setup" as default username if not provided?
        // No, explicit is better.

        // Re-reading snippet:
        // `let srp_server = SrpServer::new(&verifier, &SRP_PARAMS);`
        // `let response = ... .add_bytes(TlvType::Salt, &self.srp_salt) ...`

        // If I change `SrpServer::new` signature, I must update the `PairingServer` code I write later.
        // That is fine. I am writing `PairingServer` too.

        // So I'll make `new` take salt and username.

        let salt = self
            .salt
            .as_deref()
            .ok_or_else(|| CryptoError::SrpError("Salt not set".to_string()))?;

        // M1 calc
        let expected_m1 = {
            let hn = Sha512::digest(self.params.n.to_bytes_be());
            let hg = Sha512::digest(self.params.g.to_bytes_be());
            let mut hn_xor_hg = [0u8; 64];
            for i in 0..64 {
                hn_xor_hg[i] = hn[i] ^ hg[i];
            }

            let h_user = Sha512::digest(username);

            let mut hasher = Sha512::new();
            hasher.update(&hn_xor_hg);
            hasher.update(&h_user);
            hasher.update(salt);
            hasher.update(a_pub.to_bytes_be()); // Minimal bytes
            hasher.update(BigUint::from_bytes_be(&self.public_key).to_bytes_be()); // Minimal bytes of B?
            // Note: `self.public_key` is padded. `BigUint::from_bytes_be(...).to_bytes_be()` removes padding.
            // Python `srp` uses minimal bytes for hash updates.
            hasher.update(&k_session);
            hasher.finalize().to_vec()
        };

        if expected_m1 != client_proof {
            return Err(CryptoError::SrpError(
                "Client proof verification failed".to_string(),
            ));
        }

        // M2 = H(A, M1, K)
        let server_proof = {
            let mut hasher = Sha512::new();
            // Use minimal bytes for A (as expected by client logic in SrpVerifier)
            hasher.update(a_pub.to_bytes_be());
            hasher.update(client_proof);
            hasher.update(&k_session);
            hasher.finalize().to_vec()
        };

        Ok((SessionKey { key: k_session }, server_proof))
    }
}

pub struct SessionKey {
    key: Vec<u8>,
}

impl SessionKey {
    pub fn as_bytes(&self) -> &[u8] {
        &self.key
    }
}

impl Drop for SessionKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}
