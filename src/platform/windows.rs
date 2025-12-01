#[cfg(target_os = "windows")]
mod ffi {
    // Placeholder declarations for the WFP callout interactions. In a real driver, these
    // would be implemented in a C companion crate and linked via `windows-sys` types.
    #[repr(C)]
    pub struct PacketMetadata {
        pub source_addr: [u8; 16],
        pub dest_addr: [u8; 16],
        pub source_port: u16,
        pub dest_port: u16,
        pub process_path_offset: u32,
    }

    #[allow(dead_code)]
    extern "system" {
        pub fn register_callouts() -> i32;
        pub fn read_packets(buffer: *mut PacketMetadata, len: u32) -> u32;
    }
}

#[cfg(target_os = "windows")]
pub struct WfpPacketSource;

#[cfg(target_os = "windows")]
use async_trait::async_trait;
#[cfg(target_os = "windows")]
use futures::{stream, Stream};
#[cfg(target_os = "windows")]
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    pin::Pin,
};

#[cfg(target_os = "windows")]
use crate::core::{context::from_metadata, types::ConnectionCtx};

#[cfg(target_os = "windows")]
use super::PacketSource;

#[cfg(target_os = "windows")]
#[async_trait]
impl PacketSource for WfpPacketSource {
    type stream<'a>
        = Pin<Box<dyn Stream<Item = ConnectionCtx> + Send + 'a>>
    where
        Self: 'a;

    async fn events(&self) -> Self::stream<'_> {
        // In a real implementation this would read from a device handle or ALPC channel.
        // We provide a tiny synthetic stream to keep Linux/Mac CI compiling.
        let sample = from_metadata(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10000),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443),
            Some("example.com".into()),
            Some("C:/Windows/System32/svchost.exe".into()),
        );
        Box::pin(stream::once(async move { sample }))
    }
}
