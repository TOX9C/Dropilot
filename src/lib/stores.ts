import { writable } from 'svelte/store';
import { api } from './api';
import { subscribeBackendEvents } from './events';
import type {
  AppSnapshot,
  Campaign,
  DropsViewMode,
  LogEntry,
  NavRoute,
  PlatformId,
  Settings,
  ToastItem,
} from './types';

function readBool(key: string, fallback: boolean): boolean {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === 'true';
  } catch {
    return fallback;
  }
}

function readDropsViewMode(): DropsViewMode {
  try {
    const v = localStorage.getItem('droppilot.dropsViewMode');
    return v === 'compact' ? 'compact' : 'grid';
  } catch {
    return 'grid';
  }
}

function readRoute(): NavRoute {
  try {
    const v = localStorage.getItem('droppilot.route');
    if (
      v === 'dashboard' ||
      v === 'drops' ||
      v === 'twitch' ||
      v === 'kick' ||
      v === 'logs' ||
      v === 'settings'
    ) {
      return v;
    }
  } catch {
    // ignore storage errors
  }
  return 'dashboard';
}

export const activeRoute = writable<NavRoute>(readRoute());
export const sidebarCollapsed = writable<boolean>(readBool('droppilot.sidebarCollapsed', false));
export const checklistDismissed = writable<boolean>(readBool('droppilot.checklistDismissed', false));
export const dropsViewMode = writable<DropsViewMode>(readDropsViewMode());
export const shortcutsOpen = writable<boolean>(false);

export const appSnapshot = writable<AppSnapshot | null>(null);
export const appSettings = writable<Settings | null>(null);
export const appLogs = writable<LogEntry[]>([]);
export const mockFeatureAvailable = writable<boolean>(false);
export const lastSnapshotAt = writable<number>(Date.now());
export const sessionMinutes = writable<number>(0);
export const sessionHistory = writable<number[]>([0]);
export const unreadAlertCount = writable<number>(0);
export const uiToasts = writable<ToastItem[]>([]);

let currentRoute: NavRoute = readRoute();
let nextToastId = 1;
const previousDropMinutes = new Map<string, number>();

activeRoute.subscribe((route) => {
  currentRoute = route;
  try {
    localStorage.setItem('droppilot.route', route);
  } catch {
    // ignore
  }
  if (route === 'logs') {
    unreadAlertCount.set(0);
  }
});

sidebarCollapsed.subscribe((collapsed) => {
  try {
    localStorage.setItem('droppilot.sidebarCollapsed', String(collapsed));
  } catch {
    // ignore
  }
});

checklistDismissed.subscribe((dismissed) => {
  try {
    localStorage.setItem('droppilot.checklistDismissed', String(dismissed));
  } catch {
    // ignore
  }
});

dropsViewMode.subscribe((mode) => {
  try {
    localStorage.setItem('droppilot.dropsViewMode', mode);
  } catch {
    // ignore
  }
});

export function notifyUi(message: string, kind: 'info' | 'error' = 'info'): void {
  const id = nextToastId++;
  uiToasts.update((list) => [...list, { id, message, kind }].slice(-3));
  setTimeout(() => {
    dismissToast(id);
  }, 4000);
}

export function dismissToast(id: number): void {
  uiToasts.update((list) => list.filter((t) => t.id !== id));
}

export function clearLogsView(): void {
  appLogs.set([]);
  unreadAlertCount.set(0);
}

export async function toggleCampaignFarmAction(
  camp: Campaign,
  farm: boolean,
  hasAccount: boolean,
): Promise<void> {
  let updated = await api.setCampaignFarm(camp.platform, camp.id, farm);
  if (farm) {
    const exists = updated[camp.platform].priority.some(
      (i) => i.kind === 'campaign' && i.campaignId === camp.id,
    );
    if (!exists) {
      updated = await api.setPriority(camp.platform, [
        ...updated[camp.platform].priority,
        { kind: 'campaign', campaignId: camp.id, campaignName: camp.name },
      ]);
    }
    if (!hasAccount) {
      const platName = camp.platform === 'twitch' ? 'Twitch' : 'Kick';
      notifyUi(`Saved! Log in to ${platName} to start farming this campaign.`);
    }
  }
  appSettings.set(updated);
}

export async function toggleGameFarmAction(
  camp: Campaign,
  farm: boolean,
  hasAccount: boolean,
): Promise<void> {
  const updated = await api.setGameFarm(camp.platform, camp.game.id, camp.game.name, farm);
  if (farm && !hasAccount) {
    const platName = camp.platform === 'twitch' ? 'Twitch' : 'Kick';
    notifyUi(`Saved! Log in to ${platName} to start farming ${camp.game.name}.`);
  }
  appSettings.set(updated);
}

export async function addWatchedGameAction(
  gameName: string,
  target: 'twitch' | 'kick' | 'both',
  snapshot: AppSnapshot | null,
): Promise<void> {
  const trimmed = gameName.trim();
  if (!trimmed) return;
  const platforms: PlatformId[] = target === 'both' ? ['twitch', 'kick'] : [target];
  const lower = trimmed.toLowerCase();
  let latest: Settings | null = null;

  for (const plat of platforms) {
    const match = snapshot?.[plat].campaigns.find(
      (c) => c.game.name.trim().toLowerCase() === lower,
    );
    const gameId = match?.game.id ?? '';
    const canonicalName = match?.game.name ?? trimmed;
    latest = await api.setGameFarm(plat, gameId, canonicalName, true);
  }

  if (latest) {
    appSettings.set(latest);
  }

  const missingAccount = platforms.some((p) => !snapshot?.[p].account);
  notifyUi(
    missingAccount
      ? `Added "${trimmed}" to Auto-Mine Watchlist. Log in to auto-farm when a drop goes live.`
      : `Watching "${trimmed}" — will auto-mine whenever a drop goes live.`,
  );
}

export function updateSnapshotCampaign(updated: Campaign): void {
  appSnapshot.update((snap) => {
    if (!snap) return snap;
    const platSnap = snap[updated.platform];
    const idx = platSnap.campaigns.findIndex((c) => c.id === updated.id);
    if (idx === -1) return snap;
    const nextCampaigns = [...platSnap.campaigns];
    nextCampaigns[idx] = updated;
    return {
      ...snap,
      [updated.platform]: {
        ...platSnap,
        campaigns: nextCampaigns,
      },
    };
  });
}

function trackSessionProgress(snap: AppSnapshot, isBaseline: boolean) {
  let delta = 0;
  for (const plat of ['twitch', 'kick'] as const) {
    for (const camp of snap[plat].campaigns) {
      for (const drop of camp.drops) {
        const key = `${plat}:${camp.id}:${drop.id}`;
        const prev = previousDropMinutes.get(key);
        if (!isBaseline && prev !== undefined && drop.currentMinutes > prev) {
          delta += drop.currentMinutes - prev;
        }
        previousDropMinutes.set(key, drop.currentMinutes);
      }
    }
  }
  if (delta > 0) {
    sessionMinutes.update((total) => {
      const next = total + delta;
      sessionHistory.update((hist) => [...hist, next].slice(-60));
      return next;
    });
  }
}

export async function initializeAppStores(): Promise<() => void> {
  if (typeof navigator !== 'undefined' && navigator.userAgent) {
    void api.reportUserAgent(navigator.userAgent).catch(() => {});
  }

  const [snap, settings, logs, hasMock] = await Promise.all([
    api.getSnapshot(),
    api.getSettings(),
    api.getLogs(),
    api.isMockAvailable(),
  ]);

  trackSessionProgress(snap, true);
  appSnapshot.set(snap);
  appSettings.set(settings);
  appLogs.set(logs);
  mockFeatureAvailable.set(hasMock);
  lastSnapshotAt.set(Date.now());

  const unlisten = await subscribeBackendEvents(
    (nextSnap) => {
      trackSessionProgress(nextSnap, false);
      appSnapshot.set(nextSnap);
      lastSnapshotAt.set(Date.now());
    },
    (entry) => {
      appLogs.update((prev) => {
        const next = [entry, ...prev];
        return next.length > 1000 ? next.slice(0, 1000) : next;
      });
      if (currentRoute !== 'logs' && (entry.level === 'warn' || entry.level === 'error')) {
        unreadAlertCount.update((c) => c + 1);
      }
    },
  );

  return unlisten;
}
