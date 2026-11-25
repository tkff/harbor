use crate::auth;
use crate::forwarding::{self, Routes, Tunnels};
use crate::storage::Storage;
use anyhow::{bail, Context, Result};
use russh::{
    client,
    keys::{HashAlg, PublicKeyOrCertificate},
    ChannelMsg, Disconnect,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    Data {
        bytes: Vec<u8>,
    },
    HostKey {
        host: String,
        port: u16,
        fingerprint: String,
    },
    Connected,
    AuthenticationFailed {
        username: String,
        method: auth::Method,
    },
    Forward {
        #[serde(rename = "ruleId")]
        rule_id: String,
        state: String,
        message: Option<String>,
    },
    Closed {
        message: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginCredentials {
    pub username: String,
    pub password: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SaveProfile;
    use russh::{
        server::{self, Server as _},
        ChannelId, Pty,
    };
    use tauri::ipc::InvokeResponseBody;

    type RemoteListeners =
        Arc<Mutex<HashMap<(String, u32), (CancellationToken, tokio::task::JoinHandle<()>)>>>;
    #[derive(Clone)]
    struct TestServer {
        sizes: Arc<Mutex<Vec<(u32, u32)>>>,
        remotes: RemoteListeners,
        shells: std::collections::HashSet<ChannelId>,
        deny_remote: Arc<std::sync::atomic::AtomicBool>,
    }
    impl server::Server for TestServer {
        type Handler = Self;
        fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
            Self {
                sizes: self.sizes.clone(),
                remotes: Arc::new(Mutex::new(HashMap::new())),
                deny_remote: self.deny_remote.clone(),
                shells: Default::default(),
            }
        }
    }
    impl Drop for TestServer {
        fn drop(&mut self) {
            for (_, (cancel, task)) in self.remotes.lock().unwrap().drain() {
                cancel.cancel();
                task.abort();
            }
        }
    }
    impl server::Handler for TestServer {
        type Error = russh::Error;
        async fn channel_open_direct_tcpip(
            &mut self,
            channel: russh::Channel<server::Msg>,
            host: &str,
            port: u32,
            _: &str,
            _: u32,
            reply: server::ChannelOpenHandle,
            _: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            let host = host.to_string();
            tokio::spawn(async move {
                if let Ok(mut tcp) =
                    tokio::net::TcpStream::connect((host.as_str(), port as u16)).await
                {
                    reply.accept().await;
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
                }
            });
            Ok(())
        }
        async fn tcpip_forward(
            &mut self,
            address: &str,
            port: &mut u32,
            s: &mut server::Session,
        ) -> std::result::Result<bool, Self::Error> {
            if self.deny_remote.load(std::sync::atomic::Ordering::SeqCst) {
                return Ok(false);
            }
            let Ok(listener) = tokio::net::TcpListener::bind((address, *port as u16)).await else {
                return Ok(false);
            };
            *port = u32::from(listener.local_addr().unwrap().port());
            let port = *port;
            let address = address.to_string();
            let addr = address.clone();
            let handle = s.handle();
            let cancel = CancellationToken::new();
            let token = cancel.clone();
            let task = tokio::spawn(async move {
                let mut workers = tokio::task::JoinSet::new();
                loop {
                    tokio::select! {
                        biased;
                        _=token.cancelled()=>break,
                        Some(_)=workers.join_next(),if !workers.is_empty()=>{},
                        incoming=listener.accept()=>{
                            let Ok((mut tcp,peer))=incoming else {break;};
                            let handle=handle.clone();let address=address.clone();
                            workers.spawn(async move {
                                if let Ok(channel)=handle.channel_open_forwarded_tcpip(address,port,peer.ip().to_string(),u32::from(peer.port())).await {
                                    let mut stream=channel.into_stream();
                                    let _=tokio::io::copy_bidirectional(&mut tcp,&mut stream).await;
                                }
                            });
                        }
                    }
                }
            });
            self.remotes
                .lock()
                .unwrap()
                .insert((addr, port), (cancel, task));
            Ok(true)
        }
        async fn cancel_tcpip_forward(
            &mut self,
            address: &str,
            port: u32,
            _: &mut server::Session,
        ) -> std::result::Result<bool, Self::Error> {
            let listener = self
                .remotes
                .lock()
                .unwrap()
                .remove(&(address.to_string(), port));
            if let Some((cancel, task)) = listener {
                cancel.cancel();
                let _ = task.await;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        async fn auth_password(
            &mut self,
            user: &str,
            password: &str,
        ) -> std::result::Result<server::Auth, Self::Error> {
            Ok(if user == "tester" && password == "ssh-test-password" {
                server::Auth::Accept
            } else {
                server::Auth::reject()
            })
        }
        async fn channel_open_session(
            &mut self,
            _: russh::Channel<server::Msg>,
            reply: server::ChannelOpenHandle,
            _: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            reply.accept().await;
            Ok(())
        }
        async fn pty_request(
            &mut self,
            ch: ChannelId,
            _: &str,
            cols: u32,
            rows: u32,
            _: u32,
            _: u32,
            _: &[(Pty, u32)],
            s: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            self.sizes.lock().unwrap().push((cols, rows));
            s.channel_success(ch)?;
            Ok(())
        }
        async fn shell_request(
            &mut self,
            ch: ChannelId,
            s: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            self.shells.insert(ch);
            s.channel_success(ch)?;
            s.data(ch, b"test-shell> ".to_vec())?;
            Ok(())
        }
        async fn data(
            &mut self,
            ch: ChannelId,
            data: &[u8],
            s: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            if self.shells.contains(&ch) {
                s.data(ch, data.to_vec())?;
            }
            Ok(())
        }
        async fn window_change_request(
            &mut self,
            _: ChannelId,
            cols: u32,
            rows: u32,
            _: u32,
            _: u32,
            _: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            self.sizes.lock().unwrap().push((cols, rows));
            Ok(())
        }
    }

    fn channel() -> (
        Channel<Event>,
        tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Channel::new(move |body| {
                if let InvokeResponseBody::Json(json) = body {
                    let _ = tx.send(serde_json::from_str(&json).unwrap());
                }
                Ok(())
            }),
            rx,
        )
    }

    #[derive(Clone)]
    struct IdentityServer {
        key: String,
        ca: String,
        used: Arc<Mutex<Vec<&'static str>>>,
    }
    impl server::Server for IdentityServer {
        type Handler = Self;
        fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
            self.clone()
        }
    }
    impl server::Handler for IdentityServer {
        type Error = russh::Error;
        async fn auth_publickey(
            &mut self,
            user: &str,
            key: server::VerifiedPublicKey<'_>,
        ) -> std::result::Result<server::Auth, Self::Error> {
            if user == "tester"
                && key.public_key.fingerprint(HashAlg::Sha256).to_string() == self.key
            {
                self.used.lock().unwrap().push("key");
                Ok(server::Auth::Accept)
            } else {
                Ok(server::Auth::reject())
            }
        }
        async fn auth_openssh_certificate(
            &mut self,
            user: &str,
            cert: &russh::keys::Certificate,
        ) -> std::result::Result<server::Auth, Self::Error> {
            let ca = cert.signature_key().fingerprint(HashAlg::Sha256);
            if user == "tester"
                && ca.to_string() == self.ca
                && cert.valid_principals().iter().any(|p| p == "tester")
                && cert.critical_options().is_empty()
                && cert.validate([&ca]).is_ok()
            {
                self.used.lock().unwrap().push("certificate");
                Ok(server::Auth::Accept)
            } else {
                Ok(server::Auth::reject())
            }
        }
        async fn channel_open_session(
            &mut self,
            _: russh::Channel<server::Msg>,
            reply: server::ChannelOpenHandle,
            _: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            reply.accept().await;
            Ok(())
        }
        async fn pty_request(
            &mut self,
            ch: ChannelId,
            _: &str,
            _: u32,
            _: u32,
            _: u32,
            _: u32,
            _: &[(Pty, u32)],
            s: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            s.channel_success(ch)?;
            Ok(())
        }
        async fn shell_request(
            &mut self,
            ch: ChannelId,
            s: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            s.channel_success(ch)?;
            s.data(ch, b"identity-shell> ".to_vec())?;
            Ok(())
        }
        async fn data(
            &mut self,
            ch: ChannelId,
            data: &[u8],
            s: &mut server::Session,
        ) -> std::result::Result<(), Self::Error> {
            s.data(ch, data.to_vec())?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn ssh_encrypted_key_certificate_and_untrusted_ca() -> Result<()> {
        use russh::keys::{
            ssh_key::{certificate, LineEnding},
            Algorithm, PrivateKey,
        };
        use std::time::{SystemTime, UNIX_EPOCH};
        tokio::time::timeout(Duration::from_secs(90), async {
            let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)?;
            let ca = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)?;
            let bad_ca = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let make_cert = |ca: &PrivateKey| -> Result<String> {
                let mut b =
                    certificate::Builder::new(vec![5; 32], key.public_key(), now - 60, now + 3600)?;
                b.cert_type(certificate::CertType::User)?
                    .valid_principal("tester")?
                    .key_id("test-cert")?;
                Ok(b.sign(ca)?.to_openssh()?)
            };
            let config = Arc::new(server::Config {
                keys: vec![PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)?],
                auth_rejection_time: Duration::from_millis(5),
                ..Default::default()
            });
            let host_fingerprint = config.keys[0]
                .public_key()
                .fingerprint(HashAlg::Sha256)
                .to_string();
            let used = Arc::new(Mutex::new(vec![]));
            let mut server = IdentityServer {
                key: key.public_key().fingerprint(HashAlg::Sha256).to_string(),
                ca: ca.public_key().fingerprint(HashAlg::Sha256).to_string(),
                used: used.clone(),
            };
            let socket = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
            let port = socket.local_addr()?.port();
            let task = tokio::spawn(async move { server.run_on_socket(config, &socket).await });
            let dir = tempfile::tempdir()?;
            let vault = dir.path().join("identity.kdbx");
            let mut storage = Storage::open(&dir.path().join("local.db"))?;
            storage.unlock(
                &vault,
                zeroize::Zeroizing::new("test-master-password".into()),
                true,
            )?;
            storage.trust_key("127.0.0.1", port, &host_fingerprint)?;
            let encrypted = key
                .encrypt(&mut rand::rng(), "test-key-passphrase")?
                .to_openssh(LineEnding::LF)?;
            let profile = storage
                .save_profile(SaveProfile {
                    id: None,
                    name: "SSH identity".into(),
                    host: "127.0.0.1".into(),
                    port,
                    folder: "Tests".into(),
                    startup_snippet: None,
                    session_logging: false,
                    tags: vec![],
                    username: "tester".into(),
                    password: None,
                    forwards: vec![],
                    authentication: Some(auth::Update {
                        method: auth::Method::PrivateKey,
                        private_key: Some(encrypted.to_string()),
                        passphrase: Some("test-key-passphrase".into()),
                        ..Default::default()
                    }),
                })?
                .profiles
                .remove(0);
            for file in [&vault, &dir.path().join("local.db")] {
                assert!(!std::fs::read(file)?
                    .windows(19)
                    .any(|s| s == b"test-key-passphrase"));
            }
            let store = Arc::new(Mutex::new(storage));
            let manager = Manager::default();
            async fn terminal(
                manager: &Manager,
                store: Arc<Mutex<Storage>>,
                id: &str,
                profile: &str,
            ) -> Result<()> {
                let (events, mut rx) = channel();
                manager
                    .connect(id.into(), profile.into(), 80, 24, store, events)
                    .await?;
                manager
                    .input(id, Input::Data(b"signed-in".to_vec()))
                    .await?;
                loop {
                    let e = rx.recv().await.context("Missing terminal response")?;
                    if e["type"] == "data" {
                        let bytes: Vec<u8> = e["bytes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|n| n.as_u64().unwrap() as u8)
                            .collect();
                        if String::from_utf8_lossy(&bytes).contains("signed-in") {
                            break;
                        }
                    }
                }
                manager.close(id);
                while rx.recv().await.context("Missing close event")?["type"] != "closed" {}
                Ok(())
            }
            terminal(&manager, store.clone(), "key", &profile.id).await?;
            let save_cert = |certificate: String| -> Result<()> {
                store.lock().unwrap().save_profile(SaveProfile {
                    id: Some(profile.id.clone()),
                    name: profile.name.clone(),
                    host: profile.host.clone(),
                    port,
                    folder: "Tests".into(),
                    startup_snippet: None,
                    session_logging: false,
                    tags: vec![],
                    username: "tester".into(),
                    password: None,
                    forwards: vec![],
                    authentication: Some(auth::Update {
                        method: auth::Method::Certificate,
                        certificate: Some(certificate),
                        ..Default::default()
                    }),
                })?;
                Ok(())
            };
            save_cert(make_cert(&ca)?)?;
            {
                let mut s = store.lock().unwrap();
                s.lock();
                let state = s.unlock(
                    &vault,
                    zeroize::Zeroizing::new("test-master-password".into()),
                    false,
                )?;
                assert_eq!(state.profiles[0].auth_method, auth::Method::Certificate);
                assert_eq!(
                    state.profiles[0]
                        .identity
                        .as_ref()
                        .unwrap()
                        .certificate_id
                        .as_deref(),
                    Some("test-cert")
                );
            }
            terminal(&manager, store.clone(), "cert", &profile.id).await?;
            assert_eq!(*used.lock().unwrap(), vec!["key", "certificate"]);
            save_cert(make_cert(&bad_ca)?)?;
            let (events, _rx) = channel();
            assert!(manager
                .connect("untrusted".into(), profile.id, 80, 24, store, events)
                .await
                .is_err());
            manager.close_all();
            task.abort();
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }

    #[tokio::test]
    async fn ssh_password_host_verification_io_resize_and_disconnect() -> Result<()> {
        tokio::time::timeout(Duration::from_secs(45), async {
            let sizes = Arc::new(Mutex::new(vec![]));
            let socket = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
            let port = socket.local_addr()?.port();
            let config = Arc::new(server::Config {
                keys: vec![russh::keys::PrivateKey::random(
                    &mut rand::rng(),
                    russh::keys::Algorithm::Ed25519,
                )?],
                auth_rejection_time: Duration::from_millis(5),
                ..Default::default()
            });
            let mut server = TestServer {
                sizes: sizes.clone(),
                remotes: Arc::new(Mutex::new(HashMap::new())),
                deny_remote: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                shells: Default::default(),
            };
            let server_task =
                tokio::spawn(async move { server.run_on_socket(config, &socket).await });
            let dir = tempfile::tempdir()?;
            let mut storage = Storage::open(&dir.path().join("local.db"))?;
            storage.unlock(
                &dir.path().join("test.kdbx"),
                zeroize::Zeroizing::new("test-master-password".into()),
                true,
            )?;
            let startup = storage
                .save_snippet(crate::storage::SaveSnippet {
                    id: None,
                    name: "Startup test".into(),
                    group: "Tests".into(),
                    tags: vec![],
                    body: "echo startup-once".into(),
                })?
                .remove(0);
            let state = storage.save_profile(SaveProfile {
                id: None,
                name: "Loopback".into(),
                host: "127.0.0.1".into(),
                port,
                folder: "Tests".into(),
                startup_snippet: Some(startup.id),
                session_logging: true,
                tags: vec![],
                username: "saved-wrong-user".into(),
                password: Some("saved-wrong-password".into()),
                forwards: vec![],
                authentication: None,
            })?;
            let profile_id = state.profiles[0].id.clone();
            let store = Arc::new(Mutex::new(storage));
            let manager = Manager::default();
            let (events, mut rx) = channel();
            let cloned = manager.clone();
            let db = store.clone();
            let id = profile_id.clone();
            let connection = tokio::spawn(async move {
                cloned
                    .connect("bad-login".into(), id, 80, 24, db, events)
                    .await
            });
            let key = rx.recv().await.context("Missing host key prompt")?;
            assert_eq!(key["type"], "hostKey");
            assert!(key["fingerprint"].as_str().unwrap().starts_with("SHA256:"));
            manager.approve("bad-login", true)?;
            assert!(connection
                .await?
                .unwrap_err()
                .to_string()
                .contains("authentication failed"));
            let rejected = rx
                .recv()
                .await
                .context("Missing authentication rejection")?;
            assert_eq!(rejected["type"], "authenticationFailed");
            assert_eq!(rejected["username"], "saved-wrong-user");
            while rx.recv().await.context("Missing rejection close event")?["type"] != "closed" {}
            assert!(
                crate::session_logs::list(&store.lock().unwrap().log_access()?.unwrap())?
                    .is_empty()
            );
            let (events, mut rx) = channel();
            manager
                .connect_with_credentials(
                    "test".into(),
                    profile_id.clone(),
                    (80, 24),
                    store.clone(),
                    events,
                    Some(LoginCredentials {
                        username: "tester".into(),
                        password: Some("ssh-test-password".into()),
                    }),
                )
                .await?;
            assert_eq!(
                store.lock().unwrap().username(&profile_id)?,
                "saved-wrong-user"
            );
            assert_eq!(
                store.lock().unwrap().credentials(&profile_id)?.2.as_str(),
                "saved-wrong-password"
            );
            manager
                .input("test", Input::Data(b"hello terminal\r\n".to_vec()))
                .await?;
            manager.input("test", Input::Resize(120, 40)).await?;
            let mut output = Vec::new();
            loop {
                let e = rx.recv().await.context("Missing terminal data")?;
                if e["type"] == "data" {
                    output.extend(
                        e["bytes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_u64().unwrap() as u8),
                    );
                    if String::from_utf8_lossy(&output).contains("hello terminal\r\n") {
                        break;
                    }
                }
            }
            assert_eq!(
                String::from_utf8_lossy(&output)
                    .matches("echo startup-once\r")
                    .count(),
                1
            );
            assert!(
                String::from_utf8_lossy(&output)
                    .find("echo startup-once")
                    .unwrap()
                    < String::from_utf8_lossy(&output)
                        .find("hello terminal")
                        .unwrap()
            );
            for _ in 0..20 {
                if sizes.lock().unwrap().contains(&(120, 40)) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            assert!(sizes.lock().unwrap().contains(&(80, 24)));
            assert!(sizes.lock().unwrap().contains(&(120, 40)));
            manager.close_all();
            while rx.recv().await.context("Missing close event")?["type"] != "closed" {}
            {
                let storage = store.lock().unwrap();
                let access = storage.log_access()?.unwrap();
                let logs = crate::session_logs::list(&access)?;
                assert_eq!(logs.len(), 1);
                let log = crate::session_logs::read(&access, &logs[0].id)?;
                assert!(log.complete);
                let output: Vec<u8> = log
                    .records
                    .iter()
                    .filter_map(|record| match record {
                        crate::session_logs::Record::Output { bytes, .. } => Some(bytes.as_slice()),
                        _ => None,
                    })
                    .flatten()
                    .copied()
                    .collect();
                assert!(String::from_utf8_lossy(&output).contains("hello terminal"));
                let hosts = storage.known_hosts()?;
                assert_eq!(hosts.len(), 1);
                assert_eq!(hosts[0].port, port);
                assert!(hosts[0].fingerprint.starts_with("SHA256:"));
            }
            // A changed pinned identity must fail before authentication.
            store
                .lock()
                .unwrap()
                .sql
                .execute("UPDATE host_keys SET fingerprint='SHA256:wrong'", [])?;
            let (events, mut rx) = channel();
            let result = manager
                .connect("changed".into(), profile_id, 80, 24, store.clone(), events)
                .await;
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("fingerprint changed"));
            assert_eq!(rx.recv().await.unwrap()["type"], "closed");
            assert_eq!(
                crate::session_logs::list(&store.lock().unwrap().log_access()?.unwrap())?.len(),
                1
            );
            server_task.abort();
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }

    #[tokio::test]
    async fn forwarding_local_remote_traffic_stop_failures_and_disconnect() -> Result<()> {
        use crate::forwarding::{Direction, Rule};
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::{TcpListener, TcpStream},
        };
        tokio::time::timeout(Duration::from_secs(90),async {
            let echo=TcpListener::bind(("127.0.0.1",0)).await?;
            let target_port=echo.local_addr()?.port();
            let echo_task=tokio::spawn(async move {
                let mut clients=tokio::task::JoinSet::new();
                loop {
                    tokio::select! {
                        Some(_)=clients.join_next(),if !clients.is_empty()=>{},
                        incoming=echo.accept()=>{
                            let Ok((tcp,_))=incoming else {break;};
                            clients.spawn(async move {let(mut read,mut write)=tcp.into_split();let _=tokio::io::copy(&mut read,&mut write).await;});
                        }
                    }
                }
            });
            let local_reservation=TcpListener::bind(("127.0.0.1",0)).await?;
            let remote_reservation=TcpListener::bind(("127.0.0.1",0)).await?;
            let local_port=local_reservation.local_addr()?.port();let remote_port=remote_reservation.local_addr()?.port();
            let rules=vec![
                Rule {id:uuid::Uuid::new_v4().to_string(),direction:Direction::Local,bind_address:"127.0.0.1".into(),bind_port:local_port,target_host:"127.0.0.1".into(),target_port,auto_start:true},
                Rule {id:uuid::Uuid::new_v4().to_string(),direction:Direction::Remote,bind_address:"127.0.0.1".into(),bind_port:remote_port,target_host:"127.0.0.1".into(),target_port,auto_start:false},
            ];
            let socket=TcpListener::bind(("127.0.0.1",0)).await?;let port=socket.local_addr()?.port();
            let config=Arc::new(server::Config {keys:vec![russh::keys::PrivateKey::random(&mut rand::rng(),russh::keys::Algorithm::Ed25519)?],auth_rejection_time:Duration::from_millis(5),..Default::default()});
            let fingerprint=config.keys[0].public_key().fingerprint(HashAlg::Sha256).to_string();
            let deny_remote=Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mut server=TestServer {sizes:Arc::new(Mutex::new(vec![])),remotes:Arc::new(Mutex::new(HashMap::new())),deny_remote:deny_remote.clone(),shells:Default::default()};
            let server_task=tokio::spawn(async move {server.run_on_socket(config,&socket).await});
            let dir=tempfile::tempdir()?;let mut storage=Storage::open(&dir.path().join("local.db"))?;
            storage.unlock(&dir.path().join("tunnels.kdbx"),zeroize::Zeroizing::new("test-master-password".into()),true)?;
            let profile=storage.save_profile(SaveProfile {id:None,name:"Forwarding test".into(),host:"127.0.0.1".into(),port,folder:"Tests".into(), startup_snippet: None, session_logging: false, tags: vec![],username:"tester".into(),password:Some("ssh-test-password".into()),forwards:rules.clone(),authentication:None})?.profiles.remove(0);
            storage.trust_key("127.0.0.1",port,&fingerprint)?;
            let store=Arc::new(Mutex::new(storage));let manager=Manager::default();let(events,mut rx)=channel();
            drop(local_reservation);drop(remote_reservation);
            manager.connect("tunnels".into(),profile.id,80,24,store.clone(),events).await?;
            loop {let e=rx.recv().await.context("No forwarding state")?;if e["type"]=="forward"&&e["ruleId"]==rules[0].id&&e["state"]=="active" {break;}}
            // A full binary payload and a TCP half-close must travel in both directions.
            async fn roundtrip(port:u16)->Result<()> {
                let mut tcp=TcpStream::connect(("127.0.0.1",port)).await?;
                let payload:Vec<u8>=(0..131072).map(|n|(n%256)as u8).collect();
                tcp.write_all(&payload).await?;tcp.shutdown().await?;
                let mut output=Vec::new();tcp.read_to_end(&mut output).await?;
                assert_eq!(output.len(),payload.len(),"Forwarded payload length on port {port}");
                assert!(output==payload,"Forwarded payload content mismatch on port {port}");Ok(())
            }
            roundtrip(local_port).await?;
            manager.forward("tunnels",rules[1].id.clone(),true).await?;
            roundtrip(remote_port).await?;
            let mut live=TcpStream::connect(("127.0.0.1",local_port)).await?;
            live.write_all(b"live").await?;let mut data=[0;4];live.read_exact(&mut data).await?;assert_eq!(&data,b"live");
            manager.forward("tunnels",rules[0].id.clone(),false).await?;
            assert!(TcpStream::connect(("127.0.0.1",local_port)).await.is_err());
            let mut byte=[0];assert!(!matches!(live.read(&mut byte).await,Ok(n)if n>0));
            // Stop/start reuses the listening port immediately; occupied ports report an error.
            let occupied=TcpListener::bind(("127.0.0.1",local_port)).await?;
            assert!(manager.forward("tunnels",rules[0].id.clone(),true).await.is_err());
            drop(occupied);
            manager.forward("tunnels",rules[0].id.clone(),true).await?;roundtrip(local_port).await?;
            manager.forward("tunnels",rules[1].id.clone(),false).await?;
            assert!(TcpStream::connect(("127.0.0.1",remote_port)).await.is_err());
            deny_remote.store(true,std::sync::atomic::Ordering::SeqCst);
            assert!(manager.forward("tunnels",rules[1].id.clone(),true).await.is_err());
            deny_remote.store(false,std::sync::atomic::Ordering::SeqCst);
            manager.forward("tunnels",rules[1].id.clone(),true).await?;
            let mut live_remote=TcpStream::connect(("127.0.0.1",remote_port)).await?;
            live_remote.write_all(b"ok").await?;let mut data=[0;2];live_remote.read_exact(&mut data).await?;assert_eq!(&data,b"ok");
            // Mirrors lock_vault: drop decrypted data, then cancel all session/tunnel tasks.
            store.lock().unwrap().lock();manager.close_all();
            while rx.recv().await.context("No session close event")?["type"]!="closed" {}
            assert!(TcpStream::connect(("127.0.0.1",local_port)).await.is_err());
            for _ in 0..40 {if TcpStream::connect(("127.0.0.1",remote_port)).await.is_err(){break;}tokio::time::sleep(Duration::from_millis(25)).await;}
            assert!(TcpStream::connect(("127.0.0.1",remote_port)).await.is_err());
            assert!(!matches!(live_remote.read(&mut byte).await,Ok(n)if n>0));
            assert!(manager.forward("tunnels",rules[0].id.clone(),true).await.is_err());
            server_task.abort();echo_task.abort();
            Ok::<_,anyhow::Error>(())
        }).await?
    }
}

pub enum Input {
    Data(Vec<u8>),
    Resize(u32, u32),
}
struct Session {
    input: mpsc::Sender<Input>,
    forwarding: mpsc::Sender<forwarding::Command>,
    cancel: CancellationToken,
}
#[derive(Clone, Default)]
pub struct Manager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    approvals: Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>,
}

pub(crate) struct Verify {
    id: String,
    host: String,
    port: u16,
    store: Arc<Mutex<Storage>>,
    manager: Manager,
    events: Channel<Event>,
    routes: Routes,
}
impl client::Handler for Verify {
    type Error = anyhow::Error;
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        address: &str,
        port: u32,
        _: &str,
        _: u32,
        reply: client::ChannelOpenHandle,
        _: &mut client::Session,
    ) -> Result<()> {
        self.routes
            .incoming(channel, address, port, reply, self.events.clone());
        Ok(())
    }
    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
        let known = self
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("Storage unavailable"))?
            .known_key(&self.host, self.port)?;
        if let Some(known) = known {
            if known != fingerprint {
                bail!("SSH server fingerprint changed. Expected {known}, received {fingerprint}. Connection blocked.");
            }
            return Ok(true);
        }
        let (tx, rx) = oneshot::channel();
        self.manager
            .approvals
            .lock()
            .unwrap()
            .insert(self.id.clone(), tx);
        self.events.send(Event::HostKey {
            host: self.host.clone(),
            port: self.port,
            fingerprint: fingerprint.clone(),
        })?;
        let accepted = tokio::time::timeout(Duration::from_secs(120), rx).await;
        self.manager.approvals.lock().unwrap().remove(&self.id);
        if !matches!(accepted, Ok(Ok(true))) {
            bail!("Server identity was not approved");
        }
        self.store
            .lock()
            .map_err(|_| anyhow::anyhow!("Storage unavailable"))?
            .trust_key(&self.host, self.port, &fingerprint)?;
        Ok(true)
    }
}

impl Manager {
    pub fn has_sessions(&self) -> bool {
        !self.sessions.lock().unwrap().is_empty()
    }
    pub fn approve(&self, id: &str, accepted: bool) -> Result<()> {
        self.approvals
            .lock()
            .unwrap()
            .remove(id)
            .context("Verification request expired")?
            .send(accepted)
            .map_err(|_| anyhow::anyhow!("Connection was cancelled"))
    }
    pub fn close(&self, id: &str) {
        if let Some(s) = self.sessions.lock().unwrap().remove(id) {
            s.cancel.cancel();
        }
        self.approvals.lock().unwrap().remove(id);
    }
    pub fn close_all(&self) {
        for (_, s) in self.sessions.lock().unwrap().drain() {
            s.cancel.cancel();
        }
        self.approvals.lock().unwrap().clear();
    }
    pub async fn input(&self, id: &str, input: Input) -> Result<()> {
        let tx = self
            .sessions
            .lock()
            .unwrap()
            .get(id)
            .context("Session is closed")?
            .input
            .clone();
        tx.send(input).await.context("Session is closed")
    }
    pub async fn forward(&self, id: &str, rule_id: String, start: bool) -> Result<()> {
        let tx = self
            .sessions
            .lock()
            .unwrap()
            .get(id)
            .context("Session is closed")?
            .forwarding
            .clone();
        let (reply, rx) = oneshot::channel();
        tx.send(if start {
            forwarding::Command::Start(rule_id, reply)
        } else {
            forwarding::Command::Stop(rule_id, reply)
        })
        .await
        .context("Session is closed")?;
        rx.await
            .context("Session closed while updating forwarding")?
            .map_err(anyhow::Error::msg)
    }
    #[cfg(test)]
    pub async fn connect(
        &self,
        id: String,
        profile_id: String,
        cols: u32,
        rows: u32,
        store: Arc<Mutex<Storage>>,
        events: Channel<Event>,
    ) -> Result<()> {
        self.connect_with_credentials(id, profile_id, (cols, rows), store, events, None)
            .await
    }
    pub async fn connect_with_credentials(
        &self,
        id: String,
        profile_id: String,
        size: (u32, u32),
        store: Arc<Mutex<Storage>>,
        events: Channel<Event>,
        credentials: Option<LoginCredentials>,
    ) -> Result<()> {
        let (cols, rows) = size;
        let override_login = credentials.map(|credentials| {
            (
                zeroize::Zeroizing::new(credentials.username),
                credentials.password.map(zeroize::Zeroizing::new),
            )
        });
        // Insert before reading credentials so lock_vault always sees and cancels this session.
        let (tx, mut input) = mpsc::channel::<Input>(128);
        let cancel = CancellationToken::new();
        let cancelled = cancel.clone();
        let (forwarding, forward_commands) = mpsc::channel(64);
        let routes = Routes::default();
        {
            let mut sessions = self.sessions.lock().unwrap();
            if sessions.contains_key(&id) {
                bail!("Session already exists");
            }
            sessions.insert(
                id.clone(),
                Session {
                    input: tx,
                    forwarding,
                    cancel,
                },
            );
        }
        let manager = self.clone();
        let (ready_tx, ready_rx) = oneshot::channel();
        tauri::async_runtime::spawn(async move {
            let credentials = (|| {
                let storage = store.lock().unwrap();
                let (profile, user, material) = storage.ssh_credentials(&profile_id)?;
                let logging = if profile.session_logging {
                    Some(storage.log_access()?.context(
                        "Session logging key is missing from the vault; save the host again",
                    )?)
                } else {
                    None
                };
                let startup = storage.startup_command(&profile)?;
                Ok::<_, anyhow::Error>((profile, user, material, startup, logging))
            })();
            let init = async {
                let (profile, mut username, mut material, startup, logging) = credentials?;
                if let Some((user, password)) = override_login {
                    anyhow::ensure!(
                        !user.trim().is_empty()
                            && user.len() <= 256
                            && !user.chars().any(char::is_control),
                        "Enter a valid SSH username (up to 256 bytes)"
                    );
                    username = user;
                    if profile.auth_method == auth::Method::Password {
                        material = auth::Material::Password(
                            password.context("Enter a password for this retry")?,
                        );
                    } else {
                        anyhow::ensure!(
                            password.is_none(),
                            "Password overrides apply only to password authentication"
                        );
                    }
                }
                let verify = Verify {
                    id: id.clone(),
                    host: profile.host.clone(),
                    port: profile.port,
                    store: store.clone(),
                    manager: manager.clone(),
                    events: events.clone(),
                    routes: routes.clone(),
                };
                let config = client::Config {
                    keepalive_interval: Some(Duration::from_secs(30)),
                    keepalive_max: 3,
                    ..Default::default()
                };
                let mut handle = tokio::time::timeout(
                    Duration::from_secs(150),
                    client::connect(
                        Arc::new(config),
                        (profile.host.as_str(), profile.port),
                        verify,
                    ),
                )
                .await
                .context("SSH connection timed out")??;
                let result = tokio::time::timeout(Duration::from_secs(30), async {
                    match material {
                        auth::Material::Password(password) => Ok(handle
                            .authenticate_password(username.as_str(), password.as_str())
                            .await?),
                        auth::Material::Key {
                            private_key,
                            passphrase,
                            certificate,
                        } => {
                            let (key, cert, _) = tokio::task::spawn_blocking(move || {
                                auth::parse_key(
                                    &private_key,
                                    &passphrase,
                                    certificate.as_deref(),
                                    true,
                                )
                            })
                            .await??;
                            if let Some(cert) = cert {
                                Ok(handle
                                    .authenticate_openssh_cert(username.as_str(), key, cert)
                                    .await?)
                            } else {
                                let hash = handle.best_supported_rsa_hash().await?.flatten();
                                Ok(handle
                                    .authenticate_publickey(
                                        username.as_str(),
                                        russh::keys::PrivateKeyWithHashAlg::new(key, hash),
                                    )
                                    .await?)
                            }
                        }
                    }
                })
                .await
                .context("Authentication timed out")?
                    as Result<russh::client::AuthResult>;
                let result = result?;
                if !result.success() {
                    let _ = events.send(Event::AuthenticationFailed {
                        username: username.to_string(),
                        method: profile.auth_method,
                    });
                    bail!("SSH authentication failed. Check the username, saved credentials, and the server's authorized keys / trusted certificate authority.");
                }
                drop(username);
                let mut recorder = logging
                    .map(|access| {
                        crate::session_logs::Writer::start(
                            access,
                            &profile.name,
                            &profile.host,
                            profile.port,
                        )
                    })
                    .transpose()
                    .context("Could not start encrypted session logging")?;
                let channel = handle.channel_open_session().await?;
                channel
                    .request_pty(
                        true,
                        "xterm-256color",
                        cols.clamp(2, 500),
                        rows.clamp(1, 300),
                        0,
                        0,
                        &[],
                    )
                    .await?;
                // Drain protocol acknowledgments before reporting a working terminal.
                let mut channel = channel;
                loop {
                    match channel.wait().await {
                        Some(ChannelMsg::Success) => break,
                        Some(ChannelMsg::Failure) | None => bail!("Server refused a terminal"),
                        Some(ChannelMsg::Data { data })
                        | Some(ChannelMsg::ExtendedData { data, .. }) => {
                            if let Some(log) = recorder.as_mut() {
                                log.output(&data)
                                    .context("Encrypted session logging failed")?;
                            }
                            events.send(Event::Data {
                                bytes: data.to_vec(),
                            })?;
                        }
                        _ => {}
                    }
                }
                channel.request_shell(true).await?;
                loop {
                    match channel.wait().await {
                        Some(ChannelMsg::Success) => break,
                        Some(ChannelMsg::Failure) | None => {
                            bail!("Server refused an interactive shell")
                        }
                        Some(ChannelMsg::Data { data })
                        | Some(ChannelMsg::ExtendedData { data, .. }) => {
                            if let Some(log) = recorder.as_mut() {
                                log.output(&data)
                                    .context("Encrypted session logging failed")?;
                            }
                            events.send(Event::Data {
                                bytes: data.to_vec(),
                            })?;
                        }
                        _ => {}
                    }
                }
                if let Some(command) = startup {
                    channel
                        .data(command.as_bytes())
                        .await
                        .context("Could not send startup snippet")?;
                }
                Ok::<_, anyhow::Error>((handle, channel, profile.forwards, recorder))
            };
            let result = tokio::select! {
                biased;
                _ = cancelled.cancelled() => Err(anyhow::anyhow!("Disconnected")),
                result = tokio::time::timeout(Duration::from_secs(190), init) => result.context("SSH setup timed out").and_then(|r| r),
            };
            match result {
                Err(error) => {
                    let message = format!("{error:#}");
                    let _ = events.send(Event::Closed {
                        message: message.clone(),
                    });
                    let _ = ready_tx.send(Err(message));
                }
                Ok((handle, channel, rules, mut recorder)) => {
                    let handle = Arc::new(handle);
                    let tunnels = Tunnels::new(
                        handle.clone(),
                        rules,
                        routes,
                        cancelled.clone(),
                        events.clone(),
                    );
                    let tunnel_task = tokio::spawn(tunnels.run(forward_commands));
                    let _ = events.send(Event::Connected);
                    let _ = ready_tx.send(Ok(()));
                    let (mut reader, writer) = channel.split();
                    let transfer = async {
                        loop {
                            tokio::select! {
                                biased;
                                command = input.recv() => match command {
                                    Some(Input::Data(bytes)) => writer.data_bytes(bytes).await?,
                                    Some(Input::Resize(cols,rows)) => writer.window_change(cols.clamp(2,500),rows.clamp(1,300),0,0).await?,
                                    None => break,
                                },
                                message = reader.wait() => match message {
                                    Some(ChannelMsg::Data {data}) | Some(ChannelMsg::ExtendedData {data,..}) => { if let Some(log) = recorder.as_mut() {log.output(&data).context("Encrypted session logging failed")?;} events.send(Event::Data {bytes:data.to_vec()})?; },
                                    Some(ChannelMsg::Close) | None => break,
                                    _ => {}
                                }
                            }
                        }
                        Ok(())
                    };
                    // Cancellation also interrupts a blocked write, not just an idle reader.
                    let outcome: Result<()> = tokio::select! {
                        biased;
                        _ = cancelled.cancelled() => Ok(()),
                        result = transfer => result,
                    };
                    cancelled.cancel();
                    let _ = tunnel_task.await;
                    let _ = tokio::time::timeout(
                        Duration::from_secs(2),
                        handle.disconnect(
                            Disconnect::ByApplication,
                            "Harbor closed the session",
                            "en",
                        ),
                    )
                    .await;
                    let mut message = outcome
                        .err()
                        .map(|e| format!("{e:#}"))
                        .unwrap_or_else(|| "Session closed".into());
                    if let Some(log) = recorder.as_mut() {
                        if let Err(e) = log.finish(&message) {
                            message =
                                format!("{message}; encrypted log could not be finalized: {e}");
                        }
                    }
                    let _ = events.send(Event::Closed { message });
                }
            }
            manager.sessions.lock().unwrap().remove(&id);
            manager.approvals.lock().unwrap().remove(&id);
        });
        ready_rx
            .await
            .context("Connection task stopped")?
            .map_err(anyhow::Error::msg)
    }
}
