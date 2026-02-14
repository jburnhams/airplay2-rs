use super::CryptoError;
use num_bigint::{BigUint, RandomBits};
use num_traits::One;
use rand::Rng;
use sha2::{Digest, Sha512};
use zeroize::Zeroize;

/// SRP-6a Parameters
#[derive(Clone, Debug)]
pub struct SrpParams {
    /// Prime modulus (N)
    pub n: BigUint,
    /// Generator (g)
    pub g: BigUint,
    /// Size of N in bytes
    pub size: usize,
}

impl SrpParams {
    /// RFC 5054 3072-bit group parameters
    pub const RFC5054_3072: LazyParams = LazyParams {
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
        size: 384,
    };
}

#[derive(Clone, Copy)]
pub struct LazyParams {
    pub n_hex: &'static str,
    pub g: u32,
    pub size: usize,
}

impl From<LazyParams> for SrpParams {
    fn from(lazy: LazyParams) -> Self {
        Self {
            n: BigUint::parse_bytes(lazy.n_hex.as_bytes(), 16).expect("Valid N hex"),
            g: BigUint::from(lazy.g),
            size: lazy.size,
        }
    }
}

/// Apple SRP-6a implementation matching HomeKit/AirPlay 2 requirements
pub struct SrpClient {
    params: SrpParams,
    k: BigUint,
    a: BigUint,
    public_key: Vec<u8>,
}

impl Drop for SrpClient {
    fn drop(&mut self) {
        // BigUint doesn't implement Zeroize easily.
    }
}

impl SrpClient {
    pub fn new(params_lazy: &LazyParams) -> Result<Self, CryptoError> {
        let params: SrpParams = (*params_lazy).into();
        Self::with_params(params)
    }

    pub fn with_params(params: SrpParams) -> Result<Self, CryptoError> {
        // k = H(N, pad(g))
        let k = compute_k(&params);

        let mut rng = rand::thread_rng();
        let a: BigUint = rng.sample(RandomBits::new(256));
        let a = a % &params.n;

        // A = g^a % n
        let a_pub = params.g.modpow(&a, &params.n);
        let public_key = pad_to_size(&a_pub.to_bytes_be(), params.size);

        Ok(Self {
            params,
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
        if &b_pub % &self.params.n == BigUint::from(0u32) {
            return Err(CryptoError::SrpError(
                "Invalid server public key".to_string(),
            ));
        }

        let a_pub = BigUint::from_bytes_be(&self.public_key);

        // u = H(pad(A), pad(B))
        let u = compute_u(&self.public_key, server_public, self.params.size);

        // x = H(salt, H(username, ":", password))
        let x = compute_x(salt, username, password);

        // S = (B - k * g^x) ^ (a + u * x) % n
        let g_x = self.params.g.modpow(&x, &self.params.n);
        let k_g_x = (&self.k * g_x) % &self.params.n;
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
        let m1 = compute_m1(&self.params, username, salt, &a_pub, &b_pub, &k_session);

        Ok(SrpVerifier {
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

/// SRP Server Implementation
pub struct SrpServer {
    params: SrpParams,
    k: BigUint,
    v: BigUint,
    b: BigUint,
    public_key: Vec<u8>,
}

impl SrpServer {
    pub fn new(verifier: &[u8], params_lazy: &LazyParams) -> Self {
        let params: SrpParams = (*params_lazy).into();
        let k = compute_k(&params);
        let v = BigUint::from_bytes_be(verifier);

        let mut rng = rand::thread_rng();
        let b: BigUint = rng.sample(RandomBits::new(256));
        let b = b % &params.n;

        // B = k*v + g^b % N
        let g_b = params.g.modpow(&b, &params.n);
        let k_v = (&k * &v) % &params.n;
        let b_pub = (k_v + g_b) % &params.n;

        let public_key = pad_to_size(&b_pub.to_bytes_be(), params.size);

        Self {
            params,
            k,
            v,
            b,
            public_key,
        }
    }

    pub fn compute_verifier(
        username: &[u8],
        password: &[u8],
        salt: &[u8],
        params_lazy: &LazyParams,
    ) -> Vec<u8> {
        let params: SrpParams = (*params_lazy).into();
        let x = compute_x(salt, username, password);
        let v = params.g.modpow(&x, &params.n);
        v.to_bytes_be()
    }

    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    pub fn verify_client(
        &self,
        username: &[u8],
        salt: &[u8],
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
        let u = compute_u(client_public, &self.public_key, self.params.size);

        // S = (A * v^u) ^ b % N
        let v_u = self.v.modpow(&u, &self.params.n);
        let base = (&a_pub * v_u) % &self.params.n;
        let s_shared = base.modpow(&self.b, &self.params.n);

        // K = H(S)
        let k_session = {
            let mut hasher = Sha512::new();
            hasher.update(s_shared.to_bytes_be());
            hasher.finalize().to_vec()
        };

        let b_pub = BigUint::from_bytes_be(&self.public_key);

        // M1 = H(H(N) ^ H(g), H(username), salt, A, B, K)
        let expected_m1 = compute_m1(&self.params, username, salt, &a_pub, &b_pub, &k_session);

        if expected_m1 != client_proof {
            return Err(CryptoError::SrpError(
                "Client proof verification failed".to_string(),
            ));
        }

        // M2 = H(A, M1, K)
        let mut hasher = Sha512::new();
        hasher.update(a_pub.to_bytes_be());
        hasher.update(&expected_m1);
        hasher.update(&k_session);
        let m2 = hasher.finalize().to_vec();

        Ok((SessionKey { key: k_session }, m2))
    }
}

// Helpers

fn pad_to_size(bytes: &[u8], size: usize) -> Vec<u8> {
    if bytes.len() >= size {
        bytes.to_vec()
    } else {
        let mut padded = vec![0u8; size];
        padded[size - bytes.len()..].copy_from_slice(bytes);
        padded
    }
}

fn compute_k(params: &SrpParams) -> BigUint {
    let mut hasher = Sha512::new();
    hasher.update(params.n.to_bytes_be());
    let g_bytes = params.g.to_bytes_be();
    let g_padded = pad_to_size(&g_bytes, params.size);
    hasher.update(&g_padded);
    BigUint::from_bytes_be(&hasher.finalize())
}

fn compute_x(salt: &[u8], username: &[u8], password: &[u8]) -> BigUint {
    let mut inner = Sha512::new();
    inner.update(username);
    inner.update(b":");
    inner.update(password);
    let h_up = inner.finalize();

    let mut outer = Sha512::new();
    outer.update(salt);
    outer.update(strip_leading_zeros(&h_up));
    BigUint::from_bytes_be(&outer.finalize())
}

fn compute_u(a_pub: &[u8], b_pub: &[u8], size: usize) -> BigUint {
    let mut hasher = Sha512::new();
    hasher.update(pad_to_size(a_pub, size));
    hasher.update(pad_to_size(b_pub, size));
    BigUint::from_bytes_be(&hasher.finalize())
}

fn compute_m1(
    params: &SrpParams,
    username: &[u8],
    salt: &[u8],
    a_pub: &BigUint,
    b_pub: &BigUint,
    k_session: &[u8],
) -> Vec<u8> {
    let hn = Sha512::digest(params.n.to_bytes_be());
    let hg = Sha512::digest(params.g.to_bytes_be());
    let mut hn_xor_hg = [0u8; 64];
    for i in 0..64 {
        hn_xor_hg[i] = hn[i] ^ hg[i];
    }

    let h_user = Sha512::digest(username);

    let mut hasher = Sha512::new();
    hasher.update(strip_leading_zeros(&hn_xor_hg));
    hasher.update(strip_leading_zeros(&h_user));
    hasher.update(salt);
    // Use minimal-bytes representation of A (not padded) to match standard
    hasher.update(a_pub.to_bytes_be());
    hasher.update(b_pub.to_bytes_be());
    hasher.update(strip_leading_zeros(k_session));
    hasher.finalize().to_vec()
}

fn strip_leading_zeros(bytes: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < bytes.len() && bytes[i] == 0 {
        i += 1;
    }
    // If all zeros, return empty slice? Or one zero byte?
    // srp.py uses to_bytes which returns minimal bytes.
    // to_bytes(0) is empty bytes.
    if i == bytes.len() {
        &[]
    } else {
        &bytes[i..]
    }
}
