//! Autorisation de PwaMote dans le pare-feu Windows.
//!
//! Après une recompilation ou une réinstallation, l'exécutable change : Windows le bloque tant
//! qu'on n'a pas accepté sa fenêtre d'autorisation (ou crée une règle de blocage si on l'a fermée).
//! Le PC paraît « en ligne », mais le téléphone ne le joint plus. Un clic dans la fenêtre remplace
//! toutes les règles de cet exécutable par une seule règle d'entrée, limitée au réseau local.
//!
//! La règle couvre TCP (HTTPS) **et** UDP (annonce mDNS) : avec TCP seul, l'écoute mDNS faisait
//! apparaître la fenêtre de Windows, qui créait une règle de blocage, que le lancement suivant
//! retirait avec une nouvelle demande d'administrateur… à chaque lancement.

use base64::Engine as _;

/// Nom de la règle créée, visible dans « Pare-feu Windows avec fonctions avancées de sécurité ».
pub const RULE_NAME: &str = "PwaMote (téléphones du réseau local)";

/// Chaîne PowerShell entre apostrophes : une apostrophe s'écrit en la doublant.
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Script qui retire les règles existantes de cet exécutable (y compris un blocage créé quand la
/// fenêtre de Windows a été fermée) et en crée une qui n'accepte que le sous-réseau local
/// (tous protocoles : HTTPS en TCP, mDNS en UDP).
///
/// Les règles PwaMote d'**autres** exécutables restent en place tant que leur fichier existe :
/// l'édition portable change de nom à chaque version, et supprimer la règle de l'autre copie
/// lui faisait redemander l'administrateur à son prochain lancement (au démarrage du PC).
pub fn firewall_script(exe: &str) -> String {
    let exe = ps_quote(exe);
    let name = ps_quote(RULE_NAME);
    format!(
        "$ErrorActionPreference = 'Stop'; \
         Get-NetFirewallApplicationFilter -Program {exe} -ErrorAction SilentlyContinue | Get-NetFirewallRule -ErrorAction SilentlyContinue | Remove-NetFirewallRule; \
         Get-NetFirewallRule -DisplayName {name} -ErrorAction SilentlyContinue | Where-Object {{ $p = ($_ | Get-NetFirewallApplicationFilter).Program; -not $p -or -not (Test-Path -LiteralPath $p) }} | Remove-NetFirewallRule; \
         New-NetFirewallRule -DisplayName {name} -Direction Inbound -Action Allow -Program {exe} -Protocol Any -RemoteAddress LocalSubnet -Profile Any | Out-Null"
    )
}

/// Encodage attendu par `powershell -EncodedCommand` : UTF-16LE puis base64. Évite tout souci
/// de guillemets dans le chemin de l'exécutable.
pub fn encode_command(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// État des règles d'entrée de cet exécutable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleState {
    /// Une règle autorise l'exécutable, aucune ne le bloque.
    Allowed,
    /// Une règle bloque l'exécutable (fenêtre de Windows fermée, par exemple).
    Blocked,
    /// Aucune règle : Windows bloquera les téléphones.
    Missing,
    /// Impossible de lire le pare-feu.
    Unknown,
}

/// Script de lecture seule (sans élévation) : les règles de cet exécutable et celle de PwaMote,
/// en JSON. La décision se prend côté Rust (`decide`), où elle est testée.
///
/// Au démarrage du PC, le service du pare-feu peut ne pas encore répondre : les lectures
/// échouaient en silence, la liste vide passait pour « aucune règle » et PwaMote demandait
/// l'administrateur à chaque démarrage. Le script vérifie donc d'abord que le pare-feu répond,
/// sinon il écrit un message non JSON (→ `Unknown`, on ne demande rien).
pub fn check_script(exe: &str) -> String {
    let exe = ps_quote(exe);
    let name = ps_quote(RULE_NAME);
    format!(
        "$ErrorActionPreference = 'SilentlyContinue'; [Console]::OutputEncoding = [Text.Encoding]::UTF8; \
         if ((Get-Service -Name MpsSvc).Status -ne 'Running' -or -not (Get-NetFirewallProfile)) {{ 'PARE-FEU INDISPONIBLE'; exit 1 }}; \
         $rules = @(Get-NetFirewallApplicationFilter -Program {exe} | Get-NetFirewallRule) + @(Get-NetFirewallRule -DisplayName {name}); \
         $items = @($rules | Where-Object {{ $_ }} | ForEach-Object {{ \
           [pscustomobject]@{{ action = \"$($_.Action)\"; enabled = \"$($_.Enabled)\"; direction = \"$($_.Direction)\"; \
             program = \"$(($_ | Get-NetFirewallApplicationFilter).Program)\"; protocol = \"$(($_ | Get-NetFirewallPortFilter).Protocol)\" }} }}); \
         ConvertTo-Json -InputObject $items -Compress"
    )
}

/// Une règle telle que lue par `check_script`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RuleInfo {
    pub action: String,
    pub enabled: String,
    pub direction: String,
    pub program: String,
    pub protocol: String,
}

/// Les téléphones passent-ils ? Seules comptent les règles d'entrée actives de cet exécutable
/// (la casse du chemin n'a pas d'importance sous Windows). Il faut TCP (HTTPS) et UDP (mDNS) :
/// sans UDP, Windows afficherait sa propre fenêtre au démarrage de l'annonce mDNS.
pub fn decide(rules: &[RuleInfo], exe: &str) -> RuleState {
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let mine: Vec<&RuleInfo> = rules
        .iter()
        .filter(|r| same(&r.direction, "Inbound") && same(&r.enabled, "True"))
        .filter(|r| same(r.program.trim(), exe.trim()))
        .collect();
    if mine.iter().any(|r| same(&r.action, "Block")) {
        return RuleState::Blocked;
    }
    let covers = |proto: &str| {
        mine.iter().any(|r| {
            same(&r.action, "Allow") && (same(&r.protocol, "Any") || same(&r.protocol, proto))
        })
    };
    if covers("TCP") && covers("UDP") {
        RuleState::Allowed
    } else {
        RuleState::Missing
    }
}

/// Lit la sortie du script ; `Unknown` si elle est illisible (on ne demande alors rien).
pub fn parse_state(output: &str, exe: &str) -> RuleState {
    let json = output.trim();
    let rules: Result<Vec<RuleInfo>, _> = if json.is_empty() {
        // PowerShell n'a rien écrit (le script sort toujours au moins `[]`) : lecture ratée.
        return RuleState::Unknown;
    } else if json.starts_with('{') {
        // Selon la version de PowerShell, un tableau d'un élément peut sortir comme un objet seul.
        serde_json::from_str::<RuleInfo>(json).map(|r| vec![r])
    } else {
        serde_json::from_str(json)
    };
    match rules {
        Ok(rules) => decide(&rules, exe),
        Err(_) => RuleState::Unknown,
    }
}

/// Lit l'état du pare-feu pour cet exécutable, sans fenêtre ni élévation.
pub fn rule_state() -> RuleState {
    let Ok(exe) = std::env::current_exe() else {
        return RuleState::Unknown;
    };
    let exe = exe.display().to_string();
    let script = check_script(&exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        match std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
            .arg(encode_command(&script))
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let state = parse_state(&stdout, &exe);
                tracing::info!(?state, rules = %stdout.trim(), "règles du pare-feu pour PwaMote");
                state
            }
            Err(_) => RuleState::Unknown,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (script, exe);
        RuleState::Unknown
    }
}

/// Au lancement : si les téléphones seraient bloqués, demande l'autorisation tout de suite
/// (une fenêtre UAC, une seule fois par exécutable) au lieu d'attendre un clic dans la fenêtre.
/// Renvoie `true` si la règle vient d'être créée.
pub fn ensure_allowed() -> Result<bool, String> {
    ensure_allowed_with(
        rule_state,
        allow_this_executable,
        std::thread::sleep,
        CHECK_ATTEMPTS,
        CHECK_RETRY_DELAY,
    )
}

/// Lectures tentées au lancement avant de renoncer (sans rien demander).
pub const CHECK_ATTEMPTS: u32 = 4;
/// Attente entre deux lectures : le pare-feu finit de démarrer avec la session Windows.
pub const CHECK_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(3);

/// Cœur testable de [`ensure_allowed`] : relit tant que le pare-feu ne répond pas, et ne
/// demande l'administrateur que sur un état lu avec certitude (`Missing` ou `Blocked`).
pub fn ensure_allowed_with(
    mut read: impl FnMut() -> RuleState,
    allow: impl FnOnce() -> Result<(), String>,
    mut sleep: impl FnMut(std::time::Duration),
    attempts: u32,
    delay: std::time::Duration,
) -> Result<bool, String> {
    for attempt in 0..attempts.max(1) {
        if attempt > 0 {
            sleep(delay);
        }
        match read() {
            RuleState::Allowed => return Ok(false),
            RuleState::Blocked | RuleState::Missing => return allow().map(|()| true),
            RuleState::Unknown => {}
        }
    }
    tracing::warn!("pare-feu illisible au lancement : aucune demande d'autorisation");
    Ok(false)
}

/// Demande l'élévation (fenêtre UAC) et applique la règle. Bloquant jusqu'à la fin du script.
pub fn allow_this_executable() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = firewall_script(&exe.display().to_string());
    #[cfg(windows)]
    {
        crate::platform::windows::run_elevated_powershell(&encode_command(&script))
    }
    #[cfg(not(windows))]
    {
        let _ = script;
        Err("le pare-feu ne se règle que sous Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_targets_the_executable_and_the_local_subnet_only() {
        let s = firewall_script(r"C:\Users\Yann\dev\target\release\pwamote-desktop.exe");
        assert!(s.contains(r"-Program 'C:\Users\Yann\dev\target\release\pwamote-desktop.exe'"));
        assert!(s.contains("-RemoteAddress LocalSubnet"));
        assert!(s.contains("-Direction Inbound -Action Allow"));
        assert!(s.contains("Remove-NetFirewallRule"));
    }

    #[test]
    fn quotes_in_the_path_cannot_break_out_of_the_string() {
        let s = firewall_script(r"C:\Users\O'Brien\pwamote.exe");
        assert!(s.contains(r"'C:\Users\O''Brien\pwamote.exe'"));
    }

    #[test]
    fn check_script_only_reads_rules() {
        let s = check_script(r"C:\Program Files\PwaMote\pwamote.exe");
        assert!(s.contains(r"-Program 'C:\Program Files\PwaMote\pwamote.exe'"));
        assert!(!s.contains("Remove-") && !s.contains("New-"));
    }

    #[test]
    fn the_rule_covers_https_and_mdns() {
        let s = firewall_script(r"C:\PwaMote\pwamote.exe");
        assert!(s.contains("-Protocol Any"), "{s}");
    }

    const EXE: &str = r"C:\Program Files\PwaMote\pwamote-desktop.exe";

    fn rule(action: &str, protocol: &str, program: &str) -> RuleInfo {
        RuleInfo {
            action: action.into(),
            enabled: "True".into(),
            direction: "Inbound".into(),
            program: program.into(),
            protocol: protocol.into(),
        }
    }

    #[test]
    fn our_rule_in_place_means_no_administrator_window() {
        assert_eq!(
            decide(&[rule("Allow", "Any", EXE)], EXE),
            RuleState::Allowed
        );
        // Casse différente du chemin : même fichier sous Windows.
        let lower = EXE.to_lowercase();
        assert_eq!(
            decide(&[rule("Allow", "Any", &lower)], EXE),
            RuleState::Allowed
        );
        // Règles séparées TCP et UDP : suffisant aussi.
        let both = [rule("Allow", "TCP", EXE), rule("Allow", "UDP", EXE)];
        assert_eq!(decide(&both, EXE), RuleState::Allowed);
    }

    #[test]
    fn missing_or_incomplete_rules_ask_once() {
        assert_eq!(decide(&[], EXE), RuleState::Missing);
        // Ancienne règle TCP seule : l'annonce mDNS (UDP) ferait surgir la fenêtre de Windows.
        assert_eq!(
            decide(&[rule("Allow", "TCP", EXE)], EXE),
            RuleState::Missing
        );
        // Règle PwaMote d'un ancien exécutable : ne compte pas pour celui-ci.
        let old = rule("Allow", "Any", r"C:\dev\target\debug\pwamote-desktop.exe");
        assert_eq!(decide(&[old], EXE), RuleState::Missing);
        // Désactivée ou sortante : ne compte pas.
        let mut off = rule("Allow", "Any", EXE);
        off.enabled = "False".into();
        let mut out = rule("Allow", "Any", EXE);
        out.direction = "Outbound".into();
        assert_eq!(decide(&[off, out], EXE), RuleState::Missing);
    }

    #[test]
    fn a_block_rule_wins() {
        let rules = [rule("Allow", "Any", EXE), rule("Block", "UDP", EXE)];
        assert_eq!(decide(&rules, EXE), RuleState::Blocked);
    }

    #[test]
    fn parses_the_check_output() {
        let one = format!(
            r#"{{"action":"Allow","enabled":"True","direction":"Inbound","program":{},"protocol":"Any"}}"#,
            serde_json::to_string(EXE).unwrap()
        );
        assert_eq!(
            parse_state(&format!("[{one}]\r\n"), EXE),
            RuleState::Allowed
        );
        assert_eq!(parse_state(&one, EXE), RuleState::Allowed);
        assert_eq!(parse_state("[]", EXE), RuleState::Missing);
        // Rien en sortie : PowerShell a échoué, pas « aucune règle ».
        assert_eq!(parse_state("", EXE), RuleState::Unknown);
        assert_eq!(
            parse_state("PARE-FEU INDISPONIBLE", EXE),
            RuleState::Unknown
        );
        assert_eq!(
            parse_state("Get-NetFirewallRule : erreur", EXE),
            RuleState::Unknown
        );
    }

    #[test]
    fn check_script_refuses_to_answer_while_the_firewall_is_starting() {
        let s = check_script(EXE);
        assert!(
            s.contains("MpsSvc") && s.contains("Get-NetFirewallProfile"),
            "{s}"
        );
        assert!(s.contains("PARE-FEU INDISPONIBLE"));
    }

    #[test]
    fn other_copies_keep_their_rule() {
        // Édition portable : un nom de fichier par version. La règle d'une autre copie encore
        // présente sur le disque ne doit pas être supprimée.
        let s = firewall_script(EXE);
        assert!(s.contains("Test-Path -LiteralPath $p"), "{s}");
        assert!(!s.contains(&format!(
            "Get-NetFirewallRule -DisplayName {} -ErrorAction SilentlyContinue | Remove-NetFirewallRule",
            ps_quote(RULE_NAME)
        )));
    }

    fn run(states: &[RuleState]) -> (Result<bool, String>, usize, usize) {
        let mut it = states.iter().copied();
        let mut reads = 0;
        let mut sleeps = 0;
        let asked = std::cell::Cell::new(0);
        let r = ensure_allowed_with(
            || {
                reads += 1;
                it.next().unwrap_or(RuleState::Unknown)
            },
            || {
                asked.set(asked.get() + 1);
                Ok(())
            },
            |_| sleeps += 1,
            4,
            std::time::Duration::ZERO,
        );
        let _ = sleeps;
        (r, reads, asked.get())
    }

    #[test]
    fn rule_in_place_never_asks() {
        assert_eq!(run(&[RuleState::Allowed]), (Ok(false), 1, 0));
    }

    #[test]
    fn firewall_still_starting_is_read_again_then_never_asks() {
        // Démarrage du PC : illisible, puis la règle est bien là.
        assert_eq!(
            run(&[RuleState::Unknown, RuleState::Unknown, RuleState::Allowed]),
            (Ok(false), 3, 0)
        );
        // Toujours illisible : on abandonne sans fenêtre.
        assert_eq!(run(&[]), (Ok(false), 4, 0));
    }

    #[test]
    fn asks_once_only_when_the_rule_is_really_missing() {
        assert_eq!(run(&[RuleState::Missing]), (Ok(true), 1, 1));
        assert_eq!(
            run(&[RuleState::Unknown, RuleState::Blocked]),
            (Ok(true), 2, 1)
        );
    }

    #[test]
    fn encoded_command_is_utf16le_base64() {
        // « ab » en UTF-16LE = 61 00 62 00.
        assert_eq!(encode_command("ab"), "YQBiAA==");
    }
}
