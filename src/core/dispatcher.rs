use std::collections::HashMap;

use thiserror::Error;
use tokio::net::TcpStream;

use crate::proxy::{socks5, transparent};

use super::types::{ConnectionCtx, ProxyKind, RouteAction};

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("missing upstream: {0}")]
    UpstreamMissing(String),
}

pub async fn dispatch(
    mut inbound: TcpStream,
    ctx: &ConnectionCtx,
    action: RouteAction,
    upstreams: &HashMap<String, ProxyKind>,
) -> Result<(), ProxyError> {
    match action {
        RouteAction::Direct => {
            let outbound = TcpStream::connect(ctx.destination).await?;
            transparent::pipe(&mut inbound, outbound).await?;
        }
        RouteAction::Block => {
            // Drop the connection silently.
        }
        RouteAction::Upstream(name) => match upstreams.get(&name) {
            Some(ProxyKind::Direct) => {
                let outbound = TcpStream::connect(ctx.destination).await?;
                transparent::pipe(&mut inbound, outbound).await?;
            }
            Some(ProxyKind::Block) => {}
            Some(ProxyKind::TcpForward(addr)) => {
                let outbound = TcpStream::connect(addr).await?;
                transparent::forward_with_header(&mut inbound, outbound, ctx.destination).await?;
            }
            Some(ProxyKind::Socks5 {
                addr,
                username,
                password,
            }) => {
                let outbound = socks5::connect_via(
                    *addr,
                    ctx.destination,
                    username.as_deref(),
                    password.as_deref(),
                )
                .await?;
                transparent::pipe(&mut inbound, outbound).await?;
            }
            None => return Err(ProxyError::UpstreamMissing(name)),
        },
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
    async fn direct_dispatch_forwards_payload() {
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_addr = target_listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = target_listener.accept().await.unwrap();
            let mut buf = [0u8; 3];
            socket.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"hey");
        });

        let inbound_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let inbound_addr = inbound_listener.local_addr().unwrap();
        let ctx = ConnectionCtx {
            source: "127.0.0.1:5555".parse().unwrap(),
            destination: target_addr,
            domain: None,
            process_path: None,
        };
        tokio::spawn(async move {
            let (inbound, _) = inbound_listener.accept().await.unwrap();
            dispatch(inbound, &ctx, RouteAction::Direct, &HashMap::new())
                .await
                .unwrap();
        });

        let mut client = TcpStream::connect(inbound_addr).await.unwrap();
        client.write_all(b"hey").await.unwrap();
    }
}
