use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    println!("Bound to {:?}", sock.local_addr()?);

    // Try connecting to port 0
    match sock.connect("127.0.0.1:0").await {
        Ok(_) => println!("Connected to 127.0.0.1:0 successfully"),
        Err(e) => println!("Failed to connect to 127.0.0.1:0: {:?}", e),
    }
    Ok(())
}
