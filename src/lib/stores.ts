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
  RemoteConfigStatus,
  Settings,
  ToastItem,
  UpdateProgressEvent,
  UpdateStatus,
} from './types';

function readStorage(key: string, legacyKey: string): string | null {
  try {
    const v = localStorage.getItem(key);
    if (v !== null) return v;
    const legacy = localStorage.getItem(legacyKey);
    if (legacy !== null) {
      localStorage.setItem(key, legacy);
      return legacy;
    }
    return null;
  } catch {
    return null;
  }
}

function readBool(key: string, legacyKey: string, fallback: boolean): boolean {
  const v = readStorage(key, legacyKey);
  return v === null ? fallback : v === 'true';
}

function readDropsViewMode(): DropsViewMode {
  const v = readStorage('dropilot.dropsViewMode', 'droppilot.dropsViewMode');
  return v === 'compact' ? 'compact' : 'grid';
}

function readRoute(): NavRoute {
  const v = readStorage('dropilot.route', 'droppilot.route');
  if (
    v === 'dashboard' ||
    v === 'drops' ||
    v === 'history' ||
    v === 'twitch' ||
    v === 'kick' ||
    v === 'logs' ||
    v === 'settings'
  ) {
    return v;
  }
  return 'dashboard';
}

export const activeRoute = writable<NavRoute>(readRoute());
export const sidebarCollapsed = writable<boolean>(
  readBool('dropilot.sidebarCollapsed', 'droppilot.sidebarCollapsed', false),
);
export const checklistDismissed = writable<boolean>(
  readBool('dropilot.checklistDismissed', 'droppilot.checklistDismissed', false),
);
export const dropsViewMode = writable<DropsViewMode>(readDropsViewMode());
export const shortcutsOpen = writable<boolean>(false);

export const appSnapshot = writable<AppSnapshot | null>(null);
export const appSettings = writable<Settings | null>(null);
export const appLogs = writable<LogEntry[]>([]);
export const remoteConfigStatus = writable<RemoteConfigStatus | null>(null);
export const updateInfo = writable<UpdateStatus | null>(null);
export const updateProgress = writable<UpdateProgressEvent | null>(null);
export const updateBannerDismissed = writable<boolean>(false);
export const checkingForUpdate = writable<boolean>(false);
export const installingUpdate = writable<boolean>(false);
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
    localStorage.setItem('dropilot.route', route);
  } catch {
    // ignore
  }
  if (route === 'logs') {
    unreadAlertCount.set(0);
  }
});

sidebarCollapsed.subscribe((collapsed) => {
  try {
    localStorage.setItem('dropilot.sidebarCollapsed', String(collapsed));
  } catch {
    // ignore
  }
});

checklistDismissed.subscribe((dismissed) => {
  try {
    localStorage.setItem('dropilot.checklistDismissed', String(dismissed));
  } catch {
    // ignore
  }
});

dropsViewMode.subscribe((mode) => {
  try {
    localStorage.setItem('dropilot.dropsViewMode', mode);
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

export async function checkForAppUpdate(silent = false): Promise<UpdateStatus | null> {
  checkingForUpdate.set(true);
  try {
    const status = await api.checkUpdate();
    updateInfo.set(status);
    if (status.available) {
      updateBannerDismissed.set(false);
    } else if (!silent) {
      notifyUi(`Dropilot v${status.currentVersion} is up to date`);
    }
    return status;
  } catch (err) {
    if (!silent) {
      notifyUi(String(err), 'error');
    }
    return null;
  } finally {
    checkingForUpdate.set(false);
  }
}

export async function installAppUpdate(): Promise<void> {
  installingUpdate.set(true);
  updateProgress.set({
    stage: 'downloading',
    downloadedBytes: 0,
    totalBytes: null,
    percent: null,
  });
  try {
    await api.installUpdate();
  } catch (err) {
    installingUpdate.set(false);
    updateProgress.set(null);
    notifyUi(String(err), 'error');
  }
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

export async function toggleGameGroupFarmAction(
  campaigns: Campaign[],
  farm: boolean,
  hasAccount: boolean,
  currentSettings: Settings,
): Promise<void> {
  if (campaigns.length === 0) return;
  const primary = campaigns[0];
  const plat = primary.platform;
  const platSettings = currentSettings[plat];

  let updated = await api.setGameFarm(plat, primary.game.id, primary.game.name, farm);

  if (farm) {
    // Ensure campaigns in this game aren't stuck in excludedCampaigns when explicitly farming the game.
    for (const c of campaigns) {
      if (platSettings.excludedCampaigns.includes(c.id)) {
        updated = await api.setCampaignFarm(plat, c.id, true);
      }
    }
    if (!hasAccount) {
      const platName = plat === 'twitch' ? 'Twitch' : 'Kick';
      notifyUi(`Saved! Log in to ${platName} to start farming ${primary.game.name}.`);
    }
  } else {
    // Also clear any individual campaign-level farm/priority entries for this game so the toggle truly turns off.
    const campaignIds = new Set(campaigns.map((c) => c.id));
    const hasCampaignPriority = updated[plat].priority.some(
      (p) => p.kind === 'campaign' && campaignIds.has(p.campaignId),
    );
    if (hasCampaignPriority) {
      updated = await api.setPriority(
        plat,
        updated[plat].priority.filter(
          (p) => !(p.kind === 'campaign' && campaignIds.has(p.campaignId)),
        ),
      );
    }
    for (const c of campaigns) {
      if (updated[plat].farmCampaigns.includes(c.id)) {
        updated = await api.setCampaignFarm(plat, c.id, false);
      }
    }
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

  const [snap, settings, logs, hasMock, rcStatus] = await Promise.all([
    api.getSnapshot(),
    api.getSettings(),
    api.getLogs(),
    api.isMockAvailable(),
    api.getRemoteConfigStatus().catch(() => null),
  ]);

  trackSessionProgress(snap, true);
  appSnapshot.set(snap);
  appSettings.set(settings);
  appLogs.set(logs);
  remoteConfigStatus.set(rcStatus);
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
    (progress) => {
      updateProgress.set(progress);
      if (progress.stage === 'error') {
        installingUpdate.set(false);
      }
    },
  );

  if (settings.updateCheck) {
    void checkForAppUpdate(true);
  }

  return unlisten;
}
