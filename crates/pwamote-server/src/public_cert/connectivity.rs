//! « Le PC a-t-il internet ? » : on tente simplement de joindre Let's Encrypt.

use std::time::Duration;

use futures_util::future::BoxFuture;

pub trait Probe: Send + Sync {
    fn online(&self) -> BoxFuture<'_, bool>;
}

/// Connexion TCP au serveur ACME (sans requête HTTP : rapide et sans effet).
pub struct TcpProbe {
    pub host: &'static str,
    pub port: u16,
    pub timeout: Duration,
}

impl Default for TcpProbe {
    fn default() -> Self {
        Self {
            host: "acme-v02.api.letsencrypt.org",
            port: 443,
            timeout: Duration::from_secs(5),
        }
    }
}

impl Probe for TcpProbe {
    fn online(&self) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            matches!(
                tokio::time::timeout(
                    self.timeout,
                    tokio::net::TcpStream::connect((self.host, self.port))
                )
                .await,
                Ok(Ok(_))
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reachable_and_unreachable_hosts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let up = TcpProbe {
            host: "127.0.0.1",
            port,
            timeout: Duration::from_secs(2),
        };
        assert!(up.online().await);
        drop(listener);
        let down = TcpProbe {
            host: "127.0.0.1",
            port,
            timeout: Duration::from_secs(2),
        };
        assert!(!down.online().await);
    }
}
