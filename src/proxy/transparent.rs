use std::net::SocketAddr;

use tokio::{io, io::AsyncWriteExt, net::TcpStream};

pub async fn pipe(inbound: &mut TcpStream, mut outbound: TcpStream) -> io::Result<()> {
    tokio::io::copy_bidirectional(inbound, &mut outbound).await?;
    inbound.shutdown().await.ok();
    outbound.shutdown().await.ok();
    Ok(())
}

/// Extremely small framing for TCP forwarders: prepend destination bytes so a cooperating
/// relay can connect to the ultimate destination.
pub async fn forward_with_header(
    inbound: &mut TcpStream,
    mut outbound: TcpStream,
    destination: SocketAddr,
) -> io::Result<()> {
    let header = destination.to_string();
    outbound.write_all(header.as_bytes()).await?;
    outbound.write_all(b"\n").await?;
    pipe(inbound, outbound).await
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::{
        io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    use super::*;

    #[tokio::test]
    async fn writes_forward_header() {
        let outbound_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let outbound_addr = outbound_listener.local_addr().unwrap();
        let inbound_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let inbound_addr = inbound_listener.local_addr().unwrap();
        let dest: SocketAddr = "192.168.1.1:8080".parse().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = outbound_listener.accept().await.unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            assert_eq!(line.trim(), dest.to_string());
            let mut payload = String::new();
            reader.read_to_string(&mut payload).await.unwrap();
            assert_eq!(payload, "hi");
        });

        tokio::spawn(async move {
            let (mut inbound, _) = inbound_listener.accept().await.unwrap();
            let outbound = TcpStream::connect(outbound_addr).await.unwrap();
            forward_with_header(&mut inbound, outbound, dest)
                .await
                .unwrap();
        });

        let mut client = TcpStream::connect(inbound_addr).await.unwrap();
        client.write_all(b"hi").await.unwrap();
    }

    #[tokio::test]
    async fn pipes_bidirectionally() {
        let outbound_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let outbound_addr = outbound_listener.local_addr().unwrap();
        let inbound_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let inbound_addr = inbound_listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = outbound_listener.accept().await.unwrap();
            let mut buf = [0u8; 4];
            socket.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            socket.write_all(b"pong").await.unwrap();
        });

        tokio::spawn(async move {
            let (mut inbound, _) = inbound_listener.accept().await.unwrap();
            let outbound = TcpStream::connect(outbound_addr).await.unwrap();
            forward_with_header(&mut inbound, outbound, outbound_addr)
                .await
                .unwrap();
        });

        let mut client = TcpStream::connect(inbound_addr).await.unwrap();
        client.write_all(b"ping").await.unwrap();
        let mut resp = [0u8; 4];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(&resp, b"pong");
    }
}
