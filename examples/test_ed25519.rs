use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;

fn main() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);

    let message = b"Hello World";
    let signature = signing_key.sign(message);

    println!(
        "Public Key: {}",
        hex::encode(signing_key.verifying_key().as_bytes())
    );
    println!("Message: {}", hex::encode(message));
    println!("Signature: {}", hex::encode(signature.to_bytes()));
}
