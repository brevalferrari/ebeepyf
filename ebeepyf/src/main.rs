#![deny(
    clippy::all,
    trivial_numeric_casts,
    single_use_lifetimes,
    unused_crate_dependencies
)]
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
use buplib::{FutureMutBup, FutureMutReceiver};
use bytes::BytesMut;
use clap::Parser;
use ebeepyf_common::PacketInfo;
use rodio::{
    dynamic_mixer::mixer,
    source::{Empty, Mix, SineWave},
    OutputStream, Source,
};
use tokio::{signal, spawn};

// Check the eBPF program! This name is the name of its map variable.
const EVENTS_MAP_NAME: &str = "EBEEPYF";
const HEARING_RANGE: (f32, f32) = (31f32, 19000f32);

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
                .activate::<_, Error>(|packets: Vec<PacketInfo>| {
                    packets
                        .iter()
                        .fold(mixer(1, 48000), |(mixer, mix), p| {
                            mixer.add(SineWave::new(*p.src.ip.last().unwrap() as f32));
                            (mixer, mix)
                        })
                        .1
                        .take_duration(Duration::from_millis(100))
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
                            .map(|bytes| PacketInfo::try_from(bytes.as_ref()).unwrap())
                            .collect(),
                    );
                }
            }
        }
    }
}
