use crate::model::ApiError;
use std::time::Duration;

pub fn proxy_client(address: &str) -> Result<reqwest::Client, ApiError> {
    let invalid = || {
        ApiError::new(
            "INVALID_ARGUMENT",
            "代理地址无效，请使用 HTTP、HTTPS 或 SOCKS5 地址",
        )
    };
    let url = reqwest::Url::parse(address.trim()).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(invalid());
    }
    reqwest::Client::builder()
        .no_proxy()
        .proxy(reqwest::Proxy::all(url.as_str()).map_err(|_| invalid())?)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| ApiError::new("NETWORK_ERROR", "无法初始化代理连接"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_proxy_without_exposing_address() {
        for address in [
            "socks5://127.0.0.1:7893",
            "socks5h://localhost:1080",
            "http://localhost:8080",
            "https://localhost:8080",
        ] {
            assert!(proxy_client(address).is_ok());
        }
        for address in [
            "",
            "localhost:7893",
            "file:///tmp/proxy",
            "http://localhost/path",
            "http://localhost/?secret=1",
        ] {
            assert!(proxy_client(address).is_err());
        }
    }
}
