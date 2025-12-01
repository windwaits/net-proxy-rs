use std::net::SocketAddr;

use crate::core::types::ConnectionCtx;

/// Helper builders used by platform listeners to construct `ConnectionCtx` values.
pub fn from_metadata(
    source: SocketAddr,
    destination: SocketAddr,
    domain: Option<String>,
    process_path: Option<String>,
) -> ConnectionCtx {
    ConnectionCtx {
        source,
        destination,
        domain,
        process_path,
    }
}
