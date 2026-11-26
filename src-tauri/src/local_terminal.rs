use crate::ssh::Event;
use anyhow::{anyhow, bail, Context, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::ipc::Channel;

#[derive(Clone, Default)]
pub struct Manager(Arc<Mutex<HashMap<String, Arc<Slot>>>>);

#[derive(Default)]
pub struct Slot {
    cancelled: AtomicBool,
    session: Mutex<Option<Arc<Session>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    use tauri::ipc::InvokeResponseBody;

    fn channel() -> (Channel<Event>, mpsc::Receiver<serde_json::Value>) {
        let (tx, rx) = mpsc::channel();
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

    fn test_command(cwd: &std::path::Path) -> CommandBuilder {
        #[cfg(windows)]
        let mut cmd = {
            let mut cmd = CommandBuilder::new("cmd.exe");
            cmd.args(["/D", "/Q"]);
            cmd
        };
        #[cfg(not(windows))]
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.cwd(cwd);
        cmd
    }

    fn until_closed(rx: &mpsc::Receiver<serde_json::Value>, manager: &Manager, id: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut output = Vec::new();
        loop {
            let event = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|_| {
                    manager.close(id);
                    panic!(
                        "local shell did not close: {:?}",
                        String::from_utf8_lossy(&output)
                    )
                });
            if event["type"] == "data" {
                output.extend(
                    event["bytes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|b| b.as_u64().unwrap() as u8),
                );
            }
            if output.ends_with(b"\x1b[6n") {
                let _ = manager.write(id, b"\x1b[1;1R");
            }
            if event["type"] == "closed" {
                break;
            }
        }
        String::from_utf8_lossy(&output).into_owned()
    }

    #[test]
    fn shell_executes_input_resizes_and_drains_before_exit() {
        let manager = Manager::default();
        let slot = manager.reserve("test").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (events, rx) = channel();
        manager
            .start_command(
                "test".into(),
                slot,
                80,
                24,
                events,
                test_command(dir.path()),
            )
            .unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap()["type"],
            "connected"
        );
        manager.resize("test", 100, 31).unwrap();
        assert_eq!(
            manager
                .session("test")
                .unwrap()
                .master
                .lock()
                .unwrap()
                .get_size()
                .unwrap()
                .rows,
            31
        );
        #[cfg(windows)]
        let input = b"set HARBOR_TEST=HARBOR_\recho %HARBOR_TEST%LOCAL_OK\rexit\r";
        #[cfg(not(windows))]
        let input = b"printf 'HARBOR_%s\\n' 'LOCAL_OK'\nexit\n";
        manager.write("test", input).unwrap();
        let output = until_closed(&rx, &manager, "test");
        assert!(output.contains("HARBOR_LOCAL_OK"), "{output:?}");
        assert!(manager.0.lock().unwrap().is_empty());
    }

    #[test]
    fn closing_pending_or_running_shell_cancels_it() {
        let manager = Manager::default();
        let pending = manager.reserve("pending").unwrap();
        manager.close("pending");
        let dir = tempfile::tempdir().unwrap();
        let (events, _) = channel();
        assert!(manager
            .start_command(
                "pending".into(),
                pending,
                80,
                24,
                events,
                test_command(dir.path())
            )
            .is_err());
        let slot = manager.reserve("running").unwrap();
        let (events, rx) = channel();
        manager
            .start_command(
                "running".into(),
                slot,
                80,
                24,
                events,
                test_command(dir.path()),
            )
            .unwrap();
        manager.close_all();
        until_closed(&rx, &manager, "running");
        assert!(manager.0.lock().unwrap().is_empty());
    }
}

struct Session {
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.killer.get_mut().unwrap().kill();
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellInfo {
    pub name: String,
    pub program: PathBuf,
    pub cwd: PathBuf,
}

pub fn shell_info() -> ShellInfo {
    #[cfg(windows)]
    let program = std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|p| p.join("pwsh.exe"))
                .find(|p| p.is_file())
        })
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
                .join("System32/WindowsPowerShell/v1.0/powershell.exe")
        });
    #[cfg(not(windows))]
    let program = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_file())
        .unwrap_or_else(|| "/bin/sh".into());
    let name = program
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let cwd = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
    ShellInfo { name, program, cwd }
}

fn size(cols: u32, rows: u32) -> PtySize {
    PtySize {
        cols: cols.clamp(2, 500) as u16,
        rows: rows.clamp(1, 300) as u16,
        pixel_width: 0,
        pixel_height: 0,
    }
}

impl Manager {
    // Reserve before starting blocking setup so closing a pending tab cancels its shell.
    pub fn reserve(&self, id: &str) -> Result<Arc<Slot>> {
        let mut sessions = self.0.lock().unwrap();
        if sessions.contains_key(id) {
            bail!("Local terminal already exists");
        }
        let slot = Arc::new(Slot::default());
        sessions.insert(id.into(), slot.clone());
        Ok(slot)
    }

    pub fn start(
        &self,
        id: String,
        slot: Arc<Slot>,
        cols: u32,
        rows: u32,
        events: Channel<Event>,
    ) -> Result<()> {
        let info = shell_info();
        let mut cmd = CommandBuilder::new(&info.program);
        cmd.cwd(&info.cwd);
        cmd.arg(if cfg!(windows) { "-NoLogo" } else { "-l" });
        self.start_command(id, slot, cols, rows, events, cmd)
    }

    fn start_command(
        &self,
        id: String,
        slot: Arc<Slot>,
        cols: u32,
        rows: u32,
        events: Channel<Event>,
        mut cmd: CommandBuilder,
    ) -> Result<()> {
        let result = (|| {
            if slot.cancelled.load(Ordering::SeqCst) {
                bail!("Local terminal start cancelled");
            }
            cmd.env("TERM", "xterm-256color");
            cmd.env("COLORTERM", "truecolor");
            let pair = native_pty_system()
                .openpty(size(cols, rows))
                .context("Could not open a local terminal")?;
            let mut reader = pair.master.try_clone_reader()?;
            let writer = pair.master.take_writer()?;
            let mut child = pair
                .slave
                .spawn_command(cmd)
                .context("Could not start the local shell")?;
            drop(pair.slave);
            let session = Arc::new(Session {
                killer: Mutex::new(child.clone_killer()),
                master: Mutex::new(pair.master),
                writer: Mutex::new(writer),
            });
            let (drained_tx, drained_rx) = std::sync::mpsc::channel();
            let reader_events = events.clone();
            let reader_manager = self.clone();
            let reader_id = id.clone();
            // Drain output even when cancellation races setup; ConPTY close may wait for it.
            let accepted = {
                let mut target = slot.session.lock().unwrap();
                if slot.cancelled.load(Ordering::SeqCst) {
                    false
                } else {
                    *target = Some(session.clone());
                    events.send(Event::Connected).is_ok()
                }
            };
            std::thread::spawn(move || {
                let mut bytes = [0u8; 16384];
                loop {
                    match reader.read(&mut bytes) {
                        Ok(0) => break,
                        Ok(n) => {
                            if accepted
                                && reader_events
                                    .send(Event::Data {
                                        bytes: bytes[..n].to_vec(),
                                    })
                                    .is_err()
                            {
                                reader_manager.close(&reader_id);
                                break;
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                let _ = drained_tx.send(());
            });
            if !accepted {
                let _ = session.killer.lock().unwrap().kill();
            }
            drop(session);
            let manager = self.clone();
            let finished_slot = slot.clone();
            let finished_id = id.clone();
            std::thread::spawn(move || {
                let result = child.wait();
                // Drop the PTY master to finish output on Windows before reporting closure.
                manager.finish(&finished_id, &finished_slot);
                let _ = drained_rx.recv_timeout(std::time::Duration::from_secs(5));
                let message = match result {
                    Ok(status) => format!("Local shell exited ({})", status.exit_code()),
                    Err(e) => format!("Local shell ended: {e}"),
                };
                let _ = events.send(Event::Closed { message });
            });
            if !accepted {
                bail!("Local terminal start cancelled");
            }
            Ok(())
        })();
        if result.is_err() {
            self.finish(&id, &slot);
        }
        result
    }

    fn session(&self, id: &str) -> Result<Arc<Session>> {
        let slot = self
            .0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("Local terminal is closed"))?;
        let session = slot
            .session
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow!("Local terminal is starting"))?;
        Ok(session)
    }

    pub fn write(&self, id: &str, data: &[u8]) -> Result<()> {
        if data.len() > 65536 {
            bail!("Input chunk too large");
        }
        let session = self.session(id)?;
        let mut writer = session.writer.lock().unwrap();
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u32, rows: u32) -> Result<()> {
        self.session(id)?
            .master
            .lock()
            .unwrap()
            .resize(size(cols, rows))?;
        Ok(())
    }

    fn finish(&self, id: &str, expected: &Arc<Slot>) {
        let removed = {
            let mut sessions = self.0.lock().unwrap();
            if sessions.get(id).is_some_and(|s| Arc::ptr_eq(s, expected)) {
                sessions.remove(id)
            } else {
                None
            }
        };
        if let Some(slot) = removed {
            slot.cancelled.store(true, Ordering::SeqCst);
            let session = slot.session.lock().unwrap().take();
            drop(session);
        }
    }

    pub fn close(&self, id: &str) {
        let slot = self.0.lock().unwrap().remove(id);
        if let Some(slot) = slot {
            slot.cancelled.store(true, Ordering::SeqCst);
            let session = slot.session.lock().unwrap().take();
            // A blocked paste must not prevent cancellation or block the UI thread.
            std::thread::spawn(move || {
                if let Some(session) = session {
                    let _ = session.killer.lock().unwrap().kill();
                }
            });
        }
    }

    pub fn close_all(&self) {
        let ids: Vec<_> = self.0.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.close(&id);
        }
    }
}
