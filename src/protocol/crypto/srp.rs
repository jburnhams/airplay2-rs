use super::CryptoError;
use num_bigint::{BigUint, RandomBits};
use num_traits::One;
use rand::Rng;
use sha2::{Digest, Sha512};
use zeroize::Zeroize;

/// Apple SRP-6a implementation matching HomeKit/AirPlay 2 requirements
pub struct SrpClient {
    n: BigUint,
    g: BigUint,
    k: BigUint,
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
    pub fn new() -> Result<Self, CryptoError> {
        let n = BigUint::parse_bytes(
            b"FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E08\
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
            16,
        )
        .ok_or_else(|| CryptoError::SrpError("Failed to parse N".to_string()))?;

        let g = BigUint::from(5u32);

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

        let mut rng = rand::thread_rng();
        let a: BigUint = rng.sample(RandomBits::new(256));
        let a = a % &n;

        // A = g^a % n
        let a_pub = g.modpow(&a, &n);
        let mut public_key = a_pub.to_bytes_be();
        // Pad to 384 bytes
        if public_key.len() < 384 {
            let mut padded = vec![0u8; 384];
            padded[384 - public_key.len()..].copy_from_slice(&public_key);
            public_key = padded;
        }

        Ok(Self {
            n,
            g,
            k,
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
        if &b_pub % &self.n == BigUint::from(0u32) {
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
        let g_x = self.g.modpow(&x, &self.n);
        let k_g_x = (&self.k * g_x) % &self.n;
        let base = if b_pub >= k_g_x {
            (&b_pub - &k_g_x) % &self.n
        } else {
            (&self.n - (&k_g_x - &b_pub) % &self.n) % &self.n
        };

        let exp = &self.a + (&u * x);
        let s_shared = base.modpow(&exp, &self.n);

        // K = H(S)
        let k_session = {
            let mut hasher = Sha512::new();
            hasher.update(s_shared.to_bytes_be());
            hasher.finalize().to_vec()
        };

        // M1 = H(H(N) ^ H(g), H(username), salt, A, B, K)
        let m1 = {
            let hn = Sha512::digest(self.n.to_bytes_be());
            let hg = Sha512::digest(self.g.to_bytes_be());
            let mut hn_xor_hg = [0u8; 64];
            for i in 0..64 {
                hn_xor_hg[i] = hn[i] ^ hg[i];
            }

            let h_user = Sha512::digest(username);

            let mut hasher = Sha512::new();
            hasher.update(&hn_xor_hg);
            hasher.update(&h_user);
            hasher.update(salt);
            hasher.update(&self.public_key);
            // Pad B for M1? Python's srp.py doesn't pad here.
            // But let's see. If I don't pad, it matches to_bytes(B, False).
            hasher.update(b_pub.to_bytes_be());
            hasher.update(&k_session);
            hasher.finalize().to_vec()
        };

        Ok(SrpVerifier {
            n: self.n.clone(),
            a_pub,
            m1,
            k_session,
        })
    }
}

pub struct SrpVerifier {
    n: BigUint,
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
