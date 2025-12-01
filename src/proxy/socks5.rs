use std::net::SocketAddr;

use thiserror::Error;
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[derive(Debug, Error)]
pub enum SocksError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("server rejected authentication method")]
    AuthRejected,
    #[error("authentication failed")]
    AuthFailed,
    #[error("proxy connect rejected: code {0}")]
    Reply(u8),
    #[error("malformed response")]
    Malformed,
}

pub async fn connect_via(
    upstream: SocketAddr,
    target: SocketAddr,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<TcpStream, SocksError> {
    let mut stream = TcpStream::connect(upstream).await?;
    greet(&mut stream, username.is_some()).await?;
    if username.is_some() {
        auth(&mut stream, username.unwrap_or(""), password.unwrap_or("")).await?;
    }
    request_connect(&mut stream, target).await?;
    Ok(stream)
}

async fn greet(stream: &mut TcpStream, need_auth: bool) -> Result<(), SocksError> {
    let methods: Vec<u8> = if need_auth { vec![0, 2] } else { vec![0] };
    stream.write_all(&[5, methods.len() as u8]).await?;
    stream.write_all(&methods).await?;
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await?;
    if resp[0] != 5 {
        return Err(SocksError::Malformed);
    }
    match resp[1] {
        0 => Ok(()),
        2 if need_auth => Ok(()),
        _ => Err(SocksError::AuthRejected),
    }
}

async fn auth(stream: &mut TcpStream, username: &str, password: &str) -> Result<(), SocksError> {
    let uname = username.as_bytes();
    let passwd = password.as_bytes();
    if uname.len() > u8::MAX as usize || passwd.len() > u8::MAX as usize {
        return Err(SocksError::AuthFailed);
    }
    let mut buf = Vec::with_capacity(3 + uname.len() + passwd.len());
    buf.push(1);
    buf.push(uname.len() as u8);
    buf.extend_from_slice(uname);
    buf.push(passwd.len() as u8);
    buf.extend_from_slice(passwd);
    stream.write_all(&buf).await?;
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await?;
    if resp[1] == 0 {
        Ok(())
    } else {
        Err(SocksError::AuthFailed)
    }
}

async fn request_connect(stream: &mut TcpStream, target: SocketAddr) -> Result<(), SocksError> {
    let mut buf = Vec::with_capacity(22);
    buf.extend_from_slice(&[5, 1, 0]);
    match target {
        SocketAddr::V4(addr) => {
            buf.push(1);
            buf.extend_from_slice(&addr.ip().octets());
            buf.extend_from_slice(&addr.port().to_be_bytes());
        }
        SocketAddr::V6(addr) => {
            buf.push(4);
            buf.extend_from_slice(&addr.ip().octets());
            buf.extend_from_slice(&addr.port().to_be_bytes());
        }
    }
    stream.write_all(&buf).await?;

    let mut resp = [0u8; 4];
    stream.read_exact(&mut resp).await?;
    if resp[0] != 5 {
        return Err(SocksError::Malformed);
    }
    if resp[1] != 0 {
        return Err(SocksError::Reply(resp[1]));
    }
    let atyp = resp[3];
    match atyp {
        1 => {
            let mut skip = [0u8; 6];
            stream.read_exact(&mut skip).await?;
        }
        3 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut skip = vec![0u8; len[0] as usize + 2];
            stream.read_exact(&mut skip).await?;
        }
        4 => {
            let mut skip = [0u8; 18];
            stream.read_exact(&mut skip).await?;
        }
        _ => return Err(SocksError::Malformed),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::*;

    #[tokio::test]
    async fn performs_noauth_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 3];
            socket.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, &[5, 1, 0]);
            socket.write_all(&[5, 0]).await.unwrap();

            let mut req = [0u8; 4];
            socket.read_exact(&mut req).await.unwrap();
            assert_eq!(req[0], 5);
            assert_eq!(req[1], 1);
            assert_eq!(req[3], 1);
            let mut tail = [0u8; 6];
            socket.read_exact(&mut tail).await.unwrap();
            socket.write_all(&[5, 0, 0, 1]).await.unwrap();
            socket.write_all(&[0, 0, 0, 0, 0, 0]).await.unwrap();
        });

        let stream = connect_via(addr, "1.2.3.4:80".parse().unwrap(), None, None)
            .await
            .unwrap();
        assert!(stream.peer_addr().is_ok());
    }

    #[tokio::test]
    async fn performs_password_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4];
            socket.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, &[5, 2, 0, 2]);
            socket.write_all(&[5, 2]).await.unwrap();

            let mut auth_hdr = [0u8; 2];
            socket.read_exact(&mut auth_hdr).await.unwrap();
            let uname_len = auth_hdr[1] as usize;
            let mut uname = vec![0u8; uname_len];
            socket.read_exact(&mut uname).await.unwrap();
            let mut pass_len = [0u8; 1];
            socket.read_exact(&mut pass_len).await.unwrap();
            let mut pass = vec![0u8; pass_len[0] as usize];
            socket.read_exact(&mut pass).await.unwrap();
            assert_eq!(uname, b"user");
            assert_eq!(pass, b"pass");
            socket.write_all(&[1, 0]).await.unwrap();

            let mut req = [0u8; 4];
            socket.read_exact(&mut req).await.unwrap();
            assert_eq!(req[3], 1);
            let mut tail = [0u8; 6];
            socket.read_exact(&mut tail).await.unwrap();
            socket.write_all(&[5, 0, 0, 1]).await.unwrap();
            socket.write_all(&[0, 0, 0, 0, 0, 0]).await.unwrap();
        });

        let stream = connect_via(
            addr,
            "5.6.7.8:443".parse().unwrap(),
            Some("user"),
            Some("pass"),
        )
        .await
        .unwrap();
        assert!(stream.peer_addr().is_ok());
    }
}
