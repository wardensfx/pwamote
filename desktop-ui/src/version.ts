import type { UpdateInfo } from "../../client/src/generated/UpdateInfo";

/** Numéro discret affiché à côté du titre. */
export function versionText(info: UpdateInfo): string {
  return info.official ? `v${info.current}` : `v${info.current} · dev`;
}

/** Info-bulle du numéro de version. */
export function versionHint(info: UpdateInfo): string {
  if (!info.official) return "Version compilée localement : les mises à jour ne sont pas vérifiées.";
  if (info.available) return `PwaMote ${info.available.version} est disponible (vous avez ${info.current}).`;
  return `PwaMote ${info.current} · à jour, vérifié régulièrement sur GitHub.`;
}

/** Texte de la pastille de mise à jour, ou `null` s'il n'y a rien à proposer. */
export function updateLabel(info: UpdateInfo): string | null {
  return info.official && info.available ? `Mise à jour ${info.available.version}` : null;
}
