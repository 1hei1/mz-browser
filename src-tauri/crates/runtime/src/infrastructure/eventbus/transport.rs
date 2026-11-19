use super::error::{EventBusError, Result};
use super::message::Message;

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::infrastructure::diagnostics::{log_error, log_info};
    use bytes::BytesMut;
    use std::ffi::OsString;
    use std::sync::Arc;
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
    use tokio::sync::Mutex;

    const PIPE_PREFIX: &str = r"\\.\pipe\mz_";
    const READ_BUFFER_SIZE: usize = 65536;

    pub fn get_pipe_path(env_id: &str) -> String {
        format!("{}{}", PIPE_PREFIX, env_id)
    }

    #[allow(dead_code)]
    pub struct PipeServer {
        env_id: String,
        pipe_path: String,
    }

    #[allow(dead_code)]
    impl PipeServer {
        pub fn new(env_id: &str) -> Self {
            Self {
                env_id: env_id.to_string(),
                pipe_path: get_pipe_path(env_id),
            }
        }

        pub fn env_id(&self) -> &str {
            &self.env_id
        }

        pub fn pipe_path(&self) -> &str {
            &self.pipe_path
        }

        pub async fn accept(&self) -> Result<PipeConnection> {
            let pipe_path_os: OsString = OsString::from(&self.pipe_path);
            let server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&pipe_path_os)
                .map_err(|error| {
                    log_error("eventbus", format!("Failed to create pipe: {}", error));
                    EventBusError::Connection(format!("failed to create pipe: {}", error))
                })?;

            log_info(
                "eventbus",
                format!("Waiting for connection on: {}", self.pipe_path),
            );

            server.connect().await.map_err(|error| {
                log_error(
                    "eventbus",
                    format!("Failed to accept connection: {}", error),
                );
                EventBusError::Connection(format!("failed to accept connection: {}", error))
            })?;

            log_info(
                "eventbus",
                format!("Client connected on: {}", self.pipe_path),
            );

            Ok(PipeConnection::new(server, self.env_id.clone()))
        }
    }

    pub struct PipeConnection {
        pipe: Arc<NamedPipeServer>,
        env_id: String,
        read_buffer: Arc<Mutex<BytesMut>>,
    }

    impl PipeConnection {
        fn new(pipe: NamedPipeServer, env_id: String) -> Self {
            Self {
                pipe: Arc::new(pipe),
                env_id,
                read_buffer: Arc::new(Mutex::new(BytesMut::with_capacity(READ_BUFFER_SIZE))),
            }
        }

        pub fn env_id(&self) -> &str {
            &self.env_id
        }

        pub async fn send(&self, msg: &Message) -> Result<()> {
            let data = msg.encode()?;

            let mut pos = 0;
            while pos < data.len() {
                self.pipe.writable().await.map_err(|error| {
                    log_error("eventbus", format!("Writable wait failed: {}", error));
                    EventBusError::SendFailed(error.to_string())
                })?;

                match self.pipe.try_write(&data[pos..]) {
                    Ok(n) => pos += n,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(error) => {
                        log_error("eventbus", format!("Send failed: {}", error));
                        return Err(EventBusError::SendFailed(error.to_string()));
                    }
                }
            }

            Ok(())
        }

        pub async fn recv(&self) -> Result<Message> {
            let mut temp_buf = [0u8; READ_BUFFER_SIZE];

            loop {
                {
                    let mut buffer = self.read_buffer.lock().await;
                    if let Some((msg, consumed)) = Message::try_decode(&buffer)? {
                        let _ = buffer.split_to(consumed);
                        return Ok(msg);
                    }
                }

                self.pipe.readable().await.map_err(|error| {
                    log_error("eventbus", format!("Readable wait failed: {}", error));
                    EventBusError::ReceiveFailed(error.to_string())
                })?;

                match self.pipe.try_read(&mut temp_buf) {
                    Ok(0) => return Err(EventBusError::ReceiveFailed("connection closed".into())),
                    Ok(n) => {
                        let mut buffer = self.read_buffer.lock().await;
                        buffer.extend_from_slice(&temp_buf[..n]);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(error) => {
                        log_error("eventbus", format!("Read failed: {}", error));
                        return Err(EventBusError::ReceiveFailed(error.to_string()));
                    }
                }
            }
        }

        pub async fn try_recv(&self) -> Result<Option<Message>> {
            {
                let mut buffer = self.read_buffer.lock().await;
                if let Some((msg, consumed)) = Message::try_decode(&buffer)? {
                    let _ = buffer.split_to(consumed);
                    return Ok(Some(msg));
                }
            }

            let mut temp_buf = [0u8; READ_BUFFER_SIZE];
            let n = match self.pipe.try_read(&mut temp_buf) {
                Ok(n) => n,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
                Err(error) => return Err(EventBusError::ReceiveFailed(error.to_string())),
            };

            if n == 0 {
                return Err(EventBusError::ReceiveFailed("connection closed".into()));
            }

            {
                let mut buffer = self.read_buffer.lock().await;
                buffer.extend_from_slice(&temp_buf[..n]);
                if let Some((msg, consumed)) = Message::try_decode(&buffer)? {
                    let _ = buffer.split_to(consumed);
                    return Ok(Some(msg));
                }
            }

            Ok(None)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_pipe_path() {
            let path = get_pipe_path("env_123");
            assert_eq!(path, r"\\.\pipe\mz_env_123");
        }
    }
}

#[cfg(unix)]
#[allow(dead_code)]
mod imp {
    //! Linux/macOS：Unix domain socket，路径 `$MZ_EVENTBUS_DIR/mz_<env_id>.sock`（默认 /tmp），
    //! 与内核 transport_posix.cc 对应；字节流格式与 Windows 命名管道一致。
    use super::*;
    use crate::infrastructure::diagnostics::{log_error, log_info};
    use bytes::BytesMut;
    use std::sync::Arc;
    use tokio::net::{UnixListener, UnixStream};
    use tokio::sync::Mutex;

    const READ_BUFFER_SIZE: usize = 65536;

    pub fn socket_dir() -> String {
        std::env::var("MZ_EVENTBUS_DIR")
            .ok()
            .filter(|dir| !dir.trim().is_empty())
            .map(|dir| dir.trim_end_matches('/').to_string())
            .unwrap_or_else(|| "/tmp".to_string())
    }

    pub fn get_pipe_path(env_id: &str) -> String {
        format!("{}/mz_{}.sock", socket_dir(), env_id)
    }

    pub struct PipeServer {
        env_id: String,
        pipe_path: String,
    }

    impl PipeServer {
        pub fn new(env_id: &str) -> Self {
            Self {
                env_id: env_id.to_string(),
                pipe_path: get_pipe_path(env_id),
            }
        }

        pub fn env_id(&self) -> &str {
            &self.env_id
        }

        pub fn pipe_path(&self) -> &str {
            &self.pipe_path
        }

        pub async fn accept(&self) -> Result<PipeConnection> {
            // 上次异常退出留下的套接字文件会让 bind 失败
            let _ = std::fs::remove_file(&self.pipe_path);
            let listener = UnixListener::bind(&self.pipe_path).map_err(|error| {
                log_error("eventbus", format!("Failed to bind socket: {}", error));
                EventBusError::Connection(format!("failed to bind socket: {}", error))
            })?;
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    &self.pipe_path,
                    std::fs::Permissions::from_mode(0o600),
                );
            }
            log_info(
                "eventbus",
                format!("Waiting for connection on: {}", self.pipe_path),
            );
            let accepted = listener.accept().await;
            // 每个环境只接一个内核连接，接上后就不再需要监听文件
            drop(listener);
            let _ = std::fs::remove_file(&self.pipe_path);
            let (stream, _) = accepted.map_err(|error| {
                log_error("eventbus", format!("Failed to accept connection: {}", error));
                EventBusError::Connection(format!("failed to accept connection: {}", error))
            })?;
            log_info(
                "eventbus",
                format!("Client connected on: {}", self.pipe_path),
            );
            Ok(PipeConnection::new(stream, self.env_id.clone()))
        }
    }

    pub struct PipeConnection {
        stream: Arc<UnixStream>,
        env_id: String,
        read_buffer: Arc<Mutex<BytesMut>>,
    }

    impl PipeConnection {
        fn new(stream: UnixStream, env_id: String) -> Self {
            Self {
                stream: Arc::new(stream),
                env_id,
                read_buffer: Arc::new(Mutex::new(BytesMut::with_capacity(READ_BUFFER_SIZE))),
            }
        }

        pub fn env_id(&self) -> &str {
            &self.env_id
        }

        pub async fn send(&self, msg: &Message) -> Result<()> {
            let data = msg.encode()?;
            let mut pos = 0;
            while pos < data.len() {
                self.stream.writable().await.map_err(|error| {
                    log_error("eventbus", format!("Writable wait failed: {}", error));
                    EventBusError::SendFailed(error.to_string())
                })?;
                match self.stream.try_write(&data[pos..]) {
                    Ok(n) => pos += n,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(error) => {
                        log_error("eventbus", format!("Send failed: {}", error));
                        return Err(EventBusError::SendFailed(error.to_string()));
                    }
                }
            }
            Ok(())
        }

        pub async fn recv(&self) -> Result<Message> {
            let mut temp_buf = [0u8; READ_BUFFER_SIZE];
            loop {
                {
                    let mut buffer = self.read_buffer.lock().await;
                    if let Some((msg, consumed)) = Message::try_decode(&buffer)? {
                        let _ = buffer.split_to(consumed);
                        return Ok(msg);
                    }
                }
                self.stream.readable().await.map_err(|error| {
                    log_error("eventbus", format!("Readable wait failed: {}", error));
                    EventBusError::ReceiveFailed(error.to_string())
                })?;
                match self.stream.try_read(&mut temp_buf) {
                    Ok(0) => return Err(EventBusError::ReceiveFailed("connection closed".into())),
                    Ok(n) => {
                        let mut buffer = self.read_buffer.lock().await;
                        buffer.extend_from_slice(&temp_buf[..n]);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(error) => {
                        log_error("eventbus", format!("Read failed: {}", error));
                        return Err(EventBusError::ReceiveFailed(error.to_string()));
                    }
                }
            }
        }

        pub async fn try_recv(&self) -> Result<Option<Message>> {
            {
                let mut buffer = self.read_buffer.lock().await;
                if let Some((msg, consumed)) = Message::try_decode(&buffer)? {
                    let _ = buffer.split_to(consumed);
                    return Ok(Some(msg));
                }
            }
            let mut temp_buf = [0u8; READ_BUFFER_SIZE];
            let n = match self.stream.try_read(&mut temp_buf) {
                Ok(n) => n,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
                Err(error) => return Err(EventBusError::ReceiveFailed(error.to_string())),
            };
            if n == 0 {
                return Err(EventBusError::ReceiveFailed("connection closed".into()));
            }
            let mut buffer = self.read_buffer.lock().await;
            buffer.extend_from_slice(&temp_buf[..n]);
            if let Some((msg, consumed)) = Message::try_decode(&buffer)? {
                let _ = buffer.split_to(consumed);
                return Ok(Some(msg));
            }
            Ok(None)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn unix_socket_round_trip() {
            use crate::infrastructure::eventbus::topics::Topic;
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let env_id = format!("test_{}", std::process::id());
            let server = PipeServer::new(&env_id);
            let path = server.pipe_path().to_string();
            let accept = tokio::spawn(async move { server.accept().await });
            let mut client = loop {
                if let Ok(stream) = UnixStream::connect(&path).await {
                    break stream;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            };
            let conn = accept.await.unwrap().unwrap();
            // 连上后监听文件已删除
            assert!(!std::path::Path::new(&path).exists());

            let msg = Message::event(Topic::LaunchConfig, b"{\"a\":1}".to_vec());
            conn.send(&msg).await.unwrap();
            let encoded = msg.encode().unwrap();
            let mut got = vec![0u8; encoded.len()];
            client.read_exact(&mut got).await.unwrap();
            assert_eq!(&got[..], &encoded[..]);

            // 分两段写，接收端要能拼起来
            let (a, b) = encoded.split_at(5);
            client.write_all(a).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            client.write_all(b).await.unwrap();
            let back = conn.recv().await.unwrap();
            assert_eq!(back.data, msg.data);
        }

        #[test]
        fn socket_path_defaults_to_tmp() {
            if std::env::var("MZ_EVENTBUS_DIR").is_err() {
                assert_eq!(get_pipe_path("env_123"), "/tmp/mz_env_123.sock");
            }
        }
    }
}

pub use imp::{PipeConnection, PipeServer, get_pipe_path};
