use crate::model::{AccountQuota, ApiError};
use chrono::Utc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct CachedProbe {
    pid: u32,
    ports: Vec<u16>,
    token: String,
    cached_at: Instant,
}

static PROBE_CACHE: Mutex<Option<CachedProbe>> = Mutex::new(None);

#[cfg(windows)]
pub async fn fetch_quota(hud_account_id: &str) -> Result<AccountQuota, ApiError> {
    let (ports, token) = probe_ports_and_token()
        .ok_or_else(|| ApiError::new("NOT_RUNNING", "本地 Language Server 未运行"))?;

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|_| ApiError::new("LOCAL_CLIENT_FAILED", "创建本地请求客户端失败"))?;

    for port in ports {
        let url = format!(
            "http://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary"
        );
        let res = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("x-codeium-csrf-token", &token)
            .body("{}")
            .send()
            .await;

        match res {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(bytes) = resp.bytes().await {
                        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            if let Ok(quota) = super::providers::parse(
                                &val,
                                "antigravity",
                                hud_account_id,
                                Utc::now(),
                            ) {
                                return Ok(quota);
                            }
                        }
                    }
                }
            }
            Err(_) => {}
        }
    }

    // If probing with cached info failed, invalidate the cache so next attempt rediscovers.
    if let Ok(mut guard) = PROBE_CACHE.lock() {
        *guard = None;
    }

    Err(ApiError::new(
        "LOCAL_SERVER_ERROR",
        "本地 Language Server 未能返回额度",
    ))
}

#[cfg(not(windows))]
pub async fn fetch_quota(_hud_account_id: &str) -> Result<AccountQuota, ApiError> {
    Err(ApiError::new("NOT_SUPPORTED", "非 Windows 平台"))
}

#[cfg(windows)]
fn probe_ports_and_token() -> Option<(Vec<u16>, String)> {
    // 1. Check in-memory cache first (valid for 5 minutes as long as process PID is alive)
    if let Ok(guard) = PROBE_CACHE.lock() {
        if let Some(cached) = &*guard {
            if cached.cached_at.elapsed() < Duration::from_secs(300) {
                if is_process_alive(cached.pid) {
                    return Some((cached.ports.clone(), cached.token.clone()));
                }
            }
        }
    }

    // 2. Discover pid and token via fast, zero-subprocesses Win32 APIs (< 2ms)
    let pid = find_language_server_pid()?;
    let token = read_csrf_token(pid)?;
    let ports = get_listening_ports(pid);
    if ports.is_empty() || token.is_empty() {
        return None;
    }

    if let Ok(mut guard) = PROBE_CACHE.lock() {
        *guard = Some(CachedProbe {
            pid,
            ports: ports.clone(),
            token: token.clone(),
            cached_at: Instant::now(),
        });
    }

    Some((ports, token))
}

#[cfg(windows)]
#[repr(C)]
struct ProcessEntry32W {
    dw_size: u32,
    cnt_usage: u32,
    th32_process_id: u32,
    th32_default_heap_id: usize,
    th32_module_id: u32,
    cnt_threads: u32,
    th32_parent_process_id: u32,
    pc_pri_class_base: i32,
    dw_flags: u32,
    sz_exe_file: [u16; 260],
}

#[cfg(windows)]
#[repr(C)]
struct ProcessBasicInformation {
    exit_status: i32,
    reserved1: u32,
    peb_base_address: usize,
    affinity_mask: usize,
    base_priority: i32,
    reserved2: u32,
    unique_process_id: usize,
    inherited_from_unique_process_id: usize,
}

#[cfg(windows)]
#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    padding: u32,
    buffer: usize,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> *mut std::ffi::c_void;
    fn Process32FirstW(h_snapshot: *mut std::ffi::c_void, lppe: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(h_snapshot: *mut std::ffi::c_void, lppe: *mut ProcessEntry32W) -> i32;
    fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
    fn OpenProcess(
        dw_desired_access: u32,
        b_inherit_handle: i32,
        dw_process_id: u32,
    ) -> *mut std::ffi::c_void;
    fn ReadProcessMemory(
        h_process: *mut std::ffi::c_void,
        lp_base_address: *const std::ffi::c_void,
        lp_buffer: *mut std::ffi::c_void,
        n_size: usize,
        lp_number_of_bytes_read: *mut usize,
    ) -> i32;
    fn GetExitCodeProcess(h_process: *mut std::ffi::c_void, lp_exit_code: *mut u32) -> i32;
}

#[cfg(windows)]
#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        process_handle: *mut std::ffi::c_void,
        process_information_class: u32,
        process_information: *mut std::ffi::c_void,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[cfg(windows)]
#[link(name = "iphlpapi")]
extern "system" {
    fn GetExtendedTcpTable(
        p_tcp_table: *mut std::ffi::c_void,
        pdw_size: *mut u32,
        b_order: i32,
        ul_af: u32,
        table_class: u32,
        reserved: u32,
    ) -> u32;
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        ok != 0 && exit_code == 259 /* STILL_ACTIVE */
    }
}

#[cfg(windows)]
fn find_language_server_pid() -> Option<u32> {
    const TH32CS_SNAPPROCESS: u32 = 0x00000002;
    let target_name = "language_server.exe";

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot as isize == -1 {
            return None;
        }

        let mut entry = ProcessEntry32W {
            dw_size: std::mem::size_of::<ProcessEntry32W>() as u32,
            cnt_usage: 0,
            th32_process_id: 0,
            th32_default_heap_id: 0,
            th32_module_id: 0,
            cnt_threads: 0,
            th32_parent_process_id: 0,
            pc_pri_class_base: 0,
            dw_flags: 0,
            sz_exe_file: [0; 260],
        };

        let mut found = None;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let name = String::from_utf16_lossy(&entry.sz_exe_file);
                let trimmed = name.trim_matches('\0');
                if trimmed.eq_ignore_ascii_case(target_name) {
                    found = Some(entry.th32_process_id);
                    break;
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        found
    }
}

#[cfg(windows)]
fn read_csrf_token(pid: u32) -> Option<String> {
    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    const PROCESS_VM_READ: u32 = 0x0010;

    unsafe {
        let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if process.is_null() {
            return None;
        }

        let mut pbi = std::mem::zeroed::<ProcessBasicInformation>();
        let mut ret_len: u32 = 0;
        let status = NtQueryInformationProcess(
            process,
            0, // ProcessBasicInformation
            &mut pbi as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<ProcessBasicInformation>() as u32,
            &mut ret_len,
        );

        if status != 0 || pbi.peb_base_address == 0 {
            CloseHandle(process);
            return None;
        }

        // On 64-bit Windows, ProcessParameters pointer is at PEB + 0x20
        let mut params_ptr: usize = 0;
        let mut bytes_read: usize = 0;
        if ReadProcessMemory(
            process,
            (pbi.peb_base_address + 0x20) as *const std::ffi::c_void,
            &mut params_ptr as *mut _ as *mut std::ffi::c_void,
            8,
            &mut bytes_read,
        ) == 0
            || params_ptr == 0
        {
            CloseHandle(process);
            return None;
        }

        // CommandLine (UNICODE_STRING) is at ProcessParameters + 0x70
        let mut cmd_str = std::mem::zeroed::<UnicodeString>();
        if ReadProcessMemory(
            process,
            (params_ptr + 0x70) as *const std::ffi::c_void,
            &mut cmd_str as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<UnicodeString>(),
            &mut bytes_read,
        ) == 0
            || cmd_str.buffer == 0
            || cmd_str.length == 0
        {
            CloseHandle(process);
            return None;
        }

        let char_count = (cmd_str.length / 2) as usize;
        let mut buf = vec![0u16; char_count];
        if ReadProcessMemory(
            process,
            cmd_str.buffer as *const std::ffi::c_void,
            buf.as_mut_ptr() as *mut std::ffi::c_void,
            cmd_str.length as usize,
            &mut bytes_read,
        ) == 0
        {
            CloseHandle(process);
            return None;
        }

        CloseHandle(process);

        let cmdline = String::from_utf16_lossy(&buf);
        let key = "--csrf_token";
        if let Some(pos) = cmdline.find(key) {
            let after = &cmdline[pos + key.len()..];
            let token = after
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| c == '"' || c == '\'');
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
        None
    }
}

#[cfg(windows)]
fn get_listening_ports(pid: u32) -> Vec<u16> {
    const AF_INET: u32 = 2;
    const TCP_TABLE_OWNER_PID_LISTENER: u32 = 3;

    let mut size: u32 = 0;
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            1,
            AF_INET,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
    }
    if size == 0 {
        return vec![];
    }

    let mut buf = vec![0u8; size as usize];
    let ret = unsafe {
        GetExtendedTcpTable(
            buf.as_mut_ptr() as *mut std::ffi::c_void,
            &mut size,
            1,
            AF_INET,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if ret != 0 || buf.len() < 4 {
        return vec![];
    }

    let num_entries = u32::from_ne_bytes(buf[0..4].try_into().unwrap_or([0; 4])) as usize;
    let mut ports = Vec::new();
    let mut offset = 4;
    for _ in 0..num_entries {
        if offset + 24 > buf.len() {
            break;
        }
        let row_pid = u32::from_ne_bytes(buf[offset + 20..offset + 24].try_into().unwrap_or([0; 4]));
        if row_pid == pid {
            let raw_port = u32::from_ne_bytes(buf[offset + 8..offset + 12].try_into().unwrap_or([0; 4]));
            let port = u16::from_be((raw_port & 0xffff) as u16);
            ports.push(port);
        }
        offset += 24;
    }

    ports.sort_by(|a, b| b.cmp(a));
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_native_probe_and_fetch() {
        if let Some((ports, token)) = probe_ports_and_token() {
            println!("Probed ports: {:?}, token: {}", ports, token);
            assert!(!ports.is_empty());
            assert!(!token.is_empty());
            let quota = fetch_quota("test-account").await;
            println!("Quota result: {:?}", quota);
            assert!(quota.is_ok());
        }
    }
}
