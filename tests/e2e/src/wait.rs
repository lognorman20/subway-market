//! Event-based waits — ready files, ports, HTTP health, predicates.
//! Poll with short interval; fail on bounded timeout. No blind sleeps for work completion.

use std::future::Future;
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, Instant};

use thiserror::Error;

/// Default poll interval for readiness checks.
pub const DEFAULT_POLL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct WaitOpts {
    pub timeout: Duration,
    pub poll: Duration,
}

impl WaitOpts {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            poll: DEFAULT_POLL,
        }
    }
}

#[derive(Debug, Error)]
pub enum WaitError {
    #[error("timeout after {elapsed:?}: {context}")]
    Timeout {
        elapsed: Duration,
        context: String,
    },
    #[error("{0}")]
    Other(String),
}

/// Wait until `path` exists (e.g. `logs/backend.ready`).
pub async fn wait_for_file(path: impl AsRef<Path>, opts: WaitOpts) -> Result<(), WaitError> {
    let path = path.as_ref().to_path_buf();
    let ctx = format!("file {}", path.display());
    wait_for_condition(opts, &ctx, || {
        let p = path.clone();
        async move { Ok(p.exists()) }
    })
    .await
}

/// Wait until TCP connect to `host:port` succeeds.
pub async fn wait_for_port(host: &str, port: u16, opts: WaitOpts) -> Result<(), WaitError> {
    let addr = format!("{host}:{port}");
    let ctx = format!("port {addr}");
    wait_for_condition(opts, &ctx, || {
        let addr = addr.clone();
        async move { Ok(TcpStream::connect(&addr).is_ok()) }
    })
    .await
}

/// Wait until GET `url` returns HTTP 2xx.
pub async fn wait_for_http_ok(client: &reqwest::Client, url: &str, opts: WaitOpts) -> Result<(), WaitError> {
    let ctx = format!("HTTP OK {url}");
    wait_for_condition(opts, &ctx, || {
        let client = client.clone();
        let url = url.to_string();
        async move {
            match client.get(&url).send().await {
                Ok(res) => Ok(res.status().is_success()),
                Err(_) => Ok(false),
            }
        }
    })
    .await
}

/// Poll `check` until it returns `Ok(true)` or timeout.
pub async fn wait_for_condition<F, Fut>(
    opts: WaitOpts,
    context: &str,
    mut check: F,
) -> Result<(), WaitError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<bool, WaitError>>,
{
    let start = Instant::now();
    loop {
        if check().await? {
            return Ok(());
        }
        if start.elapsed() >= opts.timeout {
            return Err(WaitError::Timeout {
                elapsed: start.elapsed(),
                context: context.to_string(),
            });
        }
        tokio::time::sleep(opts.poll).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn condition_succeeds_before_timeout() {
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = n.clone();
        wait_for_condition(WaitOpts::new(Duration::from_secs(2)), "counter", || {
            let n2 = n2.clone();
            async move {
                let v = n2.fetch_add(1, Ordering::SeqCst);
                Ok(v >= 2)
            }
        })
        .await
        .expect("should pass");
    }

    #[tokio::test]
    async fn condition_times_out() {
        let err = wait_for_condition(
            WaitOpts {
                timeout: Duration::from_millis(200),
                poll: Duration::from_millis(50),
            },
            "never",
            || async { Ok(false) },
        )
        .await
        .expect_err("timeout");
        match err {
            WaitError::Timeout { context, .. } => assert_eq!(context, "never"),
            other => panic!("unexpected {other}"),
        }
    }

    #[tokio::test]
    async fn wait_for_file_sees_created() {
        let dir = std::env::temp_dir().join(format!("e2e-wait-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ready");
        let path2 = path.clone();
        tokio::spawn(async move {
            // Event-based: create after short poll-scale delay (not arbitrary work sleep).
            tokio::time::sleep(Duration::from_millis(100)).await;
            std::fs::write(&path2, b"ok").unwrap();
        });
        wait_for_file(
            &path,
            WaitOpts {
                timeout: Duration::from_secs(2),
                poll: Duration::from_millis(50),
            },
        )
        .await
        .expect("file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
