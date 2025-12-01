use async_trait::async_trait;
use futures::Stream;

use crate::core::types::ConnectionCtx;

#[async_trait]
pub trait PacketSource {
    type stream<'a>: Stream<Item = ConnectionCtx> + Send + Unpin
    where
        Self: 'a;
    async fn events(&self) -> Self::stream<'_>;
}

#[cfg(target_os = "windows")]
pub mod windows;

pub mod mock {
    use std::pin::Pin;

    use async_trait::async_trait;
    use futures::{stream, Stream};

    use crate::core::types::ConnectionCtx;

    use super::PacketSource;

    pub struct MockPacketSource {
        events: Vec<ConnectionCtx>,
    }

    impl MockPacketSource {
        pub fn new(events: Vec<ConnectionCtx>) -> Self {
            Self { events }
        }
    }

    #[async_trait]
    impl PacketSource for MockPacketSource {
        type stream<'a>
            = Pin<Box<dyn Stream<Item = ConnectionCtx> + Send + 'a>>
        where
            Self: 'a;

        async fn events(&self) -> Self::stream<'_> {
            Box::pin(stream::iter(self.events.clone()))
        }
    }
}
