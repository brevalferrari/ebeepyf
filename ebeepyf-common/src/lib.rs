#![no_std]
#![deny(
    clippy::all,
    trivial_numeric_casts,
    single_use_lifetimes,
    unused_crate_dependencies
)]

use derive_new::new;

/// IP & port
#[derive(new, Copy, Clone, Debug)]
pub struct IPP {
    pub ip: [u8; 4],
    pub port: u16,
}

/// Basic information on a packet
#[derive(new, Copy, Clone, Debug)]
pub struct PacketInfo {
    pub src: IPP,
    pub dst: IPP,
}

#[cfg(feature = "aya")]
pub mod aya_impls {
    use crate::PacketInfo;
    use aya::Pod;
    use core::mem;

    unsafe impl Pod for PacketInfo {}
    impl TryFrom<&[u8]> for PacketInfo {
        type Error = ();
        fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
            if value.len() < mem::size_of::<Self>() {
                return Err(());
            }
            let ptr = value.as_ptr() as *const Self;
            Ok(unsafe { ptr.read_unaligned() })
        }
    }
}
