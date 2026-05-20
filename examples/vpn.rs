//! An VPN example
//!
//! This creates one endpoint of a VPN. It takes two parameters — local address and the address
//! address of the other endpoint, and sends all packets there, encapsulated in UDP. Packets
//! received from the other side are put to the kernel from the other side.
//!
//! Unlike the other examples, this doesn't configure the kernel endpoint and it is left up for the
//! caller to bring the interface up and add an address to it (or possibly some routes).
//!
//! # Warning
//!
//! Do not use as a VPN in any real-life situation. There's no authentication, encryption, nearly
//! no error handling, etc.

use std::env;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tun_tap::r#async::Async;
use tun_tap::{Iface, Mode};

#[tokio::main]
async fn main() -> io::Result<()> {
    let loc_address: SocketAddr = env::args().nth(1).unwrap().parse().unwrap();
    let rem_address: SocketAddr = env::args().nth(2).unwrap().parse().unwrap();

    let socket = Arc::new(UdpSocket::bind(loc_address).await?);
    let tun = Iface::new("vpn%d", Mode::Tun).unwrap();
    let (mut tun_reader, mut tun_writer) = tokio::io::split(Async::new(tun).unwrap());

    // TUN → UDP (packets from kernel → remote peer)
    let tun_to_udp = {
        let socket = Arc::clone(&socket);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1504];
            loop {
                let n = tun_reader.read(&mut buf).await?;
                socket.send_to(&buf[..n], rem_address).await?;
            }
            #[allow(unreachable_code)]
            Ok::<_, io::Error>(())
        })
    };

    // UDP → TUN (packets from remote peer → kernel)
    let udp_to_tun = {
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1504];
            loop {
                let (n, _src) = socket.recv_from(&mut buf).await?;
                tun_writer.write_all(&buf[..n]).await?;
            }
            #[allow(unreachable_code)]
            Ok::<_, io::Error>(())
        })
    };

    let _ = tokio::try_join!(tun_to_udp, udp_to_tun);
    Ok(())
}
