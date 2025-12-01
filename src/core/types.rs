use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyKind {
    Direct,
    TcpForward(SocketAddr),
    Socks5 {
        addr: SocketAddr,
        username: Option<String>,
        password: Option<String>,
    },
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteAction {
    Direct,
    Block,
    Upstream(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionCtx {
    pub source: SocketAddr,
    pub destination: SocketAddr,
    pub domain: Option<String>,
    pub process_path: Option<String>,
}

impl ConnectionCtx {
    pub fn new(source: SocketAddr, destination: SocketAddr) -> Self {
        Self {
            source,
            destination,
            domain: None,
            process_path: None,
        }
    }
}
