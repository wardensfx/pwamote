//! Console à la demande pour l'exécutable de bureau, compilé sans console (sous-système
//! « windows ») : rien ne s'ouvre au double-clic, mais `--console` en retrouve une.

use windows::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};

/// D'où vient la console obtenue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Console {
    /// Celle du terminal qui a lancé le programme : la sortie s'y affiche.
    Parent,
    /// Une fenêtre neuve (lancement par raccourci ou double-clic avec un argument).
    New,
    /// Aucune : la sortie reste invisible.
    None,
}

/// Rattache le processus à la console du terminal parent, sinon en ouvre une.
/// Les sorties standard déjà redirigées (`> fichier`, tube) restent intactes.
pub fn attach() -> Console {
    // SAFETY: appels Win32 sans argument pointeur ; échouent proprement si une console existe déjà.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_ok() {
            Console::Parent
        } else if AllocConsole().is_ok() {
            Console::New
        } else {
            Console::None
        }
    }
}
