// Injecte une version (issue d'un tag `v1.2.3`) dans tous les manifestes du dépôt.
// Utilisé par le workflow de release ; `node scripts/set-version.mjs v1.2.3` en local fonctionne aussi.
import { readFileSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// SemVer 2.0 (https://semver.org), préfixe `v` toléré.
const SEMVER =
  /^v?((0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?)$/;

export function parseVersion(tag) {
  const m = SEMVER.exec(String(tag).trim().replace(/^refs\/tags\//, ""));
  if (!m) throw new Error(`Version invalide : « ${tag} » (attendu v1.2.3 ou v1.2.3-beta.1)`);
  return m[1];
}

/** Remplace `version = "…"` de la section `[workspace.package]` d'un Cargo.toml. */
export function setCargoWorkspaceVersion(toml, version) {
  const re = /(\[workspace\.package\][^[]*?\nversion\s*=\s*")[^"]*(")/;
  if (!re.test(toml)) throw new Error("Cargo.toml : version de [workspace.package] introuvable");
  return toml.replace(re, `$1${version}$2`);
}

/** Remplace le champ `version` de premier niveau d'un JSON en gardant sa mise en forme. */
export function setJsonVersion(json, version) {
  const data = JSON.parse(json);
  if (typeof data.version !== "string") throw new Error("champ \"version\" absent");
  const re = /^(\s*"version"\s*:\s*")[^"]*(")/m;
  return json.replace(re, `$1${version}$2`);
}

export const FILES = [
  ["Cargo.toml", setCargoWorkspaceVersion],
  ["crates/pwamote-desktop/tauri.conf.json", setJsonVersion],
  ["client/package.json", setJsonVersion],
  ["desktop-ui/package.json", setJsonVersion],
];

export function applyVersion(root, tag) {
  const version = parseVersion(tag);
  for (const [file, set] of FILES) {
    const path = join(root, file);
    writeFileSync(path, set(readFileSync(path, "utf8"), version));
  }
  return version;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  // `--print v1.2.3` valide et affiche la version sans rien modifier.
  const [first, second] = process.argv.slice(2);
  if (first === "--print") {
    console.log(parseVersion(second ?? ""));
  } else {
    const root = join(dirname(fileURLToPath(import.meta.url)), "..");
    console.log(applyVersion(root, first ?? ""));
  }
}
