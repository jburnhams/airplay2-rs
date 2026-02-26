use std::time::{SystemTime, UNIX_EPOCH, Duration};

fn main() {
    let now = SystemTime::now();
    let d = now.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    println!("{:?}", d);
}
