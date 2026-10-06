use crate::model::ApiError;
#[cfg(windows)]
const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
pub fn enabled() -> Result<bool, ApiError> {
    #[cfg(windows)]
    {
        use winreg::{enums::*, RegKey};
        let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey(KEY) {
            Ok(k) => k,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(error()),
        };
        return match key.get_value::<String, _>("AIDE monitor") {
            Ok(value) => Ok(value == command()?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(error()),
        };
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}
#[cfg(windows)]
fn command() -> Result<String, ApiError> {
    Ok(format!(
        "\"{}\"",
        std::env::current_exe()
            .map_err(|_| error())?
            .to_string_lossy()
    ))
}
pub fn set(enabled: bool) -> Result<bool, ApiError> {
    #[cfg(windows)]
    {
        use winreg::{enums::*, RegKey};
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(KEY)
            .map_err(|_| error())?;
        if enabled {
            key.set_value("AIDE monitor", &command()?)
                .map_err(|_| error())?;
        } else {
            match key.delete_value("AIDE monitor") {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(error()),
            }
        }
        return self::enabled();
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err(error())
    }
}
fn error() -> ApiError {
    ApiError::new("IO_ERROR", "无法更新开机自启设置")
}
