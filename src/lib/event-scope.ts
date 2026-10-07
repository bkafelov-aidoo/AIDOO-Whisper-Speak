import type { EventCallback, EventName, UnlistenFn, listen } from "@tauri-apps/api/event";

/** Consumes a native pending latch only while its owning effect is still active. */
export async function consumePendingRequest(
  takePending: () => Promise<boolean>,
  onPending: () => Promise<void> | void,
  isActive: () => boolean,
) {
  if (!isActive()) return;
  if (!await takePending() || !isActive()) return;
  await onPending();
}

/** Owns async native subscriptions for one React effect, including late registrations. */
export function createEventScope(subscribe: typeof listen, onError: (reason: unknown) => void) {
  let disposed = false;
  const subscriptions = new Set<UnlistenFn>();

  return {
    listen<T>(event: EventName, handler: EventCallback<T>, onReady?: () => void) {
      if (disposed) return;
      void subscribe<T>(event, (message) => {
        if (!disposed) handler(message);
      }).then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        subscriptions.add(unlisten);
        onReady?.();
      }).catch((reason: unknown) => {
        if (!disposed) onError(reason);
      });
    },
    dispose() {
      disposed = true;
      for (const unlisten of subscriptions) unlisten();
      subscriptions.clear();
    },
  };
}
