//! Arguments de l'exécutable de bureau : sans argument, l'application s'ouvre sans console ;
//! `--console` lance le serveur seul dans un terminal.

/// Ce que l'exécutable doit faire au lancement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// Fenêtre et barre système, aucune console.
    Gui,
    /// Serveur seul, journal dans la console.
    Console,
    Help,
    Version,
}

/// Lit les arguments (sans le nom du programme). Un argument inconnu est une erreur.
pub fn parse<I, S>(args: I) -> Result<Launch, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut launch = Launch::Gui;
    for arg in args {
        let next = match arg.as_ref() {
            "--console" | "-c" => Launch::Console,
            "--help" | "-h" | "/?" => Launch::Help,
            "--version" | "-V" => Launch::Version,
            other => return Err(format!("argument inconnu : {other}")),
        };
        // Aide puis version passent avant tout le reste, quel que soit l'ordre.
        launch = match (launch, next) {
            (Launch::Help, _) | (_, Launch::Help) => Launch::Help,
            (Launch::Version, _) | (_, Launch::Version) => Launch::Version,
            _ => next,
        };
    }
    Ok(launch)
}

/// Aide affichée par `--help`.
pub fn help(version: &str) -> String {
    format!(
        "PwaMote {version}\n\
         \n\
         Usage : PwaMote.exe [option]\n\
         \n\
         Sans option, ouvre l'application (fenêtre et barre système), sans console.\n\
         \n\
         Options :\n\
         \x20 -c, --console   serveur seul, journal dans la console (Ctrl+C pour arrêter)\n\
         \x20 -h, --help      affiche cette aide\n\
         \x20 -V, --version   affiche la version\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_argument_ouvre_l_application() {
        assert_eq!(parse(Vec::<String>::new()), Ok(Launch::Gui));
    }

    #[test]
    fn console_sur_demande() {
        assert_eq!(parse(["--console"]), Ok(Launch::Console));
        assert_eq!(parse(["-c"]), Ok(Launch::Console));
    }

    #[test]
    fn aide_et_version() {
        for a in ["--help", "-h", "/?"] {
            assert_eq!(parse([a]), Ok(Launch::Help), "{a}");
        }
        for a in ["--version", "-V"] {
            assert_eq!(parse([a]), Ok(Launch::Version), "{a}");
        }
    }

    #[test]
    fn l_aide_l_emporte_sur_le_reste() {
        assert_eq!(parse(["--console", "--help"]), Ok(Launch::Help));
        assert_eq!(parse(["--version", "--console"]), Ok(Launch::Version));
    }

    #[test]
    fn argument_inconnu_refuse() {
        let err = parse(["--consol"]).unwrap_err();
        assert!(err.contains("--consol"), "{err}");
    }

    #[test]
    fn l_aide_cite_la_version_et_les_options() {
        let h = help("1.2.3");
        assert!(h.contains("1.2.3"));
        for opt in ["--console", "--help", "--version"] {
            assert!(h.contains(opt), "{opt}");
        }
    }
}
