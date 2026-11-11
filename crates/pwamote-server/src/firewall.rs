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
pub fn firewall_script(exe: &str) -> String {
    let exe = ps_quote(exe);
    let name = ps_quote(RULE_NAME);
    format!(
        "$ErrorActionPreference = 'Stop'; \
         Get-NetFirewallApplicationFilter -Program {exe} -ErrorAction SilentlyContinue | Get-NetFirewallRule -ErrorAction SilentlyContinue | Remove-NetFirewallRule; \
         Get-NetFirewallRule -DisplayName {name} -ErrorAction SilentlyContinue | Remove-NetFirewallRule; \
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
pub fn check_script(exe: &str) -> String {
    let exe = ps_quote(exe);
    let name = ps_quote(RULE_NAME);
    format!(
        "$ErrorActionPreference = 'SilentlyContinue'; [Console]::OutputEncoding = [Text.Encoding]::UTF8; \
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
        Ok(Vec::new())
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
    match rule_state() {
        RuleState::Allowed | RuleState::Unknown => Ok(false),
        RuleState::Blocked | RuleState::Missing => allow_this_executable().map(|()| true),
    }
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
        assert_eq!(parse_state("", EXE), RuleState::Missing);
        assert_eq!(
            parse_state("Get-NetFirewallRule : erreur", EXE),
            RuleState::Unknown
        );
    }

    #[test]
    fn encoded_command_is_utf16le_base64() {
        // « ab » en UTF-16LE = 61 00 62 00.
        assert_eq!(encode_command("ab"), "YQBiAA==");
    }
}
