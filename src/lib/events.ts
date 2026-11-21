import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AppSnapshot, LogEntry } from './types';

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function subscribeBackendEvents(
  onSnapshot: (snap: AppSnapshot) => void,
  onLog: (entry: LogEntry) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }

  const unlisteners: UnlistenFn[] = [];
  unlisteners.push(
    await listen<AppSnapshot>('droppilot://snapshot', (ev) => {
      onSnapshot(ev.payload);
    }),
  );
  unlisteners.push(
    await listen<LogEntry>('droppilot://log', (ev) => {
      onLog(ev.payload);
    }),
  );

  return () => {
    for (const unlisten of unlisteners) {
      unlisten();
    }
  };
}
