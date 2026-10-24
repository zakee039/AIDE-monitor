//! Bounded local lifecycle evidence. Never log settings, account data, URLs,
//! panic payloads, or command-line arguments. UI callbacks only enqueue records.
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::{webview::PageLoadEvent, Webview, Wry};

enum Message {
    Event(Value),
    Urgent(Value, mpsc::Sender<()>),
    Exit(mpsc::Sender<()>),
}
static WRITER: OnceLock<mpsc::SyncSender<Message>> = OnceLock::new();
static WATCHED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

fn entry(event: &str, data: Value) -> Value {
    json!({"at":chrono::Utc::now().to_rfc3339(),"pid":std::process::id(),
        "version":env!("CARGO_PKG_VERSION"),"event":event,"data":data})
}
pub fn record(event: &str, data: Value) {
    if let Some(writer) = WRITER.get() {
        let _ = writer.try_send(Message::Event(entry(event, data)));
    }
}
fn write_entry(dir: &Path, value: &Value) -> std::io::Result<()> {
    let path = dir.join("lifecycle.jsonl");
    if fs::metadata(&path).is_ok_and(|m| m.len() >= MAX_LOG_BYTES) {
        let previous = dir.join("lifecycle.previous.jsonl");
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        fs::rename(&path, previous)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")
}
fn session(dir: &Path, clean: bool) {
    let _ = crate::config::atomic_json(
        &dir.join("session.json"),
        &json!({"cleanExit":clean,"pid":std::process::id(),
            "version":env!("CARGO_PKG_VERSION"),"at":chrono::Utc::now().to_rfc3339()}),
    );
}
fn open_session(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    if let Ok(bytes) = fs::read(dir.join("session.json")) {
        if let Ok(previous) = serde_json::from_slice::<Value>(&bytes) {
            if previous.get("cleanExit") == Some(&Value::Bool(false)) {
                write_entry(
                    dir,
                    &entry(
                        "previous_session.unclean",
                        json!({
                            "pid":previous.get("pid"),"at":previous.get("at")
                        }),
                    ),
                )?;
            }
        }
    }
    session(dir, false);
    write_entry(dir, &entry("session.started", json!({})))
}
pub fn initialize(dir: PathBuf) {
    let (tx, rx) = mpsc::sync_channel(256);
    if WRITER.set(tx).is_err() {
        return;
    }
    std::thread::spawn(move || {
        if open_session(&dir).is_err() {
            return;
        }
        while let Ok(message) = rx.recv() {
            match message {
                Message::Event(value) => {
                    let _ = write_entry(&dir, &value);
                }
                Message::Urgent(value, done) => {
                    let _ = write_entry(&dir, &value);
                    let _ = done.send(());
                }
                Message::Exit(done) => {
                    let _ = write_entry(&dir, &entry("session.exited", json!({})));
                    session(&dir, true);
                    let _ = done.send(());
                    break;
                }
            }
        }
    });
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A panic payload can contain provider responses or credentials.
        if let Some(writer) = WRITER.get() {
            let (tx, rx) = mpsc::channel();
            let value = entry(
                "rust.panic",
                json!({"file":info.location().map(|p|p.file()),
                "line":info.location().map(|p|p.line())}),
            );
            if writer.try_send(Message::Urgent(value, tx)).is_ok() {
                // An FFI panic may abort immediately after the hook returns.
                let _ = rx.recv_timeout(Duration::from_millis(250));
            }
        }
        previous(info);
    }));
}
pub fn exited() {
    if let Some(writer) = WRITER.get() {
        let (tx, rx) = mpsc::channel();
        if writer.try_send(Message::Exit(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_millis(500));
        }
    }
}

pub fn window_destroyed(label: &str) {
    if let Ok(mut watched) = WATCHED.get_or_init(Default::default).lock() {
        watched.remove(label);
    }
    record("window.destroyed", json!({"window":label}));
}
pub fn on_page_load(webview: &Webview<Wry>, event: &tauri::webview::PageLoadPayload<'_>) {
    if !matches!(event.event(), PageLoadEvent::Finished) {
        return;
    }
    let label = webview.label().to_owned();
    if !WATCHED
        .get_or_init(Default::default)
        .lock()
        .is_ok_and(|mut watched| watched.insert(label.clone()))
    {
        return;
    }
    #[cfg(windows)]
    {
        let failed_label = label.clone();
        let result = webview.with_webview(move |platform| {
            use webview2_com::ProcessFailedEventHandler;
            let result = unsafe {
                (|| {
                    let view = platform.controller().CoreWebView2()?;
                    let mut token = 0;
                    view.add_ProcessFailed(
                        &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                            let mut kind = Default::default();
                            if let Some(args) = args {
                                let _ = args.ProcessFailedKind(&mut kind);
                            }
                            record(
                                "webview.process_failed",
                                json!({"window":failed_label,"kind":kind.0}),
                            );
                            Ok(())
                        })),
                        &mut token,
                    )
                })()
            };
            if result.is_err() {
                record("webview.observer_failed", json!({"window":label}));
            }
        });
        if result.is_err() {
            record(
                "webview.observer_dispatch_failed",
                json!({"window":webview.label()}),
            );
        }
    }
}

/// At most one pending UI ping. No automatic restart or process termination.
pub fn start_ui_probe(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut pending: Option<(Instant, Arc<std::sync::atomic::AtomicBool>)> = None;
        let mut warned = false;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Some((started, done)) = &pending {
                if done.load(std::sync::atomic::Ordering::Acquire) {
                    if warned {
                        record("ui.responsive_again", json!({}));
                    }
                    warned = false;
                } else {
                    if !warned && started.elapsed() >= Duration::from_secs(15) {
                        record(
                            "ui.unresponsive",
                            json!({"waitSeconds":started.elapsed().as_secs()}),
                        );
                        warned = true;
                    }
                    continue;
                }
            }
            let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let ack = done.clone();
            if app
                .run_on_main_thread(move || ack.store(true, std::sync::atomic::Ordering::Release))
                .is_err()
            {
                return;
            }
            pending = Some((Instant::now(), done));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_session_is_reported_but_clean_exit_is_not() {
        let dir = tempfile::tempdir().unwrap();
        open_session(dir.path()).unwrap();
        open_session(dir.path()).unwrap();
        session(dir.path(), true);
        open_session(dir.path()).unwrap();
        let text = fs::read_to_string(dir.path().join("lifecycle.jsonl")).unwrap();
        let rows: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(
            rows.iter()
                .filter(|r| r["event"] == "previous_session.unclean")
                .count(),
            1
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["event"] == "session.started")
                .count(),
            3
        );
    }
    #[test]
    fn oversized_log_rotates_and_keeps_the_new_event() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("lifecycle.jsonl"),
            vec![b' '; MAX_LOG_BYTES as usize],
        )
        .unwrap();
        write_entry(dir.path(), &entry("test", json!({}))).unwrap();
        assert_eq!(
            fs::metadata(dir.path().join("lifecycle.previous.jsonl"))
                .unwrap()
                .len(),
            MAX_LOG_BYTES
        );
        let value: Value =
            serde_json::from_slice(&fs::read(dir.path().join("lifecycle.jsonl")).unwrap()).unwrap();
        assert_eq!(value["event"], "test");
    }
}
