//! Appairage des téléphones.
//!
//! 1. Le serveur émet un code à usage unique, de courte durée, affiché en QR.
//! 2. Le téléphone échange ce code contre un jeton d'appareil permanent.
//! 3. Aux connexions suivantes, le téléphone présente son jeton.
//!
//! Seule l'empreinte SHA-256 des jetons est conservée. L'aléa est fourni par
//! l'appelant pour garder ce module pur et testable.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Durée de validité d'un code d'appairage.
pub const CODE_TTL_SECS: u64 = 5 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub paired_at: u64,
    pub last_seen: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingCode {
    code: String,
    expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairingError {
    #[error("code d'appairage inconnu, expiré ou déjà utilisé")]
    InvalidCode,
    #[error("nom d'appareil vide")]
    EmptyName,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingRegistry {
    devices: Vec<Device>,
    #[serde(skip)]
    pending: Vec<PendingCode>,
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Comparaison en temps constant, pour ne rien laisser fuiter par le chronométrage.
fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

impl PairingRegistry {
    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    /// Enregistre un nouveau code. Les codes expirés sont purgés au passage.
    pub fn issue_code(&mut self, code: String, now: u64) {
        self.pending.retain(|p| p.expires_at > now);
        self.pending.push(PendingCode {
            code,
            expires_at: now + CODE_TTL_SECS,
        });
    }

    /// Échange un code contre un appareil enregistré avec le jeton fourni.
    pub fn redeem(
        &mut self,
        code: &str,
        device_name: &str,
        device_id: String,
        token: &str,
        now: u64,
    ) -> Result<&Device, PairingError> {
        let name = device_name.trim();
        if name.is_empty() {
            return Err(PairingError::EmptyName);
        }
        self.pending.retain(|p| p.expires_at > now);
        let index = self
            .pending
            .iter()
            .position(|p| constant_time_eq(&p.code, code))
            .ok_or(PairingError::InvalidCode)?;
        self.pending.remove(index);

        let name: String = name.chars().take(64).collect();
        self.devices.push(Device {
            id: device_id,
            name,
            token_hash: hash_token(token),
            paired_at: now,
            last_seen: now,
        });
        Ok(self.devices.last().expect("pushed above"))
    }

    /// Retrouve l'appareil correspondant à un jeton et met à jour sa dernière connexion.
    pub fn authenticate(&mut self, token: &str, now: u64) -> Option<&Device> {
        let hash = hash_token(token);
        let device = self
            .devices
            .iter_mut()
            .find(|d| constant_time_eq(&d.token_hash, &hash))?;
        device.last_seen = now;
        Some(device)
    }

    pub fn revoke(&mut self, device_id: &str) -> bool {
        let before = self.devices.len();
        self.devices.retain(|d| d.id != device_id);
        self.devices.len() != before
    }

    pub fn has_pending_code(&self, now: u64) -> bool {
        self.pending.iter().any(|p| p.expires_at > now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with_code(code: &str, now: u64) -> PairingRegistry {
        let mut r = PairingRegistry::default();
        r.issue_code(code.into(), now);
        r
    }

    #[test]
    fn a_valid_code_pairs_a_device() {
        let mut r = registry_with_code("K1", 100);
        let d = r
            .redeem("K1", "iPhone", "dev-1".into(), "secret", 110)
            .unwrap();
        assert_eq!(d.id, "dev-1");
        assert_eq!(d.name, "iPhone");
        assert_ne!(
            d.token_hash, "secret",
            "le jeton ne doit jamais être stocké en clair"
        );
    }

    #[test]
    fn a_code_is_single_use() {
        let mut r = registry_with_code("K1", 100);
        r.redeem("K1", "A", "a".into(), "t1", 101).unwrap();
        assert_eq!(
            r.redeem("K1", "B", "b".into(), "t2", 102).unwrap_err(),
            PairingError::InvalidCode
        );
    }

    #[test]
    fn a_code_expires() {
        let mut r = registry_with_code("K1", 100);
        let err = r
            .redeem("K1", "A", "a".into(), "t", 100 + CODE_TTL_SECS)
            .unwrap_err();
        assert_eq!(err, PairingError::InvalidCode);
    }

    #[test]
    fn an_unknown_code_is_rejected() {
        let mut r = registry_with_code("K1", 100);
        assert_eq!(
            r.redeem("nope", "A", "a".into(), "t", 101).unwrap_err(),
            PairingError::InvalidCode
        );
    }

    #[test]
    fn an_empty_name_is_rejected_without_burning_the_code() {
        let mut r = registry_with_code("K1", 100);
        assert_eq!(
            r.redeem("K1", "   ", "a".into(), "t", 101).unwrap_err(),
            PairingError::EmptyName
        );
        assert!(r.redeem("K1", "A", "a".into(), "t", 102).is_ok());
    }

    #[test]
    fn long_names_are_truncated() {
        let mut r = registry_with_code("K1", 0);
        let d = r
            .redeem("K1", &"é".repeat(200), "a".into(), "t", 1)
            .unwrap();
        assert_eq!(d.name.chars().count(), 64);
    }

    #[test]
    fn a_paired_device_authenticates_with_its_token() {
        let mut r = registry_with_code("K1", 100);
        r.redeem("K1", "A", "a".into(), "secret", 101).unwrap();
        let d = r.authenticate("secret", 500).unwrap();
        assert_eq!(d.id, "a");
        assert_eq!(d.last_seen, 500);
        assert!(r.authenticate("wrong", 501).is_none());
    }

    #[test]
    fn a_revoked_device_can_no_longer_authenticate() {
        let mut r = registry_with_code("K1", 100);
        r.redeem("K1", "A", "a".into(), "secret", 101).unwrap();
        assert!(r.revoke("a"));
        assert!(r.authenticate("secret", 102).is_none());
        assert!(!r.revoke("a"));
    }

    #[test]
    fn pending_codes_are_not_persisted() {
        let mut r = registry_with_code("K1", 100);
        r.redeem("K1", "A", "a".into(), "s", 101).unwrap();
        r.issue_code("K2".into(), 102);
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("K2"));
        let mut back: PairingRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.devices().len(), 1);
        assert!(back.authenticate("s", 103).is_some());
    }

    #[test]
    fn hash_is_stable_hex_sha256() {
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
