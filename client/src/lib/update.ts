/**
 * Force le téléchargement de la dernière version : supprime le service worker et ses caches,
 * puis recharge. L'identité de l'appareil (localStorage) est conservée : pas besoin de ré-associer le PC.
 */
export async function forceUpdate(): Promise<void> {
  try {
    const regs = (await navigator.serviceWorker?.getRegistrations?.()) ?? [];
    await Promise.all(regs.map((r) => r.unregister()));
    const keys = (await globalThis.caches?.keys?.()) ?? [];
    await Promise.all(keys.map((k) => caches.delete(k)));
  } catch {
    /* on recharge quand même */
  }
  location.reload();
}
