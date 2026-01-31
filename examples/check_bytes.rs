use std::fs::File;
use std::io::Read;

fn main() -> std::io::Result<()> {
    let mut file = File::open("airplay2-receiver/received_audio_44100_2ch.raw")?;
    let mut buffer = [0u8; 400]; // 100 samples (4 bytes each)
    file.read_exact(&mut buffer)?;

    println!("First 100 samples:");
    for (i, chunk) in buffer.chunks(4).enumerate() {
        let l_be = i16::from_be_bytes([chunk[0], chunk[1]]);
        let r_be = i16::from_be_bytes([chunk[2], chunk[3]]);
        println!("{}: L={} R={}", i, l_be, r_be);
    }
    Ok(())
}
