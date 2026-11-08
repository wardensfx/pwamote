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
  private edits: Edit[] = [];
  private inFlight = false;

  constructor(private readonly onChange: (s: Settings) => void) {}

  /** Réglages à afficher : serveur + modifications en attente. */
  get current(): Settings | null {
    return this.server ? this.edits.reduce((s, e) => e(s), this.server.settings) : null;
  }

  get rev(): number | null {
    return this.server?.rev ?? null;
  }

  get pending(): boolean {
    return this.edits.length > 0;
  }

  edit(edit: Edit): void {
    this.edits.push(edit);
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
    this.server = v;
    this.inFlight = false;
    // Une modification déjà reflétée par le serveur n'a plus besoin d'être rejouée.
    this.edits = this.edits.filter((e) => !same(e(v.settings), v.settings));
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
