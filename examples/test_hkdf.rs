use hkdf::Hkdf;
use sha2::Sha512;

fn main() {
    let salt = b"Pair-Setup-Controller-Sign-Salt";
    let ikm = [0u8; 32];
    let info = b"Pair-Setup-Controller-Sign-Info";

    let hkdf = Hkdf::<Sha512>::new(Some(salt), &ikm);
    let mut okm = [0u8; 32];
    hkdf.expand(info, &mut okm).unwrap();

    print!("OKM: ");
    for b in okm {
        print!("{:02x}", b);
    }
    println!();
}
