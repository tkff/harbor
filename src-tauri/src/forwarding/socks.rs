//! SOCKS5 CONNECT, with destination resolution performed by the SSH server.
use anyhow::{bail, Context, Result};
use std::net::{Ipv4Addr, Ipv6Addr};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub(super) async fn reply(tcp: &mut TcpStream, status: u8) -> Result<()> {
    // SSH direct-tcpip does not report the destination-side bound address.
    tcp.write_all(&[5, status, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
    Ok(())
}

pub(super) async fn request(tcp: &mut TcpStream) -> Result<(String, u16)> {
    let mut greeting = [0; 2];
    tcp.read_exact(&mut greeting).await?;
    if greeting[0] != 5 {
        bail!("Only SOCKS5 is supported");
    }
    let mut methods = vec![0; greeting[1] as usize];
    tcp.read_exact(&mut methods).await?;
    if !methods.contains(&0) {
        tcp.write_all(&[5, 255]).await?;
        bail!("SOCKS client must allow no-authentication mode");
    }
    tcp.write_all(&[5, 0]).await?;
    let mut header = [0; 4];
    tcp.read_exact(&mut header).await?;
    if header[0] != 5 || header[2] != 0 {
        reply(tcp, 1).await?;
        bail!("Invalid SOCKS5 request");
    }
    if header[1] != 1 {
        reply(tcp, 7).await?;
        bail!("SOCKS supports TCP CONNECT only; BIND and UDP are unsupported");
    }
    let host = match header[3] {
        1 => {
            let mut ip = [0; 4];
            tcp.read_exact(&mut ip).await?;
            Ipv4Addr::from(ip).to_string()
        }
        4 => {
            let mut ip = [0; 16];
            tcp.read_exact(&mut ip).await?;
            Ipv6Addr::from(ip).to_string()
        }
        3 => {
            let length = tcp.read_u8().await? as usize;
            let mut name = vec![0; length];
            tcp.read_exact(&mut name).await?;
            if name.is_empty()
                || !name.is_ascii()
                || name
                    .iter()
                    .any(|b| !b.is_ascii_alphanumeric() && !b".-_".contains(b))
            {
                reply(tcp, 8).await?;
                bail!("Invalid SOCKS destination hostname; use ASCII or punycode");
            }
            String::from_utf8(name).context("Invalid SOCKS hostname")?
        }
        _ => {
            reply(tcp, 8).await?;
            bail!("Unsupported SOCKS address type");
        }
    };
    let port = tcp.read_u16().await?;
    if port == 0 {
        reply(tcp, 1).await?;
        bail!("SOCKS destination port must be nonzero");
    }
    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    async fn parse(packet: &[u8], failure: Option<u8>) -> Result<(String, u16)> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let addr = listener.local_addr()?;
        let task = tokio::spawn(async move { request(&mut listener.accept().await?.0).await });
        let mut tcp = TcpStream::connect(addr).await?;
        // Fragmented greeting and requests must work without discarding pipelined bytes.
        tcp.write_all(&[5]).await?;
        tcp.write_all(&[2, 2, 0]).await?;
        let mut method = [0; 2];
        tcp.read_exact(&mut method).await?;
        assert_eq!(method, [5, 0]);
        for byte in packet {
            tcp.write_all(&[*byte]).await?;
        }
        if let Some(status) = failure {
            let mut response = [0; 10];
            tcp.read_exact(&mut response).await?;
            assert_eq!(response, [5, status, 0, 1, 0, 0, 0, 0, 0, 0]);
        }
        tcp.shutdown().await?;
        tokio::time::timeout(std::time::Duration::from_secs(3), task).await??
    }

    #[tokio::test]
    async fn ipv4_ipv6_and_hostname_requests() -> Result<()> {
        assert_eq!(
            parse(&[5, 1, 0, 1, 127, 0, 0, 1, 1, 187], None).await?,
            ("127.0.0.1".into(), 443)
        );
        let mut ipv6 = vec![5, 1, 0, 4];
        ipv6.extend(Ipv6Addr::LOCALHOST.octets());
        ipv6.extend([0, 22]);
        assert_eq!(parse(&ipv6, None).await?, ("::1".into(), 22));
        let name = b"remote-only.harbor.invalid";
        let mut packet = vec![5, 1, 0, 3, name.len() as u8];
        packet.extend(name);
        packet.extend([0, 80]);
        assert_eq!(
            parse(&packet, None).await?,
            (String::from_utf8(name.to_vec())?, 80)
        );
        Ok(())
    }

    #[tokio::test]
    async fn rejects_unsupported_commands_addresses_auth_and_truncated_input() -> Result<()> {
        for command in [2, 3] {
            assert!(parse(&[5, command, 0, 1], Some(7)).await.is_err());
        }
        assert!(parse(&[5, 1, 0, 2], Some(8)).await.is_err());
        assert!(parse(&[5, 1, 1, 1], Some(1)).await.is_err());
        assert!(parse(&[5, 1, 0, 3, 0], Some(8)).await.is_err());
        assert!(parse(&[5, 1, 0, 1, 127, 0, 0, 1, 0, 0], Some(1))
            .await
            .is_err());
        assert!(parse(&[5, 1, 0, 3, 10, b'a'], None).await.is_err());
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let addr = listener.local_addr()?;
        let task = tokio::spawn(async move { request(&mut listener.accept().await?.0).await });
        let mut tcp = TcpStream::connect(addr).await?;
        tcp.write_all(&[5, 1, 2]).await?;
        let mut response = [0; 2];
        tcp.read_exact(&mut response).await?;
        assert_eq!(response, [5, 255]);
        assert!(task.await?.is_err());
        Ok(())
    }
}
