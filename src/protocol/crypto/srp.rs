use super::CryptoError;
use sha2_011::Sha512;
use zeroize::Zeroize;

pub struct SrpClient {
    inner: srp::Client<srp::groups::G3072, Sha512>,
    private_key: Vec<u8>,
    public_key: Vec<u8>,
}

impl Drop for SrpClient {
    fn drop(&mut self) {
        self.private_key.zeroize();
    }
}

impl SrpClient {
    pub fn new() -> Result<Self, CryptoError> {
        use rand::RngCore;
        let mut private_key = vec![0u8; 32];
        rand::thread_rng()
            .try_fill_bytes(&mut private_key)
            .map_err(|_| CryptoError::RngError)?;

        Ok(Self::with_private_key(&private_key))
    }

    pub fn with_private_key(private_key: &[u8]) -> SrpClient {
        let inner = srp::Client::new();
        let public_key = inner.compute_public_ephemeral(private_key);
        Self {
            inner,
            private_key: private_key.to_vec(),
            public_key: public_key.clone(),
        }
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
        let verifier = self
            .inner
            .process_reply(&self.private_key, username, password, salt, server_public)
            .map_err(|e| CryptoError::SrpError(format!("{e:?}")))?;

        Ok(SrpVerifier { inner: verifier })
    }
}

pub struct SrpVerifier {
    inner: srp::ClientVerifier<Sha512>,
}

impl SrpVerifier {
    pub fn client_proof(&self) -> &[u8] {
        self.inner.proof()
    }

    pub fn verify_server(&self, server_proof: &[u8]) -> Result<SessionKey, CryptoError> {
        let key = self
            .inner
            .verify_server(server_proof)
            .map_err(|e| CryptoError::SrpError(format!("{e:?}")))?;

        Ok(SessionKey { key: key.to_vec() })
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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    #[test]
    fn test_client_creation() {
        let client = SrpClient::new().unwrap();
        assert!(!client.public_key().is_empty());
    }

    #[test]
    fn test_srp_handshake() {
        // 1. Client setup
        let client = SrpClient::new().unwrap();
        let username = b"Pair-Setup";
        let password = b"1234";
        let client_a = client.public_key();

        // 2. Server setup (simulation)
        let salt = b"randomsalt";

        // Use Client to compute verifier (simulating registration)
        let helper_client = srp::Client::<srp::groups::G3072, Sha512>::new();
        let verifier = helper_client.compute_verifier(username, password, salt);

        let server = srp::Server::<srp::groups::G3072, Sha512>::new();

        // Server generates ephemeral B
        let mut b_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut b_bytes);
        let server_b_pub = server.compute_public_ephemeral(&b_bytes, &verifier);

        // 3. Client processes challenge
        let client_verifier = client
            .process_challenge(username, password, salt, &server_b_pub)
            .expect("Client failed to process challenge");

        // 4. Client generates proof
        let client_m1 = client_verifier.client_proof();

        // 5. Server verifies client
        let server_verifier = server
            .process_reply(username, salt, &b_bytes, &verifier, client_a)
            .expect("Server failed to process reply");

        let server_key = server_verifier
            .verify_client(client_m1)
            .expect("Server failed to verify client");

        let server_m2 = server_verifier.proof();

        // 6. Client verifies server
        let client_key = client_verifier
            .verify_server(server_m2)
            .expect("Client failed to verify server");

        assert_eq!(client_key.as_bytes(), server_key);
    }

    #[test]
    fn test_invalid_password_fails() {
        let client = SrpClient::new().unwrap();
        let username = b"Pair-Setup";
        let password = b"correct";
        let salt = b"salt";

        // Helper for registration
        let helper_client = srp::Client::<srp::groups::G3072, Sha512>::new();
        // Server registered with "wrong" password
        let verifier = helper_client.compute_verifier(username, b"wrong", salt);

        let server = srp::Server::<srp::groups::G3072, Sha512>::new();
        let mut b_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut b_bytes);
        let server_b_pub = server.compute_public_ephemeral(&b_bytes, &verifier);

        // Client tries with "correct" password
        let client_verifier = client
            .process_challenge(username, password, salt, &server_b_pub)
            .unwrap();

        let client_m1 = client_verifier.client_proof();

        let server_verifier = server
            .process_reply(username, salt, &b_bytes, &verifier, client.public_key())
            .unwrap();

        // Verification should fail
        assert!(server_verifier.verify_client(client_m1).is_err());
    }
}
