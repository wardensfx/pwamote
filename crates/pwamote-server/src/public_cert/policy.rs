//! Décisions de la tâche de fond, sans effet de bord : quand renouveler, quand revérifier.

use std::time::Duration;

use time::OffsetDateTime;

/// Let's Encrypt délivre pour 90 jours (bientôt moins) et conseille de renouveler au dernier tiers.
pub const RENEW_BEFORE: time::Duration = time::Duration::days(30);
/// Sans internet : on retente souvent, un test de connexion ne coûte presque rien.
pub const OFFLINE_POLL: Duration = Duration::from_secs(5 * 60);
/// Tout va bien : on repasse quand même régulièrement (changement d'IP, révocation…).
pub const HEALTHY_POLL: Duration = Duration::from_secs(6 * 3600);
/// Fonction désactivée ou incomplète : réveillée par un changement de réglage.
pub const IDLE_POLL: Duration = Duration::from_secs(24 * 3600);
const FIRST_BACKOFF: Duration = Duration::from_secs(10 * 60);
const MAX_BACKOFF: Duration = Duration::from_secs(12 * 3600);

/// Ce que la tâche doit faire maintenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Certificat valide et loin de l'échéance : revenir dans `Duration`.
    Keep(Duration),
    /// Il faut un certificat mais pas d'internet : revenir dans `Duration`.
    WaitForNetwork(Duration),
    /// Échec récent : patienter encore `Duration` (limites de Let's Encrypt).
    Backoff(Duration),
    /// Demander un certificat maintenant.
    Obtain,
}

/// Faut-il (re)demander un certificat ?
pub fn renewal_due(not_after: Option<OffsetDateTime>, now: OffsetDateTime) -> bool {
    match not_after {
        None => true,
        Some(t) => t - now <= RENEW_BEFORE,
    }
}

/// Attente avant la n-ième nouvelle tentative (10 min, 20, 40… plafonnée à 12 h).
pub fn backoff(failures: u32) -> Duration {
    if failures == 0 {
        return Duration::ZERO;
    }
    let factor = 1u32 << (failures - 1).min(16);
    (FIRST_BACKOFF * factor).min(MAX_BACKOFF)
}

pub fn plan(
    not_after: Option<OffsetDateTime>,
    now: OffsetDateTime,
    online: bool,
    failures: u32,
    last_failure: Option<OffsetDateTime>,
) -> Plan {
    if !renewal_due(not_after, now) {
        let until_due = (not_after.unwrap() - RENEW_BEFORE - now)
            .try_into()
            .unwrap_or(Duration::ZERO);
        return Plan::Keep(until_due.clamp(Duration::from_secs(60), HEALTHY_POLL));
    }
    if !online {
        return Plan::WaitForNetwork(OFFLINE_POLL);
    }
    if let Some(at) = last_failure {
        let elapsed: Duration = (now - at).try_into().unwrap_or(Duration::ZERO);
        let wait = backoff(failures);
        if elapsed < wait {
            return Plan::Backoff(wait - elapsed);
        }
    }
    Plan::Obtain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
    }
    fn days(n: i64) -> Option<OffsetDateTime> {
        Some(now() + time::Duration::days(n))
    }

    #[test]
    fn renews_thirty_days_before_expiry() {
        assert!(renewal_due(None, now()));
        assert!(renewal_due(days(30), now()));
        assert!(renewal_due(days(-1), now()));
        assert!(!renewal_due(days(31), now()));
    }

    #[test]
    fn a_fresh_cert_is_kept_and_rechecked_regularly() {
        assert_eq!(
            plan(days(80), now(), true, 0, None),
            Plan::Keep(HEALTHY_POLL)
        );
        // Même hors ligne : le certificat se sert sans internet.
        assert_eq!(
            plan(days(80), now(), false, 0, None),
            Plan::Keep(HEALTHY_POLL)
        );
        // Juste avant l'échéance du renouvellement, on revient pile à temps.
        let p = plan(
            Some(now() + time::Duration::days(30) + time::Duration::hours(2)),
            now(),
            true,
            0,
            None,
        );
        assert_eq!(p, Plan::Keep(Duration::from_secs(2 * 3600)));
    }

    #[test]
    fn without_internet_waits_and_polls_often() {
        assert_eq!(
            plan(None, now(), false, 0, None),
            Plan::WaitForNetwork(OFFLINE_POLL)
        );
        assert_eq!(
            plan(days(3), now(), false, 3, None),
            Plan::WaitForNetwork(OFFLINE_POLL)
        );
    }

    #[test]
    fn obtains_when_due_and_online() {
        assert_eq!(plan(None, now(), true, 0, None), Plan::Obtain);
        assert_eq!(plan(days(10), now(), true, 0, None), Plan::Obtain);
    }

    #[test]
    fn failures_back_off_exponentially() {
        assert_eq!(backoff(1), Duration::from_secs(600));
        assert_eq!(backoff(2), Duration::from_secs(1200));
        assert_eq!(backoff(30), Duration::from_secs(12 * 3600));
        let failed = Some(now() - time::Duration::minutes(4));
        assert_eq!(
            plan(None, now(), true, 1, failed),
            Plan::Backoff(Duration::from_secs(360))
        );
        let long_ago = Some(now() - time::Duration::minutes(11));
        assert_eq!(plan(None, now(), true, 1, long_ago), Plan::Obtain);
    }
}
