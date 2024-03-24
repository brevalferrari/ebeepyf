#![deny(
    clippy::all,
    trivial_numeric_casts,
    single_use_lifetimes,
    unused_crate_dependencies
)]
#![feature(async_closure)]
use std::{borrow::BorrowMut, f32::consts::PI, time::Duration};

use anyhow::{Context, Error, Result};
use aya::{
    include_bytes_aligned,
    maps::{
        perf::{AsyncPerfEventArrayBuffer, Events, PerfBufferError},
        AsyncPerfEventArray, MapData,
    },
    programs::{Xdp, XdpFlags},
    util::online_cpus,
    Ebpf,
};
use buplib::{Bup, FutureMutBup, FutureMutReceiver, MutBup};
use bytes::BytesMut;
use clap::Parser;
use ebeepyf_common::PacketInfo;
use rodio::{OutputStream, Source};
use tokio::{signal, spawn};

// Check the eBPF program! This name is the name of its map variable.
const EVENTS_MAP_NAME: &str = "EBEEPYF";

#[derive(Debug, Parser)]
struct Opt {
    #[clap(short, long, default_value = "eth0")]
    iface: String,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    // This will include your eBPF object file as raw bytes at compile-time and load it at
    // runtime. This approach is recommended for most real-world use cases. If you would
    // like to specify the eBPF program at runtime rather than at compile-time, you can
    // reach for `Ebpf::load_file` instead.
    #[cfg(debug_assertions)]
    let mut bpf = Ebpf::load(include_bytes_aligned!(
        "../../target/bpfel-unknown-none/debug/ebeepyf"
    ))?;
    #[cfg(not(debug_assertions))]
    let mut bpf = Ebpf::load(include_bytes_aligned!(
        "../../target/bpfel-unknown-none/release/ebeepyf"
    ))?;

    let program: &mut Xdp = bpf
        .program_mut("ebeepyf")
        .context("can't get program name")?
        .try_into()?;
    program.load()?;
    program.attach(&opt.iface, XdpFlags::default())
        .context("failed to attach the XDP program with default flags - try changing XdpFlags::default() to XdpFlags::SKB_MODE")?;

    let (_stream, handle) = OutputStream::try_default().unwrap();

    let mut events: AsyncPerfEventArray<_> = bpf
        .take_map(EVENTS_MAP_NAME)
        .context("can't take map (this may result from using the wrong map name, check the variable name in your eBPF program)")?
        .try_into()
        .context("can't convert map to a AsyncPerfEventArray")?;

    for cpu_id in online_cpus()? {
        let buf = events
            .open(cpu_id, Some(256))
            .expect("can't open perf buffer");
        let handle = handle.clone();
        spawn(async move {
            FutureMutBup::new(PerfBufferReceiver(buf), &handle)
                .activate::<_, Error>(|_packets: Vec<PacketInfo>| {
                    SineWave::new(442f32, 100).take_duration(Duration::from_millis(100))
                })
                .await
                .unwrap();
        });
    }

    println!("Waiting for Ctrl-C...");
    signal::ctrl_c().await?;
    println!();
    print!("Bye bye");

    Ok(())
}

#[derive(Clone, Debug)]
pub struct SineWave {
    freq: f32,
    volume: f32,
    num_sample: usize,
}

// simple generator to make a sine wave with a frequency and a volume
impl SineWave {
    /// Make a new sinewave with a frequency and a volume.
    #[inline]
    pub fn new(freq: f32, volume: u8) -> SineWave {
        SineWave {
            freq,
            volume: volume as f32 / u8::MAX as f32,
            num_sample: 0,
        }
    }
}

// make it iterable over elements that implement the rodio Sample trait (required by the Source trait)
impl Iterator for SineWave {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.num_sample = self.num_sample.wrapping_add(1);
        let value = 2.0 * PI * self.freq * self.num_sample as f32 / 48000.0;
        Some(self.volume * value.sin())
    }
}

// implement Source to make it playable
impl Source for SineWave {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    #[inline]
    fn channels(&self) -> u16 {
        1
    }
    #[inline]
    fn sample_rate(&self) -> u32 {
        48000
    }
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

struct PerfBufferReceiver<T>(AsyncPerfEventArrayBuffer<T>)
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
                if read != 0 {
                    return Ok::<Vec<PacketInfo>, PerfBufferError>(
                        bufs.iter()
                            .take(read)
                            .map(|bytes| PacketInfo::try_from(bytes.as_ref()))
                            .flatten()
                            .collect(),
                    );
                }
            }
        }
    }
}
