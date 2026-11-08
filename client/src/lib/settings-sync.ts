import type { Settings } from "../generated/Settings";
import type { VersionedSettings } from "../generated/VersionedSettings";

export type Edit = (s: Settings) => Settings;

/**
 * Synchronisation des réglages avec le serveur (source de vérité).
 *
 * Les modifications locales sont des fonctions rejouées par-dessus la dernière
 * version du serveur : si un autre téléphone modifie autre chose en même temps,
 * les deux changements sont conservés. Converge quel que soit l'ordre des messages.
 */
export class SettingsSync {
  private server: VersionedSettings | null = null;
  private edits: { edit: Edit; key: string | null }[] = [];
  private inFlight = false;

  constructor(private readonly onChange: (s: Settings) => void) {}

  /** Réglages à afficher : serveur + modifications en attente. */
  get current(): Settings | null {
    return this.server ? this.edits.reduce((s, e) => e.edit(s), this.server.settings) : null;
  }

  get rev(): number | null {
    return this.server?.rev ?? null;
  }

  get pending(): boolean {
    return this.edits.length > 0;
  }

  /**
   * Enregistre une modification locale. Avec une `key`, elle remplace toute
   * modification précédente de la même clé : glisser un curseur ne garde que
   * la dernière valeur, jamais l'historique.
   */
  edit(edit: Edit, key: string | null = null): void {
    if (key !== null) this.edits = this.edits.filter((e) => e.key !== key);
    this.edits.push({ edit, key });
    this.emit();
  }

  /** Message à envoyer au serveur, ou `null` s'il n'y a rien à faire. */
  flush(): { base_rev: number; settings: Settings } | null {
    if (!this.server || this.inFlight || this.edits.length === 0) return null;
    this.inFlight = true;
    return { base_rev: this.server.rev, settings: this.current! };
  }

  /** Le serveur a envoyé (ou confirmé) un état. Renvoie une nouvelle mise à jour si nécessaire. */
  onServer(v: VersionedSettings): { base_rev: number; settings: Settings } | null {
    if (this.server && v.rev < this.server.rev) return null;
    // Même révision, rien en vol : c'est l'écho d'une diffusion déjà traitée.
    if (this.server && v.rev === this.server.rev && !this.inFlight) return null;
    this.server = v;
    this.inFlight = false;
    // Une modification déjà reflétée par le serveur n'a plus besoin d'être rejouée.
    // On ne retire que celle-là : les modifications qui la suivent restent en attente.
    this.edits = this.edits.filter((e) => !same(e.edit(v.settings), v.settings));
    this.emit();
    return this.flush();
  }

  /** La connexion est perdue : la prochaine mise à jour pourra repartir. */
  onDisconnect(): void {
    this.inFlight = false;
  }

  private emit(): void {
    const c = this.current;
    if (c) this.onChange(c);
  }
}

const same = (a: Settings, b: Settings) => JSON.stringify(a) === JSON.stringify(b);
