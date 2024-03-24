use aya::maps::{
    perf::{AsyncPerfEventArrayBuffer, Events, PerfBufferError},
    MapData,
};
use buplib::FutureMutReceiver;
use bytes::BytesMut;
use derive_new::new;
use ebeepyf_common::PacketInfo;
use std::borrow::BorrowMut;

#[derive(new)]
pub(super) struct PerfBufferReceiver<T>(AsyncPerfEventArrayBuffer<T>)
where
    T: BorrowMut<MapData>;

impl<T> FutureMutReceiver<Vec<PacketInfo>, PerfBufferError> for PerfBufferReceiver<T>
where
    T: BorrowMut<MapData>,
{
    fn accept(
        &mut self,
    ) -> impl std::future::Future<Output = std::prelude::v1::Result<Vec<PacketInfo>, PerfBufferError>>
    {
        let mut bufs = vec![BytesMut::zeroed(20); 10];
        async move {
            loop {
                bufs.fill(BytesMut::zeroed(20));
                let Events { read, lost: _ } = self.0.read_events(&mut bufs).await?;
                return Ok::<Vec<PacketInfo>, PerfBufferError>(
                    bufs.iter()
                        .take(read)
                        .map(|bytes| PacketInfo::try_from(bytes.as_ref()).unwrap())
                        .collect(),
                );
            }
        }
    }
}
