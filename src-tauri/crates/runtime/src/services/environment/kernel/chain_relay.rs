//! 链式代理（前置代理）中继。
//!
//! 有些 HTTP/SOCKS5 代理本机直连不通，但经过本机的代理软件（Clash、v2rayN 等）中转就能连上。
//! 这里在 127.0.0.1 上开一个端口：每个进来的连接，先经“前置代理”建立到目标代理
//! `host:port` 的 TCP 隧道，然后原样转发字节。浏览器、出口检测、代理测试都把这个本地端口
//! 当成目标代理本身来连，目标代理的协议和账号密码仍由它们自己处理，所以 HTTP/HTTPS/SOCKS5
//! 和认证都不需要在这里重新实现。
//!
//! 链路：浏览器 → 127.0.0.1:中继端口 → 前置代理 → 目标代理 → 网站
//!
//! 前置代理连不上时连接直接失败，绝不改走直连。
//! 同一对（前置代理，目标代理）只开一个中继端口，进程内复用。

use std::collections::HashMap;
use std::sync::LazyLock;

use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::socks_forward::{Upstream, connect_upstream};

/// 前置代理（只支持 http / socks5，账号密码可选）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FrontProxy {
    pub scheme: FrontScheme,
    pub host: String,
    pub port: u16,
    pub auth: Option<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontScheme {
    Http,
    Socks5,
}

impl FrontProxy {
    /// 解析前置代理地址：`http://127.0.0.1:7890`、`socks5://user:pass@127.0.0.1:7891`。
    /// 不写协议时按 http 处理（`127.0.0.1:7890`）。
    pub fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("前置代理地址为空".into());
        }
        let (scheme, rest) = match input.split_once("://") {
            Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
            None => ("http".to_string(), input),
        };
        let scheme = match scheme.as_str() {
            "http" => FrontScheme::Http,
            "socks5" | "socks5h" | "socks" => FrontScheme::Socks5,
            other => {
                return Err(format!(
                    "前置代理不支持 {other} 协议，只支持 http 和 socks5"
                ));
            }
        };
        let rest = rest.trim_end_matches('/');
        let (auth, address) = match rest.rsplit_once('@') {
            Some((userinfo, address)) => {
                let (username, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
                (
                    Some((percent_decode(username), percent_decode(password))),
                    address,
                )
            }
            None => (None, rest),
        };
        let (host, port) = address
            .rsplit_once(':')
            .ok_or_else(|| format!("前置代理地址缺少端口：{input}"))?;
        let host = host.trim_start_matches('[').trim_end_matches(']').trim().to_string();
        let port: u16 = port
            .trim()
            .parse()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| format!("前置代理端口无效：{input}"))?;
        if host.is_empty() {
            return Err(format!("前置代理地址无效：{input}"));
        }
        Ok(Self {
            scheme,
            host,
            port,
            auth,
        })
    }

    fn upstream(&self) -> Upstream {
        let host = self.host.clone();
        let port = self.port;
        let auth = self.auth.clone();
        match self.scheme {
            FrontScheme::Http => Upstream::Http { host, port, auth },
            FrontScheme::Socks5 => Upstream::Socks5 { host, port, auth },
        }
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

struct Relay {
    port: u16,
    task: JoinHandle<()>,
}

type RelayKey = (FrontProxy, String, u16);

static RELAYS: LazyLock<Mutex<HashMap<RelayKey, Relay>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 返回一个本地端口：连到它就等于经前置代理连到 `target_host:target_port`。
/// 同一组参数重复调用返回同一个端口。
pub async fn ensure_relay(
    front: &FrontProxy,
    target_host: &str,
    target_port: u16,
) -> Result<u16, String> {
    let target_host = target_host.trim().trim_start_matches('[').trim_end_matches(']').to_string();
    if target_host.is_empty() || target_port == 0 {
        return Err("目标代理地址或端口无效".into());
    }
    let key = (front.clone(), target_host.clone(), target_port);
    let mut relays = RELAYS.lock().await;
    if let Some(relay) = relays.get(&key)
        && !relay.task.is_finished()
    {
        return Ok(relay.port);
    }
    let relay = start_relay(front.upstream(), target_host, target_port).await?;
    let port = relay.port;
    relays.insert(key, relay);
    Ok(port)
}

async fn start_relay(
    front: Upstream,
    target_host: String,
    target_port: u16,
) -> Result<Relay, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| format!("链式代理本地端口启动失败：{error}"))?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut client, _)) = listener.accept().await else {
                continue;
            };
            let front = front.clone();
            let target_host = target_host.clone();
            tokio::spawn(async move {
                match connect_upstream(&front, &target_host, target_port).await {
                    Ok(mut remote) => {
                        let _ = tokio::io::copy_bidirectional(&mut client, &mut remote).await;
                    }
                    Err(error) => {
                        // 直接断开，浏览器会看到代理连接失败；不会改走直连
                        log::warn!(
                            "链式代理：经前置代理连接 {target_host}:{target_port} 失败：{error}"
                        );
                    }
                }
            });
        }
    });
    Ok(Relay { port, task })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    #[test]
    fn parses_front_proxy_addresses() {
        assert_eq!(
            FrontProxy::parse("http://127.0.0.1:7890").unwrap(),
            FrontProxy {
                scheme: FrontScheme::Http,
                host: "127.0.0.1".into(),
                port: 7890,
                auth: None
            }
        );
        assert_eq!(
            FrontProxy::parse(" 127.0.0.1:7890 ").unwrap(),
            FrontProxy {
                scheme: FrontScheme::Http,
                host: "127.0.0.1".into(),
                port: 7890,
                auth: None
            }
        );
        assert_eq!(
            FrontProxy::parse("socks5://u%40x:p%3Aw@[::1]:7891/").unwrap(),
            FrontProxy {
                scheme: FrontScheme::Socks5,
                host: "::1".into(),
                port: 7891,
                auth: Some(("u@x".into(), "p:w".into())),
            }
        );
        assert!(FrontProxy::parse("").is_err());
        assert!(FrontProxy::parse("https://127.0.0.1:7890").is_err());
        assert!(FrontProxy::parse("http://127.0.0.1").is_err());
        assert!(FrontProxy::parse("http://127.0.0.1:0").is_err());
    }

    /// 假的 HTTP 前置代理：收 CONNECT，记下目标，然后回显
    async fn fake_http_front() -> (u16, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).await.unwrap();
                head.push(byte[0]);
            }
            socket.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await.unwrap();
            let mut buf = [0u8; 4];
            socket.read_exact(&mut buf).await.unwrap();
            socket.write_all(&buf).await.unwrap();
            String::from_utf8(head).unwrap()
        });
        (port, task)
    }

    #[tokio::test]
    async fn relays_raw_bytes_through_the_front_proxy() {
        let (front_port, seen) = fake_http_front().await;
        let front = FrontProxy::parse(&format!("http://127.0.0.1:{front_port}")).unwrap();
        let port = ensure_relay(&front, "203.0.113.7", 1080).await.unwrap();
        // 同一组参数复用同一个端口
        assert_eq!(
            ensure_relay(&front, "203.0.113.7", 1080).await.unwrap(),
            port
        );

        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        // 这几个字节是浏览器发给目标代理的（比如 SOCKS5 握手），中继必须原样送到
        stream.write_all(&[5, 1, 0, 9]).await.unwrap();
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, [5, 1, 0, 9]);
        assert!(seen.await.unwrap().starts_with("CONNECT 203.0.113.7:1080 HTTP/1.1\r\n"));
    }

    #[tokio::test]
    async fn a_dead_front_proxy_closes_the_connection() {
        let unused = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_port = unused.local_addr().unwrap().port();
        drop(unused);
        let front = FrontProxy::parse(&format!("socks5://127.0.0.1:{dead_port}")).unwrap();
        let port = ensure_relay(&front, "proxy.test", 8080).await.unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut buf = [0u8; 1];
        let read = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut buf))
            .await
            .unwrap();
        assert!(matches!(read, Ok(0) | Err(_)));
    }

    /// 真正转发的 HTTP CONNECT 代理（模拟本机 Clash）
    async fn tunneling_http_front() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut byte = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        socket.read_exact(&mut byte).await.unwrap();
                        head.push(byte[0]);
                    }
                    let head = String::from_utf8(head).unwrap();
                    let target = head.split_whitespace().nth(1).unwrap().to_string();
                    let mut remote = TcpStream::connect(target).await.unwrap();
                    socket.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut remote).await;
                });
            }
        });
        port
    }

    /// 需要账号密码的 SOCKS5 目标代理，只放行 user/pass
    async fn socks5_with_auth() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut greeting = [0u8; 2];
            socket.read_exact(&mut greeting).await.unwrap();
            let mut methods = vec![0u8; greeting[1] as usize];
            socket.read_exact(&mut methods).await.unwrap();
            assert!(methods.contains(&2));
            socket.write_all(&[5, 2]).await.unwrap();
            let mut ver_ulen = [0u8; 2];
            socket.read_exact(&mut ver_ulen).await.unwrap();
            let mut user = vec![0u8; ver_ulen[1] as usize];
            socket.read_exact(&mut user).await.unwrap();
            let mut plen = [0u8; 1];
            socket.read_exact(&mut plen).await.unwrap();
            let mut pass = vec![0u8; plen[0] as usize];
            socket.read_exact(&mut pass).await.unwrap();
            assert_eq!((user.as_slice(), pass.as_slice()), (&b"user"[..], &b"pass"[..]));
            socket.write_all(&[1, 0]).await.unwrap();
            let mut request = [0u8; 5];
            socket.read_exact(&mut request).await.unwrap();
            let mut name = vec![0u8; request[4] as usize];
            socket.read_exact(&mut name).await.unwrap();
            let mut port = [0u8; 2];
            socket.read_exact(&mut port).await.unwrap();
            let target = format!("{}:{}", String::from_utf8(name).unwrap(), u16::from_be_bytes(port));
            let mut remote = TcpStream::connect(target).await.unwrap();
            socket.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.unwrap();
            let _ = tokio::io::copy_bidirectional(&mut socket, &mut remote).await;
        });
        port
    }

    #[tokio::test]
    async fn full_chain_reaches_the_site_through_front_and_target_proxies() {
        // 网站：回显
        let site = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let site_port = site.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut socket, _) = site.accept().await.unwrap();
            let mut buf = [0u8; 5];
            socket.read_exact(&mut buf).await.unwrap();
            socket.write_all(&buf).await.unwrap();
        });
        let front_port = tunneling_http_front().await;
        let target_port = socks5_with_auth().await;

        let front = FrontProxy::parse(&format!("http://127.0.0.1:{front_port}")).unwrap();
        let relay_port = ensure_relay(&front, "127.0.0.1", target_port).await.unwrap();

        // 浏览器一侧：照常连“目标代理”（其实是中继端口），自己做 SOCKS5 认证
        let browser_side = Upstream::Socks5 {
            host: "127.0.0.1".into(),
            port: relay_port,
            auth: Some(("user".into(), "pass".into())),
        };
        let mut stream = connect_upstream(&browser_side, "127.0.0.1", site_port).await.unwrap();
        stream.write_all(b"hello").await.unwrap();
        let mut buf = [0u8; 5];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
    }
}
