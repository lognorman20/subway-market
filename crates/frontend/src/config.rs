use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub backend_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000);
        let backend_url = std::env::var("BACKEND_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".into());

        Self {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            backend_url,
        }
    }
}
