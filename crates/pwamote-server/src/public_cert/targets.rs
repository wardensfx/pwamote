//! Le vrai choix du domaine : service PwaMote (`<id>.pwamote.app`) ou domaine personnel.

use std::sync::Arc;

use futures_util::future::BoxFuture;

use super::config::{CertMode, PublicCertConfig};
use super::dns;
use super::maintainer::{Resolved, Target, Targets};
use super::relay::RelayClient;

pub struct SystemTargets {
    pub http: reqwest::Client,
    pub relay: RelayClient,
}

impl SystemTargets {
    fn relay_target(&self, creds: super::relay::RelayCredentials) -> Resolved {
        Resolved::Ready(Target {
            domain: creds.domain.clone(),
            dns: Arc::new(self.relay.dns(creds)),
        })
    }
}

impl Targets for SystemTargets {
    fn resolve<'a>(&'a self, cfg: &'a PublicCertConfig, online: bool) -> BoxFuture<'a, Resolved> {
        Box::pin(async move {
            match cfg.mode {
                CertMode::Off => Resolved::Incomplete("désactivé".into()),
                CertMode::Custom => match cfg.missing() {
                    Some(reason) => Resolved::Incomplete(reason),
                    None => match dns::for_config(cfg, self.http.clone()) {
                        Some(dns) => Resolved::Ready(Target {
                            domain: cfg.domain.clone(),
                            dns,
                        }),
                        None => Resolved::Incomplete("Service DNS inconnu.".into()),
                    },
                },
                CertMode::Auto => {
                    if let Some(creds) = self.relay.saved() {
                        return self.relay_target(creds);
                    }
                    if !online {
                        return Resolved::NeedsNetwork;
                    }
                    match self.relay.register().await {
                        Ok(creds) => self.relay_target(creds),
                        Err(e) => Resolved::Failed(e.to_string()),
                    }
                }
            }
        })
    }
}
