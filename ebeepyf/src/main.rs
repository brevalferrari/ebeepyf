#![deny(
    clippy::all,
    trivial_numeric_casts,
    single_use_lifetimes,
    unused_crate_dependencies
)]
use anyhow::{Context, Error, Result};
use aya::{
    include_bytes_aligned,
    maps::AsyncPerfEventArray,
    programs::{Xdp, XdpFlags},
    util::online_cpus,
    Ebpf,
};
use buplib::FutureMutBup;
use clap::Parser;
use ebeepyf_common::PacketInfo;
use rodio::{dynamic_mixer::mixer, OutputStream, Source};
use std::time::Duration;
use tokio::{signal, spawn};
mod bupwants;
use bupwants::PerfBufferReceiver;

use crate::sources::per_ip_sine;
mod sources;

// Check the eBPF program! This name is the name of its map variable.
const EVENTS_MAP_NAME: &str = "EBEEPYF";
const BEEPS_FREQ_RANGE: (f32, f32) = (31f32, 10000f32);

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

    let cpus = online_cpus()?;
    let ncpus = cpus.len() as f32;
    for cpu_id in cpus {
        let buf = events
            .open(cpu_id, Some(256))
            .expect("can't open perf buffer");
        let handle = handle.clone();
        spawn(async move {
            FutureMutBup::new(PerfBufferReceiver::new(buf), &handle)
                .activate::<_, Error>(|packets: Vec<PacketInfo>| {
                    // print!("{} ", packets.len());
                    let len = packets.len();
                    packets
                        .iter()
                        .fold(mixer(1, 48000), |(mixer, mix), p| {
                            mixer.add(
                                per_ip_sine(p.src.ip).amplify((1f32 / ncpus) * (1f32 / len as f32)),
                            );
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
