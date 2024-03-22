use anyhow::Context;
use aya::{
    include_bytes_aligned,
    maps::{perf::Events, AsyncPerfEventArray},
    programs::{Xdp, XdpFlags},
    util::online_cpus,
    Ebpf,
};
use bytes::BytesMut;
use clap::Parser;
use ebeepyf_common::PacketInfo;
use tokio::{signal, spawn};

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

    let mut events: AsyncPerfEventArray<_> = bpf
        .take_map(EVENTS_MAP_NAME)
        .context("can't take map")?
        .try_into()
        .context("can't convert map")?;

    for cpu_id in online_cpus()? {
        let mut buf = events.open(cpu_id, Some(256))?;
        spawn(async move {
            loop {
                let mut bufs = vec![BytesMut::zeroed(20); 10];
                let Events { read, lost: _ } = buf.read_events(&mut bufs).await.unwrap();
                bufs.iter()
                    .take(read)
                    .for_each(|bytes| println!("{:?}", PacketInfo::try_from(bytes.as_ref())));
            }
        });
    }

    println!("Waiting for Ctrl-C...");
    signal::ctrl_c().await?;
    println!("Exiting...");

    Ok(())
}
