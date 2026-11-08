//! Actions programmables : un panneau de boutons façon Stream Deck, défini sur le PC.
//!
//! * Le **PC** est seul à définir les actions (`Deck`) : ce qu'elles font (`ActionStep`)
//!   ne quitte jamais le serveur. Le téléphone ne reçoit qu'une vue (`DeckView`) : libellés,
//!   disposition et champs à remplir.
//! * Le téléphone demande l'exécution d'un bouton par son identifiant, avec des valeurs.
//!   Le serveur valide chaque valeur contre la déclaration du champ (`ActionButton::plan`)
//!   avant de produire un plan d'exécution (`PlannedStep`).
//! * Les valeurs ne sont insérées que dans les textes, les arguments et les adresses,
//!   jamais dans le nom du programme ni dans son dossier : un argument reste un seul
//!   argument, sans passer par un interpréteur de commandes.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::protocol::{Key, Modifier};

pub const MAX_PAGES: usize = 24;
pub const MIN_COLUMNS: u8 = 2;
pub const MAX_COLUMNS: u8 = 5;
pub const MIN_ROWS: u8 = 1;
pub const MAX_ROWS: u8 = 6;
pub const MAX_STEPS: usize = 32;
pub const MAX_INPUTS: usize = 8;
pub const MAX_OPTIONS: usize = 32;
pub const MAX_ARGS: usize = 32;
pub const MAX_DELAY_MS: u32 = 10_000;
pub const MAX_LABEL: usize = 40;
pub const MAX_ICON: usize = 8;
pub const MAX_TEXT: usize = 4_000;
/// Longueur maximale d'une valeur saisie sur le téléphone.
pub const MAX_VALUE: usize = 1_000;

/// Teinte d'un bouton, choisie dans une petite palette accordée au reste de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ButtonColor {
    #[default]
    Glass,
    Amber,
    Red,
    Green,
    Blue,
    Violet,
}

/// Une étape d'action, exécutée sur le PC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum ActionStep {
    /// Appui sur une touche, avec des modificateurs maintenus. Passe par le clavier virtuel de
    /// Windows comme une vraie touche : raccourcis et extensions de navigateur la voient.
    Keys {
        key: Key,
        #[serde(default)]
        modifiers: Vec<Modifier>,
    },
    /// Texte tapé tel quel (accents, emoji). Accepte les champs : `{nom}`.
    Text { text: String },
    /// Lance un programme, sans interpréteur de commandes. Les arguments acceptent les champs.
    Run {
        program: String,
        #[serde(default)]
        args: Vec<String>,
        /// Dossier de travail ; vide = dossier courant de PwaMote.
        #[serde(default)]
        cwd: String,
        /// Pas de fenêtre de console (scripts PowerShell, outils en ligne de commande).
        #[serde(default)]
        hidden: bool,
    },
    /// Ouvre une adresse web, un fichier ou un dossier avec l'application par défaut.
    /// Accepte les champs ; `{nom:url}` encode la valeur pour une adresse.
    Open { target: String },
    /// Pause entre deux étapes.
    Delay { ms: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChoiceOption {
    pub label: String,
    pub value: String,
}

fn default_on() -> String {
    "true".into()
}

fn default_off() -> String {
    "false".into()
}

/// Ce que le téléphone demande avant de lancer l'action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum InputKind {
    /// Liste déroulante de valeurs prédéfinies.
    Choice {
        options: Vec<ChoiceOption>,
        /// Valeur choisie par défaut (doit faire partie des options).
        #[serde(default)]
        default: String,
    },
    Slider {
        min: f64,
        max: f64,
        step: f64,
        default: f64,
        /// Unité affichée à côté de la valeur (« % », « s »…).
        #[serde(default)]
        unit: String,
    },
    Number {
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        #[serde(default)]
        default: f64,
        #[serde(default)]
        integer: bool,
    },
    Text {
        #[serde(default)]
        default: String,
        #[serde(default)]
        placeholder: String,
    },
    /// Interrupteur : insère `on` ou `off`.
    Toggle {
        #[serde(default)]
        default: bool,
        #[serde(default = "default_on")]
        on: String,
        #[serde(default = "default_off")]
        off: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActionInput {
    /// Nom utilisé dans les étapes : `{id}`. Lettres, chiffres et `_`.
    pub id: String,
    pub label: String,
    pub kind: InputKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActionButton {
    pub id: String,
    pub label: String,
    /// Emoji ou texte très court affiché en grand.
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub color: ButtonColor,
    #[serde(default)]
    pub inputs: Vec<ActionInput>,
    #[serde(default)]
    pub steps: Vec<ActionStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeckPage {
    pub id: String,
    pub name: String,
    pub columns: u8,
    pub rows: u8,
    /// Cases de la grille, ligne par ligne : `columns × rows` cases, vides ou occupées.
    pub cells: Vec<Option<ActionButton>>,
}

/// Toutes les pages d'actions, telles que définies sur le PC.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Deck {
    pub pages: Vec<DeckPage>,
}

// ---------- Vue envoyée au téléphone ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ButtonView {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub color: ButtonColor,
    pub inputs: Vec<ActionInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PageView {
    pub id: String,
    pub name: String,
    pub columns: u8,
    pub rows: u8,
    pub cells: Vec<Option<ButtonView>>,
}

/// Ce que le téléphone voit du panneau : jamais les étapes elles-mêmes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeckView {
    pub pages: Vec<PageView>,
}

/// Valeur saisie sur le téléphone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum InputValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

/// Étape prête à exécuter : champs remplacés, valeurs validées.
#[derive(Debug, Clone, PartialEq)]
pub enum PlannedStep {
    Keys {
        key: Key,
        modifiers: Vec<Modifier>,
    },
    Text(String),
    Run {
        program: String,
        args: Vec<String>,
        cwd: Option<String>,
        hidden: bool,
    },
    Open(String),
    Delay(u32),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ActionError {
    #[error("cette action n'existe plus sur le PC")]
    UnknownAction,
    #[error("champ inconnu : {0}")]
    UnknownInput(String),
    #[error("« {label} » : {reason}")]
    InvalidValue { label: String, reason: String },
    #[error("l'action n'a aucune étape")]
    Empty,
    #[error("une étape « Lancer » n'a pas de programme")]
    MissingProgram,
    #[error("une étape « Ouvrir » n'a pas de cible")]
    MissingTarget,
}

// ---------- Champs dans les textes ----------

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Encode une valeur pour l'insérer dans une adresse (RFC 3986, caractères non réservés conservés).
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Remplace `{nom}` par la valeur du champ, `{nom:url}` par la valeur encodée pour une adresse.
/// `{{` et `}}` donnent des accolades. Un nom inconnu est laissé tel quel.
pub fn render(template: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(pos) = rest.find(['{', '}']) {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        if tail.starts_with('{') {
            if let Some(end) = tail.find('}') {
                let inner = &tail[1..end];
                let (name, filter) = match inner.split_once(':') {
                    Some((n, f)) => (n, Some(f)),
                    None => (inner, None),
                };
                if !name.is_empty() && name.chars().all(is_name_char) {
                    if let (Some(value), Some("url") | None) = (values.get(name), filter) {
                        match filter {
                            Some(_) => out.push_str(&url_encode(value)),
                            None => out.push_str(value),
                        }
                        rest = &tail[end + 1..];
                        continue;
                    }
                }
            }
        }
        out.push_str(&tail[..1]);
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

// ---------- Validation des valeurs ----------

/// Écrit un nombre sans zéros superflus : `2`, `1.25`, `-0.5`.
pub fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn invalid(input: &ActionInput, reason: impl Into<String>) -> ActionError {
    ActionError::InvalidValue {
        label: input.label.clone(),
        reason: reason.into(),
    }
}

fn as_number(input: &ActionInput, v: &InputValue) -> Result<f64, ActionError> {
    let n = match v {
        InputValue::Number(n) => *n,
        InputValue::Text(t) => t
            .trim()
            .replace(',', ".")
            .parse::<f64>()
            .map_err(|_| invalid(input, "un nombre est attendu"))?,
        InputValue::Bool(_) => return Err(invalid(input, "un nombre est attendu")),
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(invalid(input, "un nombre est attendu"))
    }
}

impl ActionInput {
    /// Valeur par défaut, déjà sous forme de texte.
    pub fn default_value(&self) -> String {
        match &self.kind {
            InputKind::Choice { options, default } => {
                if options.iter().any(|o| &o.value == default) {
                    default.clone()
                } else {
                    options.first().map(|o| o.value.clone()).unwrap_or_default()
                }
            }
            InputKind::Slider { default, .. } | InputKind::Number { default, .. } => {
                format_number(*default)
            }
            InputKind::Text { default, .. } => default.clone(),
            InputKind::Toggle { default, on, off } => {
                if *default {
                    on.clone()
                } else {
                    off.clone()
                }
            }
        }
    }

    /// Valide une valeur envoyée par le téléphone et la convertit en texte.
    pub fn validate(&self, value: &InputValue) -> Result<String, ActionError> {
        match &self.kind {
            InputKind::Choice { options, .. } => {
                let InputValue::Text(t) = value else {
                    return Err(invalid(self, "choix attendu"));
                };
                options
                    .iter()
                    .find(|o| &o.value == t)
                    .map(|o| o.value.clone())
                    .ok_or_else(|| invalid(self, "ce choix n'est pas proposé"))
            }
            InputKind::Slider { min, max, step, .. } => {
                let n = as_number(self, value)?;
                if n < *min - 1e-9 || n > *max + 1e-9 {
                    return Err(invalid(
                        self,
                        format!("entre {} et {}", format_number(*min), format_number(*max)),
                    ));
                }
                let snapped = min + ((n - min) / step).round() * step;
                Ok(format_number(snapped.clamp(*min, *max)))
            }
            InputKind::Number {
                min, max, integer, ..
            } => {
                let n = as_number(self, value)?;
                if *integer && n.fract() != 0.0 {
                    return Err(invalid(self, "un nombre entier est attendu"));
                }
                if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                    let reason = match (min, max) {
                        (Some(a), Some(b)) => {
                            format!("entre {} et {}", format_number(*a), format_number(*b))
                        }
                        (Some(a), None) => format!("au moins {}", format_number(*a)),
                        (None, Some(b)) => format!("au plus {}", format_number(*b)),
                        (None, None) => unreachable!(),
                    };
                    return Err(invalid(self, reason));
                }
                Ok(format_number(n))
            }
            InputKind::Text { .. } => {
                let t = match value {
                    InputValue::Text(t) => t.clone(),
                    InputValue::Number(n) => format_number(*n),
                    InputValue::Bool(_) => return Err(invalid(self, "texte attendu")),
                };
                if t.chars().count() > MAX_VALUE {
                    return Err(invalid(self, "texte trop long"));
                }
                if t.contains('\0') {
                    return Err(invalid(self, "caractère interdit"));
                }
                Ok(t)
            }
            InputKind::Toggle { on, off, .. } => match value {
                InputValue::Bool(b) => Ok(if *b { on.clone() } else { off.clone() }),
                _ => Err(invalid(self, "oui ou non attendu")),
            },
        }
    }
}

impl ActionButton {
    /// Valeurs finales de tous les champs : celles du téléphone, validées, ou les valeurs par défaut.
    pub fn resolve(
        &self,
        values: &BTreeMap<String, InputValue>,
    ) -> Result<BTreeMap<String, String>, ActionError> {
        if let Some(unknown) = values
            .keys()
            .find(|k| !self.inputs.iter().any(|i| &i.id == *k))
        {
            return Err(ActionError::UnknownInput(unknown.clone()));
        }
        self.inputs
            .iter()
            .map(|input| {
                let v = match values.get(&input.id) {
                    Some(v) => input.validate(v)?,
                    None => input.default_value(),
                };
                Ok((input.id.clone(), v))
            })
            .collect()
    }

    /// Plan d'exécution : valeurs validées, champs remplacés.
    pub fn plan(
        &self,
        values: &BTreeMap<String, InputValue>,
    ) -> Result<Vec<PlannedStep>, ActionError> {
        let values = self.resolve(values)?;
        if self.steps.is_empty() {
            return Err(ActionError::Empty);
        }
        let mut plan = Vec::with_capacity(self.steps.len());
        for step in &self.steps {
            plan.push(match step {
                ActionStep::Keys { key, modifiers } => PlannedStep::Keys {
                    key: *key,
                    modifiers: modifiers.clone(),
                },
                ActionStep::Text { text } => {
                    let t = render(text, &values);
                    if t.is_empty() {
                        continue;
                    }
                    PlannedStep::Text(t)
                }
                ActionStep::Run {
                    program,
                    args,
                    cwd,
                    hidden,
                } => {
                    // Le programme et son dossier ne sont jamais construits à partir des valeurs.
                    if program.trim().is_empty() {
                        return Err(ActionError::MissingProgram);
                    }
                    PlannedStep::Run {
                        program: program.trim().to_string(),
                        args: args.iter().map(|a| render(a, &values)).collect(),
                        cwd: (!cwd.trim().is_empty()).then(|| cwd.trim().to_string()),
                        hidden: *hidden,
                    }
                }
                ActionStep::Open { target } => {
                    let t = render(target, &values);
                    if t.trim().is_empty() {
                        return Err(ActionError::MissingTarget);
                    }
                    PlannedStep::Open(t.trim().to_string())
                }
                ActionStep::Delay { ms } => PlannedStep::Delay((*ms).min(MAX_DELAY_MS)),
            });
        }
        Ok(plan)
    }

    pub fn view(&self) -> ButtonView {
        ButtonView {
            id: self.id.clone(),
            label: self.label.clone(),
            icon: self.icon.clone(),
            color: self.color,
            inputs: self.inputs.clone(),
        }
    }
}

// ---------- Nettoyage d'un panneau reçu de l'interface ou lu sur le disque ----------

fn truncate(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

fn finite_or(v: f64, fallback: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.chars().all(|c| is_name_char(c) || c == '-')
}

fn sanitize_input(input: ActionInput, index: usize, taken: &mut HashSet<String>) -> ActionInput {
    let mut id: String = input.id.trim().to_string();
    if !(valid_id(&id) && !id.contains('-')) || taken.contains(&id) {
        id = (index + 1..)
            .map(|n| format!("champ{n}"))
            .find(|c| !taken.contains(c))
            .expect("un nom libre existe toujours");
    }
    taken.insert(id.clone());
    let label = match truncate(&input.label, MAX_LABEL) {
        l if l.is_empty() => id.clone(),
        l => l,
    };
    let kind = match input.kind {
        InputKind::Choice { options, default } => {
            let mut seen = HashSet::new();
            let options: Vec<ChoiceOption> = options
                .into_iter()
                .map(|o| ChoiceOption {
                    label: truncate(&o.label, MAX_LABEL),
                    value: o.value.chars().take(MAX_VALUE).collect(),
                })
                .filter(|o| seen.insert(o.value.clone()))
                .map(|o| ChoiceOption {
                    label: if o.label.is_empty() {
                        o.value.clone()
                    } else {
                        o.label
                    },
                    value: o.value,
                })
                .take(MAX_OPTIONS)
                .collect();
            let default = if options.iter().any(|o| o.value == default) {
                default
            } else {
                options.first().map(|o| o.value.clone()).unwrap_or_default()
            };
            InputKind::Choice { options, default }
        }
        InputKind::Slider {
            min,
            max,
            step,
            default,
            unit,
        } => {
            let (mut min, mut max) = (finite_or(min, 0.0), finite_or(max, 100.0));
            if min > max {
                std::mem::swap(&mut min, &mut max);
            }
            if min == max {
                max = min + 1.0;
            }
            let step = match finite_or(step, 1.0) {
                s if s > 0.0 && s <= max - min => s,
                _ => ((max - min) / 100.0).max(f64::EPSILON),
            };
            InputKind::Slider {
                min,
                max,
                step,
                default: finite_or(default, min).clamp(min, max),
                unit: truncate(&unit, 8),
            }
        }
        InputKind::Number {
            min,
            max,
            default,
            integer,
        } => {
            let mut min = min.filter(|v| v.is_finite());
            let mut max = max.filter(|v| v.is_finite());
            if let (Some(a), Some(b)) = (min, max) {
                if a > b {
                    std::mem::swap(&mut min, &mut max);
                }
            }
            let mut default = finite_or(default, 0.0);
            if integer {
                default = default.round();
            }
            if let Some(m) = min {
                default = default.max(m);
            }
            if let Some(m) = max {
                default = default.min(m);
            }
            InputKind::Number {
                min,
                max,
                default,
                integer,
            }
        }
        InputKind::Text {
            default,
            placeholder,
        } => InputKind::Text {
            default: default.chars().take(MAX_VALUE).collect(),
            placeholder: truncate(&placeholder, 80),
        },
        InputKind::Toggle { default, on, off } => InputKind::Toggle {
            default,
            on: on.chars().take(MAX_VALUE).collect(),
            off: off.chars().take(MAX_VALUE).collect(),
        },
    };
    ActionInput { id, label, kind }
}

fn sanitize_step(step: ActionStep) -> ActionStep {
    let text = |s: String| -> String { s.chars().take(MAX_TEXT).collect() };
    match step {
        ActionStep::Keys { key, modifiers } => {
            let mut mods = Vec::new();
            for m in modifiers {
                if !mods.contains(&m) {
                    mods.push(m);
                }
            }
            ActionStep::Keys {
                key,
                modifiers: mods,
            }
        }
        ActionStep::Text { text: t } => ActionStep::Text { text: text(t) },
        ActionStep::Run {
            program,
            args,
            cwd,
            hidden,
        } => ActionStep::Run {
            program: text(program).trim().to_string(),
            args: args.into_iter().take(MAX_ARGS).map(text).collect(),
            cwd: text(cwd).trim().to_string(),
            hidden,
        },
        ActionStep::Open { target } => ActionStep::Open {
            target: text(target).trim().to_string(),
        },
        ActionStep::Delay { ms } => ActionStep::Delay {
            ms: ms.min(MAX_DELAY_MS),
        },
    }
}

impl Deck {
    /// Ramène le panneau dans des limites sûres : tailles de grille, longueurs, identifiants
    /// uniques (complétés par `new_id`), champs cohérents.
    pub fn sanitized(self, new_id: &mut dyn FnMut() -> String) -> Deck {
        let mut page_ids = HashSet::new();
        let mut button_ids = HashSet::new();
        let mut fresh = |taken: &mut HashSet<String>, wanted: &str| -> String {
            let id = if valid_id(wanted) && !taken.contains(wanted) {
                wanted.to_string()
            } else {
                loop {
                    let candidate = new_id();
                    if valid_id(&candidate) && !taken.contains(&candidate) {
                        break candidate;
                    }
                }
            };
            taken.insert(id.clone());
            id
        };
        let pages = self
            .pages
            .into_iter()
            .take(MAX_PAGES)
            .enumerate()
            .map(|(index, page)| {
                let columns = page.columns.clamp(MIN_COLUMNS, MAX_COLUMNS);
                let rows = page.rows.clamp(MIN_ROWS, MAX_ROWS);
                let mut cells: Vec<Option<ActionButton>> = page
                    .cells
                    .into_iter()
                    .take(columns as usize * rows as usize)
                    .map(|cell| {
                        cell.map(|b| {
                            let mut taken = HashSet::new();
                            ActionButton {
                                id: fresh(&mut button_ids, &b.id),
                                label: truncate(&b.label, MAX_LABEL),
                                icon: truncate(&b.icon, MAX_ICON),
                                color: b.color,
                                inputs: b
                                    .inputs
                                    .into_iter()
                                    .take(MAX_INPUTS)
                                    .enumerate()
                                    .map(|(i, input)| sanitize_input(input, i, &mut taken))
                                    .collect(),
                                steps: b
                                    .steps
                                    .into_iter()
                                    .take(MAX_STEPS)
                                    .map(sanitize_step)
                                    .collect(),
                            }
                        })
                    })
                    .collect();
                cells.resize(columns as usize * rows as usize, None);
                let name = match truncate(&page.name, MAX_LABEL) {
                    n if n.is_empty() => format!("Page {}", index + 1),
                    n => n,
                };
                DeckPage {
                    id: fresh(&mut page_ids, &page.id),
                    name,
                    columns,
                    rows,
                    cells,
                }
            })
            .collect();
        Deck { pages }
    }

    pub fn button(&self, id: &str) -> Option<&ActionButton> {
        self.pages
            .iter()
            .flat_map(|p| p.cells.iter().flatten())
            .find(|b| b.id == id)
    }

    pub fn view(&self) -> DeckView {
        DeckView {
            pages: self
                .pages
                .iter()
                .map(|p| PageView {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    columns: p.columns,
                    rows: p.rows,
                    cells: p
                        .cells
                        .iter()
                        .map(|c| c.as_ref().map(ActionButton::view))
                        .collect(),
                })
                .collect(),
        }
    }

    /// Panneau proposé au premier lancement : de quoi voir le principe et l'adapter.
    pub fn example() -> Deck {
        fn keys(
            id: &str,
            label: &str,
            icon: &str,
            key: Key,
            modifiers: &[Modifier],
        ) -> ActionButton {
            ActionButton {
                id: id.into(),
                label: label.into(),
                icon: icon.into(),
                color: ButtonColor::Glass,
                inputs: vec![],
                steps: vec![ActionStep::Keys {
                    key,
                    modifiers: modifiers.to_vec(),
                }],
            }
        }
        fn tinted(mut b: ActionButton, color: ButtonColor) -> ActionButton {
            b.color = color;
            b
        }
        let video = vec![
            Some(tinted(
                keys("slower", "Ralentir", "🐢", Key::Char('s'), &[]),
                ButtonColor::Blue,
            )),
            Some(tinted(
                keys("faster", "Accélérer", "🐇", Key::Char('d'), &[]),
                ButtonColor::Amber,
            )),
            Some(keys("key_q", "Touche Q", "Q", Key::Char('q'), &[])),
            Some(keys(
                "play",
                "Lecture / pause",
                "⏯",
                Key::MediaPlayPause,
                &[],
            )),
            Some(keys("fullscreen", "Plein écran", "⛶", Key::Char('f'), &[])),
            Some(keys("mute", "Muet", "🔇", Key::VolumeMute, &[])),
        ];
        let shortcuts = vec![
            Some(keys(
                "copy",
                "Copier",
                "⧉",
                Key::Char('c'),
                &[Modifier::Ctrl],
            )),
            Some(keys(
                "paste",
                "Coller",
                "📋",
                Key::Char('v'),
                &[Modifier::Ctrl],
            )),
            Some(keys(
                "undo",
                "Annuler",
                "↶",
                Key::Char('z'),
                &[Modifier::Ctrl],
            )),
            Some(keys(
                "desktop",
                "Bureau",
                "🖥",
                Key::Char('d'),
                &[Modifier::Meta],
            )),
            Some(keys(
                "capture",
                "Capture",
                "✂",
                Key::Char('s'),
                &[Modifier::Meta, Modifier::Shift],
            )),
            Some(ActionButton {
                id: "youtube".into(),
                label: "Chercher sur YouTube".into(),
                icon: "🔎".into(),
                color: ButtonColor::Red,
                inputs: vec![ActionInput {
                    id: "recherche".into(),
                    label: "Recherche".into(),
                    kind: InputKind::Text {
                        default: String::new(),
                        placeholder: "Titre, chaîne…".into(),
                    },
                }],
                steps: vec![ActionStep::Open {
                    target: "https://www.youtube.com/results?search_query={recherche:url}".into(),
                }],
            }),
        ];
        Deck {
            pages: vec![
                DeckPage {
                    id: "video".into(),
                    name: "Vidéo".into(),
                    columns: 3,
                    rows: 2,
                    cells: video,
                },
                DeckPage {
                    id: "shortcuts".into(),
                    name: "Raccourcis".into(),
                    columns: 3,
                    rows: 2,
                    cells: shortcuts,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn input(id: &str, kind: InputKind) -> ActionInput {
        ActionInput {
            id: id.into(),
            label: id.to_uppercase(),
            kind,
        }
    }

    fn button(inputs: Vec<ActionInput>, steps: Vec<ActionStep>) -> ActionButton {
        ActionButton {
            id: "b".into(),
            label: "B".into(),
            icon: String::new(),
            color: ButtonColor::Glass,
            inputs,
            steps,
        }
    }

    fn counter() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("id{n}")
        }
    }

    #[test]
    fn render_replaces_fields_and_keeps_the_rest() {
        let v = values(&[("x", "1.5"), ("q", "chats & chiens")]);
        assert_eq!(render("--speed={x}", &v), "--speed=1.5");
        assert_eq!(render("?q={q:url}", &v), "?q=chats%20%26%20chiens");
        assert_eq!(render("{{x}} {inconnu} {x", &v), "{x} {inconnu} {x");
        assert_eq!(render("}} {x:autre} {}", &v), "} {x:autre} {}");
        assert_eq!(render("aucun champ", &v), "aucun champ");
    }

    #[test]
    fn url_encoding_handles_unicode() {
        assert_eq!(url_encode("é/ ?"), "%C3%A9%2F%20%3F");
        assert_eq!(url_encode("a-b_c.d~"), "a-b_c.d~");
    }

    #[test]
    fn numbers_are_written_without_noise() {
        assert_eq!(format_number(2.0), "2");
        assert_eq!(format_number(1.25), "1.25");
        assert_eq!(format_number(-0.5), "-0.5");
        assert_eq!(format_number(0.1 + 0.2), "0.3");
    }

    #[test]
    fn choices_only_accept_declared_values() {
        let i = input(
            "site",
            InputKind::Choice {
                options: vec![
                    ChoiceOption {
                        label: "A".into(),
                        value: "a".into(),
                    },
                    ChoiceOption {
                        label: "B".into(),
                        value: "b".into(),
                    },
                ],
                default: "b".into(),
            },
        );
        assert_eq!(i.default_value(), "b");
        assert_eq!(i.validate(&InputValue::Text("a".into())).unwrap(), "a");
        assert!(i.validate(&InputValue::Text("c; rm -rf".into())).is_err());
        assert!(i.validate(&InputValue::Number(1.0)).is_err());
    }

    #[test]
    fn sliders_clamp_to_range_and_snap_to_step() {
        let i = input(
            "v",
            InputKind::Slider {
                min: 0.5,
                max: 4.0,
                step: 0.25,
                default: 1.0,
                unit: "×".into(),
            },
        );
        assert_eq!(i.validate(&InputValue::Number(1.3)).unwrap(), "1.25");
        assert_eq!(i.validate(&InputValue::Text("2,5".into())).unwrap(), "2.5");
        assert!(i.validate(&InputValue::Number(5.0)).is_err());
        assert!(i.validate(&InputValue::Number(f64::NAN)).is_err());
        assert_eq!(i.default_value(), "1");
    }

    #[test]
    fn numbers_respect_bounds_and_integers() {
        let i = input(
            "n",
            InputKind::Number {
                min: Some(1.0),
                max: None,
                default: 3.0,
                integer: true,
            },
        );
        assert_eq!(i.validate(&InputValue::Number(12.0)).unwrap(), "12");
        assert!(i.validate(&InputValue::Number(0.0)).is_err());
        assert!(i.validate(&InputValue::Number(2.5)).is_err());
        assert!(i.validate(&InputValue::Text("abc".into())).is_err());
    }

    #[test]
    fn text_and_toggle_values() {
        let t = input(
            "t",
            InputKind::Text {
                default: "x".into(),
                placeholder: String::new(),
            },
        );
        assert_eq!(
            t.validate(&InputValue::Text("Ça va".into())).unwrap(),
            "Ça va"
        );
        assert!(t
            .validate(&InputValue::Text("a".repeat(MAX_VALUE + 1)))
            .is_err());
        assert!(t.validate(&InputValue::Text("a\0b".into())).is_err());

        let g = input(
            "g",
            InputKind::Toggle {
                default: true,
                on: "--on".into(),
                off: String::new(),
            },
        );
        assert_eq!(g.default_value(), "--on");
        assert_eq!(g.validate(&InputValue::Bool(false)).unwrap(), "");
        assert!(g.validate(&InputValue::Text("true".into())).is_err());
    }

    #[test]
    fn plan_validates_values_and_fills_templates() {
        let b = button(
            vec![input(
                "speed",
                InputKind::Slider {
                    min: 0.0,
                    max: 4.0,
                    step: 0.5,
                    default: 1.0,
                    unit: String::new(),
                },
            )],
            vec![
                ActionStep::Keys {
                    key: Key::Char('s'),
                    modifiers: vec![],
                },
                ActionStep::Run {
                    program: " powershell.exe ".into(),
                    args: vec!["-File".into(), "speed.ps1".into(), "{speed}".into()],
                    cwd: String::new(),
                    hidden: true,
                },
                ActionStep::Delay { ms: 99_999 },
                ActionStep::Text {
                    text: "{inutile_vide}".into(),
                },
            ],
        );
        let mut v = BTreeMap::new();
        v.insert("speed".to_string(), InputValue::Number(2.0));
        assert_eq!(
            b.plan(&v).unwrap(),
            vec![
                PlannedStep::Keys {
                    key: Key::Char('s'),
                    modifiers: vec![]
                },
                PlannedStep::Run {
                    program: "powershell.exe".into(),
                    args: vec!["-File".into(), "speed.ps1".into(), "2".into()],
                    cwd: None,
                    hidden: true,
                },
                PlannedStep::Delay(MAX_DELAY_MS),
                PlannedStep::Text("{inutile_vide}".into()),
            ]
        );
        // Sans valeur envoyée : la valeur par défaut.
        assert!(matches!(
            &b.plan(&BTreeMap::new()).unwrap()[1],
            PlannedStep::Run { args, .. } if args[2] == "1"
        ));
    }

    #[test]
    fn a_value_stays_a_single_argument() {
        let b = button(
            vec![input(
                "t",
                InputKind::Text {
                    default: String::new(),
                    placeholder: String::new(),
                },
            )],
            vec![ActionStep::Run {
                program: "notepad.exe".into(),
                args: vec!["{t}".into()],
                cwd: String::new(),
                hidden: false,
            }],
        );
        let mut v = BTreeMap::new();
        v.insert("t".into(), InputValue::Text("a.txt & calc.exe".into()));
        assert_eq!(
            b.plan(&v).unwrap(),
            vec![PlannedStep::Run {
                program: "notepad.exe".into(),
                args: vec!["a.txt & calc.exe".into()],
                cwd: None,
                hidden: false
            }]
        );
    }

    #[test]
    fn plan_rejects_unknown_fields_and_broken_steps() {
        let b = button(
            vec![],
            vec![ActionStep::Keys {
                key: Key::Enter,
                modifiers: vec![],
            }],
        );
        let mut v = BTreeMap::new();
        v.insert("program".into(), InputValue::Text("cmd".into()));
        assert_eq!(b.plan(&v), Err(ActionError::UnknownInput("program".into())));
        assert_eq!(
            button(vec![], vec![]).plan(&BTreeMap::new()),
            Err(ActionError::Empty)
        );
        let run = button(
            vec![],
            vec![ActionStep::Run {
                program: " ".into(),
                args: vec![],
                cwd: String::new(),
                hidden: false,
            }],
        );
        assert_eq!(run.plan(&BTreeMap::new()), Err(ActionError::MissingProgram));
    }

    #[test]
    fn sanitize_fixes_grid_sizes_ids_and_fields() {
        let dup = button(
            vec![
                input(
                    "a b",
                    InputKind::Slider {
                        min: 10.0,
                        max: 0.0,
                        step: -1.0,
                        default: 50.0,
                        unit: String::new(),
                    },
                ),
                input(
                    "x",
                    InputKind::Choice {
                        options: vec![],
                        default: "z".into(),
                    },
                ),
                input(
                    "x",
                    InputKind::Text {
                        default: String::new(),
                        placeholder: String::new(),
                    },
                ),
            ],
            vec![ActionStep::Delay { ms: u32::MAX }],
        );
        let deck = Deck {
            pages: vec![
                DeckPage {
                    id: String::new(),
                    name: "  ".into(),
                    columns: 9,
                    rows: 0,
                    cells: vec![Some(dup.clone()), Some(dup)],
                },
                DeckPage {
                    id: "p".into(),
                    name: "Deux".into(),
                    columns: 2,
                    rows: 2,
                    cells: (0..10).map(|_| None).collect(),
                },
            ],
        }
        .sanitized(&mut counter());

        let p0 = &deck.pages[0];
        assert_eq!((p0.columns, p0.rows, p0.cells.len()), (MAX_COLUMNS, 1, 5));
        assert_eq!(p0.name, "Page 1");
        assert!(!p0.id.is_empty());
        let b0 = p0.cells[0].as_ref().unwrap();
        let b1 = p0.cells[1].as_ref().unwrap();
        assert_ne!(b0.id, b1.id, "identifiants uniques");
        let ids: Vec<&str> = b0.inputs.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["champ1", "x", "champ3"]);
        match &b0.inputs[0].kind {
            InputKind::Slider {
                min,
                max,
                step,
                default,
                ..
            } => {
                assert_eq!((*min, *max, *default), (0.0, 10.0, 10.0));
                assert!(*step > 0.0);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(b0.steps, vec![ActionStep::Delay { ms: MAX_DELAY_MS }]);
        assert_eq!(deck.pages[1].cells.len(), 4);
        assert_eq!(deck.pages[1].id, "p");
    }

    #[test]
    fn sanitize_is_idempotent_on_the_example() {
        let ex = Deck::example();
        assert_eq!(ex.clone().sanitized(&mut counter()), ex);
    }

    #[test]
    fn view_hides_the_steps() {
        let deck = Deck::example();
        let json = serde_json::to_string(&deck.view()).unwrap();
        assert!(!json.contains("steps"));
        assert!(!json.contains("search_query"));
        assert!(json.contains("Ralentir"));
        assert!(deck.button("slower").is_some());
        assert!(deck.button("nope").is_none());
    }

    #[test]
    fn json_shape_is_stable() {
        let step: ActionStep = serde_json::from_value(json!({"kind": "keys", "key": "s"})).unwrap();
        assert_eq!(
            step,
            ActionStep::Keys {
                key: Key::Char('s'),
                modifiers: vec![]
            }
        );
        let kind: InputKind = serde_json::from_value(json!({"type": "toggle"})).unwrap();
        assert_eq!(
            kind,
            InputKind::Toggle {
                default: false,
                on: "true".into(),
                off: "false".into()
            }
        );
        let v: BTreeMap<String, InputValue> =
            serde_json::from_value(json!({"a": true, "b": 2.5, "c": "x"})).unwrap();
        assert_eq!(v["a"], InputValue::Bool(true));
        assert_eq!(v["b"], InputValue::Number(2.5));
        assert_eq!(v["c"], InputValue::Text("x".into()));
    }
}
