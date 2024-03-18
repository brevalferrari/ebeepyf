#![no_std]

#[cfg(feature = "aya")]
use aya::Pod;
use derive_new::new;

#[derive(new, Copy, Clone)]
pub struct IPP {
    pub ip: u32,
    pub port: u16,
}

#[derive(new, Copy, Clone)]
pub struct PacketInfo {
    pub src: IPP,
    pub dst: IPP,
}

#[cfg(feature = "aya")]
unsafe impl Pod for PacketInfo {}
