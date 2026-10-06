//! Runs the real isolated WebView against a synthetic package; never touches accounts.
use crate::{service::Service, theme_package as package, theme_runtime as runtime};
use serde_json::{json, Value};
use std::io::Write;
use tauri::Manager;
pub fn enabled() -> bool {
    std::env::args().any(|a| a == "--theme-smoke")
}
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = run(&app);
        let report = match result {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        let ok = report["ok"] == true;
        let _ = crate::config::atomic_json(
            &std::env::current_dir()
                .unwrap()
                .join("artifacts/theme-smoke.json"),
            &report,
        );
        app.exit(if ok { 0 } else { 1 });
    });
}
fn run(app: &tauri::AppHandle) -> Result<Value, String> {
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = temp.path().join("test.aidetheme");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, data) in [
        (
            "manifest.json",
            r#"{"schemaVersion":1,"id":"dev.aide.security-test","name":"Isolation test","version":"1.0.0","apiVersion":"1"}"#,
        ),
        (
            "index.html",
            r#"<!doctype html><html><head><script src="test.js" defer></script></head><body>Isolation test</body></html>"#,
        ),
        ("test.js", SCRIPT),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let root = runtime::root(&app.state::<Service>());
    package::install(&root, &path, false, true).map_err(|e| e.message)?;
    runtime::select(app, "dev.aide.security-test").map_err(|e| e.message)?;
    let mut last = Value::Null;
    for _ in 0..120 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let windows = app.webview_windows();
        let Some(w) = windows
            .values()
            .find(|w| w.label().starts_with("aide-theme-"))
        else {
            continue;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        w.eval_with_callback("JSON.stringify(window.__AIDE_TEST__ || {pending:true,hasAide:!!window.aide,href:location.href,body:document.body?.innerText,resources:performance.getEntriesByType('resource').map(r=>r.name)})",move|value|{let _=tx.send(value);}).map_err(|e|e.to_string())?;
        if let Ok(v) = rx.recv_timeout(std::time::Duration::from_secs(1)) {
            let outer: Value = serde_json::from_str(&v).unwrap_or(Value::Null);
            last = if let Some(s) = outer.as_str() {
                serde_json::from_str(s).unwrap_or(Value::Null)
            } else {
                outer
            };
            if last.get("ok").is_some() {
                break;
            }
        }
    }
    if last["ok"] != true {
        return Err(format!(
            "theme runtime failed: {last}; notice={:?}",
            app.state::<runtime::Runtime>().notice.lock().unwrap()
        ));
    }
    let window = runtime::visible(app).ok_or("ready did not activate theme")?;
    if window.is_resizable().ok() != Some(false) || window.is_maximizable().ok() != Some(false) {
        return Err("window flags failed".into());
    }
    if !runtime::toggle(app)
        || !app
            .state::<runtime::Runtime>()
            .folded
            .load(std::sync::atomic::Ordering::SeqCst)
    {
        return Err("fold failed".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(350));
    if window.is_visible().ok() != Some(false) {
        return Err("fold did not hide theme".into());
    }
    crate::window_action(app, "hide").map_err(|e| e.message)?;
    crate::window_action(app, "show").map_err(|e| e.message)?;
    if window.is_visible().ok() != Some(false) {
        return Err("show while folded revealed wrong window".into());
    }
    runtime::resume(app);
    if window.is_visible().ok() != Some(true) {
        return Err("resume failed".into());
    }
    let stale_url = format!(
        "aide://localhost/{}/rpc",
        window.label().trim_start_matches("aide-theme-")
    );
    runtime::restore(app, None);
    let denied = runtime::response(
        app,
        window.label(),
        tauri::http::Request::builder()
            .method("POST")
            .uri(stale_url)
            .body(b"{}".to_vec())
            .unwrap(),
    );
    if denied.status() != 403 {
        return Err("revoked session served request".into());
    }
    if runtime::visible(app).is_some() {
        return Err("session not revoked".into());
    }
    // A package that never calls ready must recover by itself.
    let record = package::installed(&root, "dev.aide.security-test").unwrap();
    std::fs::write(
        package::content(&root, &record).join("test.js"),
        "document.body.textContent='never ready'",
    )
    .unwrap();
    runtime::select(app, "dev.aide.security-test").map_err(|e| e.message)?;
    std::thread::sleep(std::time::Duration::from_secs(10));
    if app
        .webview_windows()
        .keys()
        .any(|k| k.starts_with("aide-theme-"))
    {
        return Err("timeout failed to revoke session".into());
    }
    // Load the shipped authoring template too, including its CSS and actual SDK calls.
    let example = temp.path().join("example.aidetheme");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&example).unwrap());
    for (name, data) in [
        (
            "manifest.json",
            include_str!("../../examples/widget/manifest.json"),
        ),
        (
            "index.html",
            include_str!("../../examples/widget/index.html"),
        ),
        ("app.js", include_str!("../../examples/widget/app.js")),
        ("style.css", include_str!("../../examples/widget/style.css")),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let example = package::install(&root, &example, false, true).map_err(|e| e.message)?;
    runtime::select(app, &example.manifest.id).map_err(|e| e.message)?;
    for _ in 0..80 {
        if runtime::visible(app).is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let example_window = runtime::visible(app).ok_or("example did not become ready")?;
    let (tx, rx) = std::sync::mpsc::channel();
    example_window.eval_with_callback("document.styleSheets.length > 0 && document.querySelector('main').getBoundingClientRect().height > 0 && !!document.querySelector('#settings')", move |v| {let _=tx.send(v);}).map_err(|e|e.to_string())?;
    if rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .ok()
        .as_deref()
        != Some("true")
    {
        return Err("example assets or layout failed".into());
    }
    runtime::uninstall(app, &example.manifest.id, true).map_err(|e| e.message)?;
    if runtime::visible(app).is_some()
        || app.state::<Service>().settings().active_theme_id != "default"
    {
        return Err("active uninstall did not restore builtin".into());
    }
    Ok(
        json!({"ok":true,"isolatedSource":true,"realAccountQueries":0,"runtime":last,"timeoutFallback":true,"sessionRevoked":true,"windowLocked":true,"foldResume":true,"revokedRequestDenied":true,"exampleReady":true,"activeUninstall":true}),
    )
}
const SCRIPT: &str = r#"
(async()=>{
const checks=[];const assert=(value,name)=>{if(!value)throw new Error(name);checks.push(name);};
window.__AIDE_TEST__={pending:true,checks};
const violations=[];window.addEventListener('securitypolicyviolation',event=>violations.push(event.effectiveDirective));
assert(!!window.aide,'sdk');assert(localStorage.getItem('isolation-probe')===null,'isolated-browser-storage');localStorage.setItem('isolation-probe','private');const snapshot=await aide.usage.get();assert(Array.isArray(snapshot.accounts)&&snapshot.accounts.length===0,'selected-snapshot');
await aide.storage.set('test',{value:42});assert((await aide.storage.get('test')).value===42,'storage');await aide.storage.remove('test');assert(await aide.storage.get('test')===null,'storage-remove');
const size=await aide.window.resize(320,100);assert(size.width===320&&size.height===100,'resize');
let denied=false;try{await Promise.race([window.__TAURI_INTERNALS__.invoke('hud_v1_settings_get',{request:{}}),new Promise((_,reject)=>setTimeout(()=>reject(new Error('blocked')),500))]);}catch{denied=true;}assert(denied,'raw-ipc-denied');
denied=false;try{await fetch('https://example.invalid/leak?test=synthetic');}catch{denied=true;}assert(denied,'external-fetch-denied');
denied=false;try{await fetch('http://tauri.localhost/index.html');}catch{denied=true;}assert(denied,'host-resources-denied');
denied=false;try{await fetch('http://aide.localhost/other-session/index.html');}catch{denied=true;}assert(denied,'cross-theme-fetch-denied');
const ownUrl=location.href;location.href='https://example.invalid/navigation';await new Promise(r=>setTimeout(r,100));assert(location.href===ownUrl,'external-navigation-denied');
const popup=window.open('https://example.invalid/popup');assert(!popup,'popup-denied');
const image=new Image();const imageBlocked=await new Promise(resolve=>{image.onerror=()=>resolve(true);image.onload=()=>resolve(false);image.src='https://example.invalid/image.png';setTimeout(()=>resolve(true),300);});assert(imageBlocked,'external-image-denied');
await new Promise(r=>setTimeout(r,100));assert(violations.includes('connect-src')&&violations.includes('img-src'),'host-csp-enforced');
assert(typeof RTCPeerConnection==='undefined'&&typeof Worker==='undefined','alternate-transports-disabled');
const url=new URL('rpc',location.href);const sessionId=location.pathname.split('/')[1];
let reply=await (await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({apiVersion:'1',sessionId,requestId:'probe',method:'shell.run',params:{}})})).json();assert(!reply.ok&&reply.error.code==='FORBIDDEN','unknown-method-denied');
reply=await (await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({apiVersion:'1',sessionId:'wrong',requestId:'probe',method:'usage.get',params:{}})})).json();assert(!reply.ok,'wrong-session-denied');
await aide.theme.ready();window.__AIDE_TEST__={ok:true,checks};
})().catch(error=>{window.__AIDE_TEST__={ok:false,error:String(error),stack:error.stack};});
"#;
