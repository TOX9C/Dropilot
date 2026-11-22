import { invoke } from '@tauri-apps/api/core';
import type {
  AppSnapshot,
  Campaign,
  LogEntry,
  LogLevel,
  PlatformId,
  PriorityItem,
  Settings,
  UpdateStatus,
} from './types';

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

function createPreviewData(): {
  settings: Settings;
  snapshot: AppSnapshot;
  logs: LogEntry[];
} {
  if (!import.meta.env.DEV) {
    const emptyPlat = (platform: PlatformId) => ({
      platform,
      state: 'LoggedOut' as const,
      stateReason: null,
      account: null,
      watching: null,
      currentCampaignId: null,
      currentDropId: null,
      nextActionAt: null,
      lastError: null,
      campaigns: [],
      candidateChannels: [],
    });
    const emptyPS = () => ({
      enabled: true,
      priority: [],
      farmCampaigns: [],
      excludedCampaigns: [],
      pinnedChannel: null,
    });
    return {
      settings: {
        schemaVersion: 1,
        theme: 'dark',
        autostart: false,
        startMinimized: false,
        closeToTray: true,
        notifications: true,
        autoClaim: true,
        idleBehavior: 'stay_idle',
        refreshMinutes: 30,
        farmUnlinked: false,
        compatWatchMode: false,
        updateCheck: false,
        disclaimerAcceptedVersion: 1,
        logLevel: 'info',
        mockMode: false,
        windowBounds: null,
        twitch: emptyPS(),
        kick: emptyPS(),
      },
      snapshot: {
        twitch: emptyPlat('twitch'),
        kick: emptyPlat('kick'),
        recentClaims: [],
        mockActive: false,
      },
      logs: [],
    };
  }
  const previewNow = Date.now();
  return {
    settings: {
      schemaVersion: 1,
      theme: 'dark',
      autostart: false,
      startMinimized: false,
      closeToTray: true,
      notifications: true,
      autoClaim: true,
      idleBehavior: 'stay_idle',
      refreshMinutes: 30,
      farmUnlinked: false,
      compatWatchMode: false,
      updateCheck: false,
      disclaimerAcceptedVersion: 1,
      logLevel: 'info',
      mockMode: true,
      windowBounds: null,
      twitch: {
        enabled: true,
        priority: [{ kind: 'game', gameId: '263490', gameName: 'Rust' }],
        farmCampaigns: ['tw-rust-1'],
        excludedCampaigns: [],
        pinnedChannel: null,
      },
      kick: {
        enabled: true,
        priority: [{ kind: 'game', gameId: 'rust', gameName: 'Rust' }],
        farmCampaigns: ['kk-rust-1'],
        excludedCampaigns: [],
        pinnedChannel: null,
      },
    },
    snapshot: {
      mockActive: true,
      recentClaims: [
        {
          id: 'kk-eft-1-kk-eft-d1',
          platform: 'kick',
          gameName: 'Escape from Tarkov',
          campaignName: 'Tarkov Weekend Drop',
          dropName: 'Streamer Supply Case',
          rewardImageUrl: null,
          claimedAt: new Date(previewNow - 24 * 60_000).toISOString(),
        },
      ],
      twitch: {
        platform: 'twitch',
        state: 'Mining',
        stateReason: 'Watching hJune (Rust)',
        account: { id: '1', login: 'demo_pilot', displayName: 'DemoPilot', avatarUrl: null },
        watching: {
          id: '101',
          login: 'hJune',
          displayName: 'hJune',
          avatarUrl: null,
          gameName: 'Rust',
          viewerCount: 14820,
          isLive: true,
          dropsEnabled: true,
          isFollowed: true,
        },
        currentCampaignId: 'tw-rust-1',
        currentDropId: 'tw-rust-d1',
        nextActionAt: new Date(previewNow + 45_000).toISOString(),
        lastError: null,
        candidateChannels: [
          {
            id: '101',
            login: 'hJune',
            displayName: 'hJune',
            avatarUrl: null,
            gameName: 'Rust',
            viewerCount: 14820,
            isLive: true,
            dropsEnabled: true,
            isFollowed: true,
          },
          {
            id: '102',
            login: 'Posty',
            displayName: 'Posty',
            avatarUrl: null,
            gameName: 'Rust',
            viewerCount: 6410,
            isLive: true,
            dropsEnabled: true,
            isFollowed: false,
          },
        ],
        campaigns: [
          {
            id: 'tw-rust-1',
            platform: 'twitch',
            game: { id: '263490', name: 'Rust', boxArtUrl: null },
            name: 'Rust Twitch Rivals Drops',
            startsAt: new Date(previewNow - 12 * 3600_000).toISOString(),
            endsAt: new Date(previewNow + 42 * 3600_000).toISOString(),
            status: 'active',
            requiresLink: true,
            linked: true,
            linkUrl: 'https://twitch.facepunch.com',
            allowedChannels: null,
            drops: [
              {
                id: 'tw-rust-d1',
                instanceId: 'inst-1',
                name: 'Cobalt Garage Door',
                rewards: [{ name: 'Cobalt Garage Door', imageUrl: null }],
                requiredMinutes: 30,
                currentMinutes: 19,
                claimable: false,
                claimed: false,
                needsManualClaim: false,
              },
              {
                id: 'tw-rust-d2',
                instanceId: 'inst-2',
                name: 'Rivals Hoodie',
                rewards: [{ name: 'Rivals Hoodie', imageUrl: null }],
                requiredMinutes: 60,
                currentMinutes: 0,
                claimable: false,
                claimed: false,
                needsManualClaim: false,
              },
            ],
          },
          {
            id: 'tw-cs2-urgent',
            platform: 'twitch',
            game: { id: '32399', name: 'Counter-Strike 2', boxArtUrl: null },
            name: 'Major Austin Viewer Souvenir',
            startsAt: new Date(previewNow - 19 * 3600_000).toISOString(),
            endsAt: new Date(previewNow + 4.5 * 3600_000).toISOString(),
            status: 'active',
            requiresLink: false,
            linked: true,
            linkUrl: null,
            allowedChannels: null,
            drops: [
              {
                id: 'tw-cs2-d1',
                instanceId: 'inst-tw-cs2-d1',
                name: 'Championship Sticker Capsule',
                rewards: [{ name: 'Sticker Capsule', imageUrl: null }],
                requiredMinutes: 30,
                currentMinutes: 5,
                claimable: false,
                claimed: false,
                needsManualClaim: false,
              },
            ],
          },
          {
            id: 'tw-poe2-1',
            platform: 'twitch',
            game: { id: '1702520304', name: 'Path of Exile 2', boxArtUrl: null },
            name: 'Dawn of the Hunt Week 1',
            startsAt: new Date(previewNow - 6 * 3600_000).toISOString(),
            endsAt: new Date(previewNow + 72 * 3600_000).toISOString(),
            status: 'active',
            requiresLink: true,
            linked: false,
            linkUrl: 'https://www.pathofexile.com/my-account/twitch',
            allowedChannels: null,
            drops: [
              {
                id: 'tw-poe2-d1',
                instanceId: null,
                name: "Executioner's Portal Effect",
                rewards: [{ name: "Executioner's Portal", imageUrl: null }],
                requiredMinutes: 120,
                currentMinutes: 0,
                claimable: false,
                claimed: false,
                needsManualClaim: false,
              },
            ],
          },
        ],
      },
      kick: {
        platform: 'kick',
        state: 'Idle',
        stateReason: 'No live channels for Rust, retrying in 5 min',
        account: { id: '2', login: 'kick_pilot', displayName: 'KickPilot', avatarUrl: null },
        watching: null,
        currentCampaignId: null,
        currentDropId: null,
        nextActionAt: new Date(previewNow + 300_000).toISOString(),
        lastError: null,
        candidateChannels: [],
        campaigns: [
          {
            id: 'kk-rust-1',
            platform: 'kick',
            game: { id: 'rust', name: 'Rust', boxArtUrl: null },
            name: 'Kick Rust Creators Round',
            startsAt: new Date(previewNow - 4 * 3600_000).toISOString(),
            endsAt: new Date(previewNow + 36 * 3600_000).toISOString(),
            status: 'active',
            requiresLink: true,
            linked: true,
            linkUrl: 'https://kick.com/drops/inventory',
            allowedChannels: null,
            drops: [
              {
                id: 'kk-rust-d1',
                instanceId: null,
                name: 'Emerald Sleeping Bag',
                rewards: [{ name: 'Emerald Sleeping Bag', imageUrl: null }],
                requiredMinutes: 45,
                currentMinutes: 25,
                claimable: false,
                claimed: false,
                needsManualClaim: false,
              },
            ],
          },
          {
            id: 'kk-poe2-upcoming',
            platform: 'kick',
            game: { id: 'poe2', name: 'Path of Exile 2', boxArtUrl: null },
            name: 'Kick Launch Race Drops',
            startsAt: new Date(previewNow + 14 * 3600_000).toISOString(),
            endsAt: new Date(previewNow + 96 * 3600_000).toISOString(),
            status: 'upcoming',
            requiresLink: true,
            linked: false,
            linkUrl: 'https://kick.com/drops/inventory',
            allowedChannels: null,
            drops: [
              {
                id: 'kk-poe2-d1',
                instanceId: null,
                name: 'Verdant Weapon Effect',
                rewards: [{ name: 'Verdant Weapon Effect', imageUrl: null }],
                requiredMinutes: 60,
                currentMinutes: 0,
                claimable: false,
                claimed: false,
                needsManualClaim: false,
              },
            ],
          },
        ],
      },
    },
    logs: [
      {
        id: 2,
        timestamp: new Date(previewNow - 5_000).toISOString(),
        level: 'info',
        platform: 'twitch',
        message: 'Selected stream hJune (Rust)',
      },
      {
        id: 1,
        timestamp: new Date(previewNow - 20_000).toISOString(),
        level: 'info',
        platform: null,
        message: 'DropPilot preview initialized',
      },
    ],
  };
}

const previewData = createPreviewData();
let previewSettings: Settings = previewData.settings;
const previewSnapshot: AppSnapshot = previewData.snapshot;
const previewLogs: LogEntry[] = previewData.logs;

async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
  fallback?: () => T,
): Promise<T> {
  if (!isTauriRuntime()) {
    return (fallback ? fallback() : undefined) as T;
  }
  return invoke<T>(cmd, args);
}

const cloneSettings = () => structuredClone(previewSettings);

export const api = {
  getSnapshot: () => call<AppSnapshot>('get_snapshot', undefined, () => structuredClone(previewSnapshot)),
  login: (platform: PlatformId) => call<void>('login', { platform }),
  logout: (platform: PlatformId) => call<void>('logout', { platform }),
  setCampaignFarm: (platform: PlatformId, campaignId: string, farm: boolean) =>
    call<Settings>('set_campaign_farm', { platform, campaignId, farm }, cloneSettings),
  setGameFarm: (platform: PlatformId, gameId: string, gameName: string, farm: boolean) =>
    call<Settings>('set_game_farm', { platform, gameId, gameName, farm }, () => {
      const cleanName = gameName.trim();
      const lower = cleanName.toLowerCase();
      if (farm && cleanName) {
        const exists = previewSettings[platform].priority.some(
          (i) =>
            i.kind === 'game' &&
            ((gameId && i.gameId === gameId) || i.gameName.trim().toLowerCase() === lower),
        );
        if (!exists) {
          previewSettings[platform].priority = [
            ...previewSettings[platform].priority,
            { kind: 'game', gameId, gameName: cleanName },
          ];
        }
      } else if (!farm) {
        previewSettings[platform].priority = previewSettings[platform].priority.filter(
          (i) =>
            !(
              i.kind === 'game' &&
              ((gameId && i.gameId === gameId) || (lower && i.gameName.trim().toLowerCase() === lower))
            ),
        );
      }
      return cloneSettings();
    }),
  fetchCampaignDetails: (platform: PlatformId, campaignId: string) =>
    call<Campaign>('fetch_campaign_details', { platform, campaignId }, () => {
      const found = previewSnapshot[platform].campaigns.find((c) => c.id === campaignId);
      if (!found) throw new Error('Campaign not found');
      return structuredClone(found);
    }),
  setPriority: (platform: PlatformId, items: PriorityItem[]) =>
    call<Settings>('set_priority', { platform, items }, () => {
      previewSettings[platform].priority = items;
      return cloneSettings();
    }),
  pause: (platform: PlatformId) =>
    call<void>('pause', { platform }, () => {
      previewSnapshot[platform].state = 'Paused';
      previewSnapshot[platform].stateReason = 'Paused by user';
    }),
  resume: (platform: PlatformId) =>
    call<void>('resume', { platform }, () => {
      previewSnapshot[platform].state = 'Mining';
    }),
  switchChannel: (platform: PlatformId) => call<void>('switch_channel', { platform }),
  pinChannel: (platform: PlatformId, channelLogin: string | null) =>
    call<Settings>('pin_channel', { platform, channelLogin }, () => {
      previewSettings[platform].pinnedChannel = channelLogin;
      return cloneSettings();
    }),
  retryPlatform: (platform: PlatformId) => call<void>('retry_platform', { platform }),
  openDropsInventory: (platform: PlatformId) => call<void>('open_drops_inventory', { platform }),
  getSettings: () => call<Settings>('get_settings', undefined, cloneSettings),
  updateSettings: (newSettings: Settings) =>
    call<Settings>('update_settings', { newSettings }, () => {
      previewSettings = structuredClone(newSettings);
      return cloneSettings();
    }),
  acceptDisclaimer: () =>
    call<Settings>('accept_disclaimer', undefined, () => {
      previewSettings.disclaimerAcceptedVersion = 1;
      return cloneSettings();
    }),
  clearAppData: () => call<void>('clear_app_data'),
  getLogs: (level?: LogLevel | null, platform?: PlatformId | null) =>
    call<LogEntry[]>('get_logs', { level: level ?? null, platform: platform ?? null }, () => previewLogs),
  openUrl: (url: string) =>
    call<void>('open_url', { url }, () => {
      window.open(url, '_blank', 'noopener,noreferrer');
    }),
  openLogFolder: () => call<void>('open_log_folder'),
  openDataFolder: () => call<void>('open_data_folder'),
  checkUpdate: () =>
    call<UpdateStatus>('check_update', undefined, () => ({
      available: false,
      currentVersion: '0.1.0',
      latestVersion: '0.1.0',
      releaseUrl: 'https://github.com/droppilot/droppilot/releases',
      publishedAt: null,
    })),
  isMockAvailable: () => call<boolean>('is_mock_available', undefined, () => true),
  reportUserAgent: (userAgent: string) => call<void>('report_user_agent', { userAgent }),
};
