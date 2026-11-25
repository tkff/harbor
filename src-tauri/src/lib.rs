mod auth;
mod forwarding;
mod hello;
mod ssh;
mod storage;

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use storage::{SaveProfile, Status, Storage};
use tauri::{ipc::Channel, Manager, State};
use zeroize::Zeroizing;

struct AppState {
    store: Arc<Mutex<Storage>>,
    ssh: ssh::Manager,
    vault_epoch: Arc<AtomicU64>,
}
fn message(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
fn status(state: State<AppState>) -> Result<Status, String> {
    state.store.lock().unwrap().status().map_err(message)
}

#[tauri::command]
async fn unlock_vault(
    state: State<'_, AppState>,
    path: String,
    password: String,
    create: bool,
    key_file_path: Option<String>,
) -> Result<Status, String> {
    let store = state.store.clone();
    let password = Zeroizing::new(password);
    let epoch = state.vault_epoch.clone();
    let expected = epoch.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        let key_file = key_file_path
            .filter(|p| !p.is_empty())
            .map(|p| storage::read_key_file(std::path::Path::new(&p)))
            .transpose()
            .map_err(message)?;
        let mut store = store.lock().unwrap();
        if epoch.load(Ordering::SeqCst) != expected {
            return Err("Vault operation was cancelled".into());
        }
        store
            .unlock_with_keyfile(
                std::path::Path::new(&path),
                password,
                create,
                key_file.as_deref().map(|b| b.as_slice()),
            )
            .map_err(message)
    })
    .await
    .map_err(message)?
}

#[tauri::command]
fn lock_vault(state: State<AppState>) {
    state.vault_epoch.fetch_add(1, Ordering::SeqCst);
    // Lock while holding storage, then cancel sessions. New connects cannot extract credentials.
    let mut store = state.store.lock().unwrap();
    store.lock();
    state.ssh.close_all();
}

#[tauri::command]
async fn save_connection(
    state: State<'_, AppState>,
    profile: SaveProfile,
) -> Result<Status, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store.lock().unwrap().save_profile(profile).map_err(message)
    })
    .await
    .map_err(message)?
}

#[tauri::command]
fn list_snippets(state: State<AppState>) -> Result<Vec<storage::Snippet>, String> {
    state.store.lock().unwrap().snippets().map_err(message)
}

#[tauri::command]
async fn save_snippet(
    state: State<'_, AppState>,
    snippet: storage::SaveSnippet,
) -> Result<Vec<storage::Snippet>, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store.lock().unwrap().save_snippet(snippet).map_err(message)
    })
    .await
    .map_err(message)?
}

#[tauri::command]
async fn delete_snippet(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<storage::Snippet>, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store.lock().unwrap().delete_snippet(&id).map_err(message)
    })
    .await
    .map_err(message)?
}

#[tauri::command]
async fn hello_status(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<hello::Status, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (supported, reason) = match hello::available() {
            Ok(true) => (true, None),
            Ok(false) => (
                false,
                Some(
                    "Windows Hello is not configured or supported for this account / platform."
                        .into(),
                ),
            ),
            Err(_) => (
                false,
                Some("Windows Hello is unavailable in this Windows session.".into()),
            ),
        };
        let path = path.or(store.lock().unwrap().status().map_err(message)?.vault_path);
        let canonical = path
            .and_then(|p| std::path::Path::new(&p).canonicalize().ok())
            .map(|p| p.to_string_lossy().into_owned());
        let enabled = canonical
            .map(|p| store.lock().unwrap().hello_record(&p).map(|e| e.is_some()))
            .transpose()
            .map_err(message)?
            .unwrap_or(false);
        Ok(hello::Status {
            supported,
            enabled,
            reason,
        })
    })
    .await
    .map_err(message)?
}

#[tauri::command]
async fn hello_enroll(
    state: State<'_, AppState>,
    path: String,
    password: String,
    recovery_path: String,
) -> Result<Status, String> {
    let store = state.store.clone();
    let epoch = state.vault_epoch.clone();
    let expected = epoch.load(Ordering::SeqCst);
    let password = Zeroizing::new(password);
    tauri::async_runtime::spawn_blocking(move || -> Result<Status, String> {
        let (canonical, key_file, rekey) = store
            .lock()
            .unwrap()
            .hello_material(
                std::path::Path::new(&path),
                &password,
                std::path::Path::new(&recovery_path),
            )
            .map_err(message)?;
        let envelope = hello::enroll(&canonical, &key_file).map_err(message)?;
        let result = (|| -> anyhow::Result<Status> {
            let mut store = store.lock().unwrap();
            anyhow::ensure!(
                epoch.load(Ordering::SeqCst) == expected,
                "Vault was locked during Hello setup; start again"
            );
            if rekey {
                storage::save_recovery(
                    std::path::Path::new(&recovery_path),
                    std::path::Path::new(&canonical),
                    &key_file,
                )?;
            }
            store.commit_hello(&canonical, &password, &key_file, &envelope, rekey)
        })();
        if result.is_err() {
            let _ = hello::delete(&envelope);
        }
        result.map_err(message)
    })
    .await
    .map_err(message)?
}

#[tauri::command]
async fn hello_unlock(
    state: State<'_, AppState>,
    path: String,
    password: String,
) -> Result<Status, String> {
    let store = state.store.clone();
    let epoch = state.vault_epoch.clone();
    let expected = epoch.fetch_add(1, Ordering::SeqCst) + 1;
    let password = Zeroizing::new(password);
    tauri::async_runtime::spawn_blocking(move || -> Result<Status, String> {
        let canonical = std::path::Path::new(&path)
            .canonicalize()
            .map_err(message)?
            .to_string_lossy()
            .into_owned();
        let envelope = store
            .lock()
            .unwrap()
            .hello_record(&canonical)
            .map_err(message)?
            .ok_or("Windows Hello has not been registered for this vault on this PC")?;
        let key_file = hello::unlock(&canonical, &envelope).map_err(message)?;
        let mut store = store.lock().unwrap();
        if epoch.load(Ordering::SeqCst) != expected {
            return Err("Vault operation was cancelled".into());
        }
        store
            .unlock_with_keyfile(
                std::path::Path::new(&canonical),
                password,
                false,
                Some(&key_file),
            )
            .map_err(message)
    })
    .await
    .map_err(message)?
}

#[tauri::command]
async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<Status, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store.lock().unwrap().delete_profile(&id).map_err(message)
    })
    .await
    .map_err(message)?
}

#[tauri::command]
fn connection_username(state: State<AppState>, id: String) -> Result<String, String> {
    state.store.lock().unwrap().username(&id).map_err(message)
}

#[tauri::command]
async fn ssh_connect(
    state: State<'_, AppState>,
    session_id: String,
    profile_id: String,
    cols: u32,
    rows: u32,
    events: Channel<ssh::Event>,
) -> Result<(), String> {
    state
        .ssh
        .connect(
            session_id,
            profile_id,
            cols,
            rows,
            state.store.clone(),
            events,
        )
        .await
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn ssh_write(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    if data.len() > 65536 {
        return Err("Input chunk too large".into());
    }
    state
        .ssh
        .input(&session_id, ssh::Input::Data(data))
        .await
        .map_err(message)
}
#[tauri::command]
async fn ssh_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    state
        .ssh
        .input(&session_id, ssh::Input::Resize(cols, rows))
        .await
        .map_err(message)
}
#[tauri::command]
fn ssh_close(state: State<AppState>, session_id: String) {
    state.ssh.close(&session_id);
}
#[tauri::command]
async fn ssh_forward(
    state: State<'_, AppState>,
    session_id: String,
    rule_id: String,
    start: bool,
) -> Result<(), String> {
    state
        .ssh
        .forward(&session_id, rule_id, start)
        .await
        .map_err(message)
}
#[tauri::command]
fn ssh_approve(state: State<AppState>, session_id: String, accepted: bool) -> Result<(), String> {
    state.ssh.approve(&session_id, accepted).map_err(message)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(AppState {
                store: Arc::new(Mutex::new(Storage::open(&dir.join("harbor.db"))?)),
                ssh: ssh::Manager::default(),
                vault_epoch: Arc::new(AtomicU64::new(0)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            status,
            unlock_vault,
            lock_vault,
            save_connection,
            list_snippets,
            save_snippet,
            delete_snippet,
            delete_connection,
            connection_username,
            ssh_connect,
            ssh_write,
            ssh_resize,
            ssh_close,
            ssh_forward,
            ssh_approve,
            hello_status,
            hello_enroll,
            hello_unlock
        ])
        .run(tauri::generate_context!())
        .expect("Could not start Harbor");
}
