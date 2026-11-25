import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { normalizeSnapshot, type WireAppSnapshot } from './snapshot';
import type { AppSnapshot, LogEntry, Settings, UpdateProgressEvent } from './types';

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function subscribeBackendEvents(
  onSnapshot: (snap: AppSnapshot) => void,
  onLog: (entry: LogEntry) => void,
  onUpdateProgress?: (progress: UpdateProgressEvent) => void,
  onSettings?: (settings: Settings) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }

  const unlisteners: UnlistenFn[] = [];
  unlisteners.push(
    await listen<WireAppSnapshot>('dropilot://snapshot', (ev) => {
      onSnapshot(normalizeSnapshot(ev.payload));
    }),
  );
  unlisteners.push(
    await listen<LogEntry>('dropilot://log', (ev) => {
      onLog(ev.payload);
    }),
  );
  if (onUpdateProgress) {
    unlisteners.push(
      await listen<UpdateProgressEvent>('dropilot://update-progress', (ev) => {
        onUpdateProgress(ev.payload);
      }),
    );
  }
  if (onSettings) {
    unlisteners.push(
      await listen<Settings>('dropilot://settings', (ev) => {
        onSettings(ev.payload);
      }),
    );
  }

  return () => {
    for (const unlisten of unlisteners) {
      unlisten();
    }
  };
}
