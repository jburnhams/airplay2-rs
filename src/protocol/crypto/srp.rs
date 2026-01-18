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
