use crate::ssh::{Event, Verify};
use anyhow::{bail, Context, Result};
use russh::{client, ChannelOpenFailure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::ipc::Channel;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot, Semaphore},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub direction: Direction,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub auto_start: bool,
}

fn bind_address(address: &str) -> String {
    if address == "localhost" {
        "127.0.0.1".into()
    } else {
        address.into()
    }
}

pub fn validate(rules: &[Rule]) -> Result<()> {
    if rules.len() > 32 {
        bail!("A connection can have at most 32 forwarding rules");
    }
    let mut ids = HashSet::new();
    let mut binds = HashSet::new();
    for r in rules {
        uuid::Uuid::parse_str(&r.id).context("Invalid forwarding rule identifier")?;
        if !ids.insert(&r.id) {
            bail!("Duplicate forwarding rule identifier");
        }
        let address = bind_address(&r.bind_address);
        address
            .parse::<IpAddr>()
            .context("Listen address must be an IP address or localhost")?;
        if r.bind_port == 0 || r.target_port == 0 {
            bail!("Forwarding ports must be between 1 and 65535");
        }
        if !binds.insert((r.direction, address, r.bind_port)) {
            bail!("Two rules cannot listen on the same address and port in the same direction");
        }
        if r.target_host.is_empty()
            || r.target_host.len() > 253
            || r.target_host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
            || r.target_host.contains('/')
        {
            bail!("Enter a forwarding destination hostname or IP without a protocol or path");
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Route {
    rule: Rule,
    cancel: CancellationToken,
    slots: Arc<Semaphore>,
}
#[derive(Clone, Default)]
pub struct Routes(Arc<Mutex<HashMap<String, Route>>>);

impl Routes {
    // Reject unrequested forwarded channels. Only explicit active rules can reach local TCP.
    pub fn incoming(
        &self,
        channel: russh::Channel<client::Msg>,
        address: &str,
        port: u32,
        reply: client::ChannelOpenHandle,
        events: Channel<Event>,
    ) {
        let route = self
            .0
            .lock()
            .unwrap()
            .values()
            .find(|route| {
                let bind = bind_address(&route.rule.bind_address);
                let wildcard = match (bind.parse::<IpAddr>(), address.parse::<IpAddr>()) {
                    (Ok(IpAddr::V4(b)), Ok(IpAddr::V4(_))) => b.is_unspecified(),
                    (Ok(IpAddr::V6(b)), Ok(IpAddr::V6(_))) => b.is_unspecified(),
                    _ => false,
                };
                u32::from(route.rule.bind_port) == port
                    && (bind == bind_address(address) || wildcard)
                    && !route.cancel.is_cancelled()
            })
            .cloned();
        let Some(route) = route else {
            return;
        }; // Dropping reply rejects the open request.
        let Ok(permit) = route.slots.clone().try_acquire_owned() else {
            return;
        };
        tokio::spawn(async move {
            let _permit = permit;
            let transfer = async {
                match tokio::time::timeout(
                    Duration::from_secs(10),
                    TcpStream::connect((route.rule.target_host.as_str(), route.rule.target_port)),
                )
                .await
                {
                    Ok(Ok(mut tcp)) => {
                        reply.accept().await;
                        let mut stream = channel.into_stream();
                        if let Err(error) =
                            tokio::io::copy_bidirectional(&mut tcp, &mut stream).await
                        {
                            report(
                                &events,
                                &route.rule,
                                "active",
                                Some(format!("A forwarded connection closed: {error}")),
                            );
                        }
                    }
                    result => {
                        let detail = match result {
                            Ok(Err(e)) => e.to_string(),
                            _ => "connection timed out".into(),
                        };
                        report(
                            &events,
                            &route.rule,
                            "active",
                            Some(format!("Destination unavailable: {detail}")),
                        );
                        reply.reject(ChannelOpenFailure::ConnectFailed).await;
                    }
                }
            };
            tokio::select! { biased; _=route.cancel.cancelled()=>{}, _=transfer=>{} }
        });
    }
}

pub enum Command {
    Start(String, oneshot::Sender<Result<(), String>>),
    Stop(String, oneshot::Sender<Result<(), String>>),
}
struct Running {
    cancel: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub struct Tunnels {
    handle: Arc<client::Handle<Verify>>,
    rules: Vec<Rule>,
    active: HashMap<String, Running>,
    routes: Routes,
    cancel: CancellationToken,
    events: Channel<Event>,
}
impl Drop for Tunnels {
    fn drop(&mut self) {
        self.active.clear();
        self.routes.0.lock().unwrap().clear();
    }
}
fn report(events: &Channel<Event>, rule: &Rule, state: &str, message: Option<String>) {
    let _ = events.send(Event::Forward {
        rule_id: rule.id.clone(),
        state: state.into(),
        message,
    });
}

impl Tunnels {
    pub fn new(
        handle: Arc<client::Handle<Verify>>,
        rules: Vec<Rule>,
        routes: Routes,
        cancel: CancellationToken,
        events: Channel<Event>,
    ) -> Self {
        Self {
            handle,
            rules,
            active: HashMap::new(),
            routes,
            cancel,
            events,
        }
    }
    pub async fn run(mut self, mut commands: mpsc::Receiver<Command>) {
        let lifetime = self.cancel.clone();
        let work = async {
            for rule in self.rules.clone() {
                if rule.auto_start {
                    let _ = self.start(&rule.id).await;
                }
            }
            while let Some(command) = commands.recv().await {
                match command {
                    Command::Start(id, reply) => {
                        let _ = reply.send(self.start(&id).await.map_err(|e| format!("{e:#}")));
                    }
                    Command::Stop(id, reply) => {
                        let _ = reply.send(self.stop(&id).await.map_err(|e| format!("{e:#}")));
                    }
                }
            }
        };
        tokio::select! { biased; _=lifetime.cancelled()=>{}, _=work=>{} }
        for (_, mut running) in self.active.drain() {
            running.cancel.cancel();
            if let Some(task) = running.task.take() {
                let _ = task.await;
            }
        }
        self.routes.0.lock().unwrap().clear();
    }

    async fn start(&mut self, id: &str) -> Result<()> {
        let rule = self
            .rules
            .iter()
            .find(|r| r.id == id)
            .context("Forwarding rule not found; reconnect after editing rules")?
            .clone();
        if self
            .active
            .get(id)
            .is_some_and(|r| !r.cancel.is_cancelled())
        {
            return Ok(());
        }
        self.active.remove(id);
        report(&self.events, &rule, "starting", None);
        let cancel = self.cancel.child_token();
        let mut listener_task = None;
        let result:Result<()>=async {
            match rule.direction {
                Direction::Local => {
                    let listener=TcpListener::bind((bind_address(&rule.bind_address).as_str(),rule.bind_port)).await.context("Could not open the local listening port")?;
                    let handle=self.handle.clone(); let token=cancel.clone(); let rule=rule.clone(); let events=self.events.clone();
                    listener_task=Some(tokio::spawn(async move {
                        let mut transfers=JoinSet::new();
                        let slots=Arc::new(Semaphore::new(128));
                        loop {
                            tokio::select! {
                                biased;
                                _=token.cancelled()=>break,
                                Some(_)=transfers.join_next(),if !transfers.is_empty()=>{},
                                incoming=listener.accept()=>{
                                    let (mut tcp,peer)=match incoming {
                                        Ok(pair)=>pair,
                                        Err(e)=>{ report(&events,&rule,"error",Some(e.to_string())); token.cancel(); break; }
                                    };
                                    let Ok(permit)=slots.clone().try_acquire_owned() else { continue; };
                                    let handle=handle.clone(); let rule=rule.clone(); let events=events.clone();
                                    transfers.spawn(async move {
                                        let _permit=permit;
                                        let open=tokio::time::timeout(Duration::from_secs(10),handle.channel_open_direct_tcpip(rule.target_host.clone(),u32::from(rule.target_port),peer.ip().to_string(),u32::from(peer.port()))).await;
                                        let result:Result<()>=async {
                                            let channel=open.context("Forward destination request timed out")??;
                                            let mut stream=channel.into_stream();
                                            tokio::io::copy_bidirectional(&mut tcp,&mut stream).await?;
                                            Ok(())
                                        }.await;
                                        if let Err(e)=result { report(&events,&rule,"active",Some(format!("Destination connection failed: {e:#}"))); }
                                    });
                                }
                            }
                        }
                        // Finish cancellation before reporting the listener stopped.
                        transfers.shutdown().await;
                    }));
                },
                Direction::Remote => {
                    self.routes.0.lock().unwrap().insert(id.into(),Route {rule:rule.clone(),cancel:cancel.clone(),slots:Arc::new(Semaphore::new(128))});
                    match tokio::time::timeout(Duration::from_secs(10),self.handle.tcpip_forward(bind_address(&rule.bind_address),u32::from(rule.bind_port))).await {
                        Ok(result)=>{ result.context("SSH server refused remote forwarding (check AllowTcpForwarding, GatewayPorts, and port availability)")?; },
                        Err(_)=>{ self.cancel.cancel(); bail!("Remote listen request timed out; closing SSH to release any server-side listener"); }
                    }
                }
            }
            Ok(())
        }.await;
        match result {
            Ok(()) => {
                self.active.insert(
                    id.into(),
                    Running {
                        cancel,
                        task: listener_task,
                    },
                );
                report(&self.events, &rule, "active", None);
                Ok(())
            }
            Err(error) => {
                cancel.cancel();
                self.routes.0.lock().unwrap().remove(id);
                report(&self.events, &rule, "error", Some(format!("{error:#}")));
                Err(error)
            }
        }
    }

    async fn stop(&mut self, id: &str) -> Result<()> {
        let rule = self
            .rules
            .iter()
            .find(|r| r.id == id)
            .context("Forwarding rule not found")?
            .clone();
        if !self.active.contains_key(id) {
            report(&self.events, &rule, "inactive", None);
            return Ok(());
        }
        if rule.direction == Direction::Remote {
            match tokio::time::timeout(
                Duration::from_secs(10),
                self.handle.cancel_tcpip_forward(
                    bind_address(&rule.bind_address),
                    u32::from(rule.bind_port),
                ),
            )
            .await
            {
                Ok(Ok(())) => {}
                result => {
                    let message = if result.is_err() {
                        self.cancel.cancel();
                        "Remote stop timed out; closing the SSH session"
                    } else {
                        "SSH server refused to stop the remote listener; close the SSH session to release it"
                    };
                    report(&self.events, &rule, "active", Some(message.into()));
                    bail!("{message}");
                }
            }
        }
        self.routes.0.lock().unwrap().remove(id);
        if let Some(mut running) = self.active.remove(id) {
            running.cancel.cancel();
            if let Some(task) = running.task.take() {
                let _ = task.await;
            }
        }
        report(&self.events, &rule, "inactive", None);
        Ok(())
    }
}
