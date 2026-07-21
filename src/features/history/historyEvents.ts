/** Dispatched when request history rows change (send success or purge). */
export const HISTORY_CHANGED_EVENT = "conductor:history-changed";

export function notifyHistoryChanged() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new Event(HISTORY_CHANGED_EVENT));
}
