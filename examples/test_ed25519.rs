use ed25519_dalek::{Keypair, Signer};
use rand::rngs::OsRng;

fn main() {
    let mut csprng = OsRng;
    let keypair = Keypair::generate(&mut csprng);

    let message = b"Hello World";
    let signature = keypair.sign(message);

    println!("Public Key: {}", hex::encode(keypair.public.as_bytes()));
    println!("Message: {}", hex::encode(message));
    println!("Signature: {}", hex::encode(signature.to_bytes()));
}
