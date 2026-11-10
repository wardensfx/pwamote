//! Certificat public Let's Encrypt, avec repli sur l'autorité locale.
//!
//! Let's Encrypt ne signe que des noms de domaine publics dont on prouve la
//! maîtrise : ni adresse IP, ni nom `.local`. PwaMote utilise donc le défi
//! DNS-01 sur un domaine choisi par l'utilisateur (DuckDNS gratuit, ou une zone
//! Cloudflare), dont l'enregistrement A pointe vers l'adresse du PC sur le
//! réseau local. Le PC n'a pas besoin d'être joignable depuis internet.
//!
//! Les deux certificats sont servis en même temps, selon le nom demandé (SNI) :
//! le domaine public reçoit le certificat Let's Encrypt, tout le reste (IP,
//! `.local`, `localhost`) le certificat de l'autorité locale. Une tâche de fond
//! surveille la connexion et l'échéance, et obtient ou renouvelle le certificat.

pub mod acme;
pub mod cert;
pub mod config;
pub mod connectivity;
pub mod dns;
pub mod maintainer;
pub mod policy;
pub mod relay;
pub mod resolver;
pub mod store;
pub mod targets;

pub use config::{CertMode, DnsProvider, PublicCertConfig};
pub use maintainer::{Maintainer, PublicCertPhase, PublicCertStatus};
pub use resolver::CertResolver;
