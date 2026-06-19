use anyhow::Result;
use bytes::Bytes;
use iroh::endpoint::Connection;
use tokio::sync::mpsc;

use crate::tun::TunDevice;

pub async fn run(tun: TunDevice, conn: Connection) -> Result<()> {
    let (tun_tx, tun_rx) = mpsc::channel::<Vec<u8>>(256);

    let tun_to_iroh = tokio::spawn(tun_read_loop(tun, conn.clone(), tun_rx));
    let iroh_to_tun = tokio::spawn(iroh_read_loop(conn, tun_tx));

    tokio::select! {
        r = tun_to_iroh => r??,
        r = iroh_to_tun => r??,
    }

    Ok(())
}

async fn tun_read_loop(
    mut tun: TunDevice,
    conn: Connection,
    mut incoming: mpsc::Receiver<Vec<u8>>,
) -> Result<()> {
    let mut buf = vec![0u8; 1500];
    loop {
        tokio::select! {
            result = tun.read_packet(&mut buf) => {
                let n = result?;
                if n > 0 {
                    conn.send_datagram(Bytes::copy_from_slice(&buf[..n]))?;
                }
            }
            Some(packet) = incoming.recv() => {
                tun.write_packet(&packet).await?;
            }
        }
    }
}

async fn iroh_read_loop(conn: Connection, tun_tx: mpsc::Sender<Vec<u8>>) -> Result<()> {
    loop {
        let datagram = conn.read_datagram().await?;
        tun_tx.send(datagram.to_vec()).await?;
    }
}
