//! Autorisation de PwaMote dans le pare-feu Windows.
//!
//! Après une recompilation ou une réinstallation, l'exécutable change : Windows le bloque tant
//! qu'on n'a pas accepté sa fenêtre d'autorisation (ou crée une règle de blocage si on l'a fermée).
//! Le PC paraît « en ligne », mais le téléphone ne le joint plus. Un clic dans la fenêtre remplace
//! toutes les règles de cet exécutable par une seule règle d'entrée, limitée au réseau local.

use base64::Engine as _;

/// Nom de la règle créée, visible dans « Pare-feu Windows avec fonctions avancées de sécurité ».
pub const RULE_NAME: &str = "PwaMote (téléphones du réseau local)";

/// Chaîne PowerShell entre apostrophes : une apostrophe s'écrit en la doublant.
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Script qui retire les règles existantes de cet exécutable (y compris un blocage créé quand la
/// fenêtre de Windows a été fermée) et en crée une qui n'accepte que le sous-réseau local, en TCP.
pub fn firewall_script(exe: &str) -> String {
    let exe = ps_quote(exe);
    let name = ps_quote(RULE_NAME);
    format!(
        "$ErrorActionPreference = 'Stop'; \
         Get-NetFirewallApplicationFilter -Program {exe} -ErrorAction SilentlyContinue | Get-NetFirewallRule -ErrorAction SilentlyContinue | Remove-NetFirewallRule; \
         Get-NetFirewallRule -DisplayName {name} -ErrorAction SilentlyContinue | Remove-NetFirewallRule; \
         New-NetFirewallRule -DisplayName {name} -Direction Inbound -Action Allow -Program {exe} -Protocol TCP -RemoteAddress LocalSubnet -Profile Any | Out-Null"
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

/// Script de lecture seule (sans élévation) : `allowed`, `blocked` ou `missing`.
pub fn check_script(exe: &str) -> String {
    let exe = ps_quote(exe);
    format!(
        "$r = Get-NetFirewallApplicationFilter -Program {exe} -ErrorAction SilentlyContinue | Get-NetFirewallRule -ErrorAction SilentlyContinue | Where-Object {{ $_.Direction -eq 'Inbound' -and $_.Enabled -eq 'True' }}; \
         if ($r | Where-Object Action -eq 'Block') {{ 'blocked' }} elseif ($r | Where-Object Action -eq 'Allow') {{ 'allowed' }} else {{ 'missing' }}"
    )
}

pub fn parse_state(output: &str) -> RuleState {
    match output.trim().lines().last().map(str::trim) {
        Some("allowed") => RuleState::Allowed,
        Some("blocked") => RuleState::Blocked,
        Some("missing") => RuleState::Missing,
        _ => RuleState::Unknown,
    }
}

/// Lit l'état du pare-feu pour cet exécutable, sans fenêtre ni élévation.
pub fn rule_state() -> RuleState {
    let Ok(exe) = std::env::current_exe() else {
        return RuleState::Unknown;
    };
    let script = check_script(&exe.display().to_string());
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
            Ok(out) => parse_state(&String::from_utf8_lossy(&out.stdout)),
            Err(_) => RuleState::Unknown,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = script;
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
    fn parses_the_check_output() {
        assert_eq!(parse_state("allowed\r\n"), RuleState::Allowed);
        assert_eq!(parse_state("blocked"), RuleState::Blocked);
        assert_eq!(parse_state("avertissement\nmissing\n"), RuleState::Missing);
        assert_eq!(parse_state(""), RuleState::Unknown);
    }

    #[test]
    fn encoded_command_is_utf16le_base64() {
        // « ab » en UTF-16LE = 61 00 62 00.
        assert_eq!(encode_command("ab"), "YQBiAA==");
    }
}
