//! Lancement au démarrage de Windows.

/// Que faire de l'entrée « lancer au démarrage » ? `Some(true)` : (ré)écrire l'entrée,
/// `Some(false)` : la retirer, `None` : ne rien toucher.
///
/// Quand l'option est voulue, on réécrit **toujours** l'entrée : elle contient le chemin de
/// l'exécutable, et l'édition portable change de nom à chaque version. Sans cela, le PC
/// relançait l'ancienne copie au démarrage, qui redemandait l'autorisation du pare-feu.
pub fn launch_at_login_action(wanted: bool, registered: bool) -> Option<bool> {
    match (wanted, registered) {
        (true, _) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wanted_entry_is_rewritten_with_the_current_executable() {
        assert_eq!(launch_at_login_action(true, false), Some(true));
        assert_eq!(launch_at_login_action(true, true), Some(true));
    }

    #[test]
    fn unwanted_entry_is_removed_only_if_present() {
        assert_eq!(launch_at_login_action(false, true), Some(false));
        assert_eq!(launch_at_login_action(false, false), None);
    }
}
