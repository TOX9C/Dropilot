import { invoke } from '@tauri-apps/api/core';
import type {
  AppSnapshot,
  Campaign,
  EditionInfo,
  EpicFeedSnapshot,
  HistoryExportResult,
  HistoryStats,
  LogEntry,
  LogLevel,
  PlatformId,
  PriorityItem,
  RemoteConfigStatus,
  Settings,
  UpdateStatus,
  WebhookChannel,
  WebhookSecretsStatus,
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
      pinnedChannel: null,
    });
    const emptyPS = () => ({
      enabled: true,
      priority: [],
      farmCampaigns: [],
      excludedCampaigns: [],
      pinnedChannel: null,
      prioritizeEndingSoon: false,
    });
    return {
      settings: {
        schemaVersion: 2,
        theme: 'dark',
        fontFamily: 'plex',
        fontScale: 'default',
        autostart: false,
        startInTray: true,
        startMinimized: false,
        closeToTray: true,
        notifications: true,
        autoClaim: true,
        idleBehavior: 'stay_idle',
        refreshMinutes: 30,
        farmUnlinked: false,
        compatWatchMode: false,
        updateCheck: true,
        autoInstallUpdates: true,
        remoteConfigEnabled: true,
        schedule: {
          quietHoursEnabled: false,
          quietStartMinutes: 22 * 60,
          quietEndMinutes: 8 * 60,
          humanizedJitter: true,
        },
        webhooks: {
          discordEnabled: false,
          onDropClaimed: true,
          onActionRequired: true,
          onCampaignCompleted: true,
          onEpicFreeGames: true,
        },
        disclaimerAcceptedVersion: 1,
        logLevel: 'info',
        mockMode: false,
        windowBounds: null,
        twitch: emptyPS(),
        kick: emptyPS(),
        profiles: [
          { id: 'default-twitch', label: 'Main', platform: 'twitch', settings: emptyPS() },
          { id: 'default-kick', label: 'Main', platform: 'kick', settings: emptyPS() },
        ],
        activeTwitchProfileId: 'default-twitch',
        activeKickProfileId: 'default-kick',
      },
      snapshot: {
        twitch: emptyPlat('twitch'),
        kick: emptyPlat('kick'),
        profiles: [
          { profileId: 'default-twitch', label: 'Main', platform: 'twitch', snapshot: emptyPlat('twitch') },
          { profileId: 'default-kick', label: 'Main', platform: 'kick', snapshot: emptyPlat('kick') },
        ],
        recentClaims: [],
        mockActive: false,
      },
      logs: [],
    };
  }
  const previewNow = Date.now();
  return {
    settings: {
      schemaVersion: 2,
      theme: 'dark',
      fontFamily: 'plex',
      fontScale: 'default',
      autostart: false,
      startInTray: true,
      startMinimized: false,
      closeToTray: true,
      notifications: true,
      autoClaim: true,
      idleBehavior: 'stay_idle',
      refreshMinutes: 30,
      farmUnlinked: false,
      compatWatchMode: false,
      updateCheck: true,
      autoInstallUpdates: true,
      remoteConfigEnabled: true,
      schedule: {
        quietHoursEnabled: false,
        quietStartMinutes: 22 * 60,
        quietEndMinutes: 8 * 60,
        humanizedJitter: true,
      },
      webhooks: {
        discordEnabled: false,
        onDropClaimed: true,
        onActionRequired: true,
        onCampaignCompleted: true,
        onEpicFreeGames: true,
      },
      disclaimerAcceptedVersion: 1,
      logLevel: 'info',
      mockMode: true,
      windowBounds: null,
      activeTwitchProfileId: 'default-twitch',
      activeKickProfileId: 'default-kick',
      twitch: {
        enabled: true,
        priority: [{ kind: 'game', gameId: '263490', gameName: 'Rust' }],
        farmCampaigns: ['tw-rust-1'],
        excludedCampaigns: [],
        pinnedChannel: null,
        prioritizeEndingSoon: false,
      },
      kick: {
        enabled: true,
        priority: [{ kind: 'game', gameId: 'rust', gameName: 'Rust' }],
        farmCampaigns: ['kk-rust-1'],
        excludedCampaigns: [],
        pinnedChannel: null,
        prioritizeEndingSoon: false,
      },
      profiles: [
        {
          id: 'default-twitch',
          label: 'Main',
          platform: 'twitch',
          settings: {
            enabled: true,
            priority: [{ kind: 'game', gameId: '263490', gameName: 'Rust' }],
            farmCampaigns: ['tw-rust-1'],
            excludedCampaigns: [],
            pinnedChannel: null,
            prioritizeEndingSoon: false,
          },
        },
        {
          id: 'default-kick',
          label: 'Main',
          platform: 'kick',
          settings: {
            enabled: true,
            priority: [{ kind: 'game', gameId: 'rust', gameName: 'Rust' }],
            farmCampaigns: ['kk-rust-1'],
            excludedCampaigns: [],
            pinnedChannel: null,
            prioritizeEndingSoon: false,
          },
        },
      ],
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
        message: 'Dropilot preview initialized',
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

function syncPreviewProfiles(next: Settings) {
  if (!next.profiles) return;
  const tw = next.profiles.find((p) => p.id === next.activeTwitchProfileId);
  if (tw) next.twitch = structuredClone(tw.settings);
  const kk = next.profiles.find((p) => p.id === next.activeKickProfileId);
  if (kk) next.kick = structuredClone(kk.settings);
}

export const api = {
  getSnapshot: () => call<AppSnapshot>('get_snapshot', undefined, () => {
    const snap = structuredClone(previewSnapshot);
    if (!snap.profiles) {
      snap.profiles = [
        { profileId: 'default-twitch', label: 'Main', platform: 'twitch', snapshot: structuredClone(snap.twitch) },
        { profileId: 'default-kick', label: 'Main', platform: 'kick', snapshot: structuredClone(snap.kick) },
      ];
    }
    return snap;
  }),
  addAccountProfile: (platform: PlatformId, label: string) =>
    call<Settings>('add_account_profile', { platform, label }, () => {
      const id = `${platform}-${Date.now()}`;
      const cleanLabel = label.trim() || `Account ${(previewSettings.profiles?.filter((p) => p.platform === platform).length ?? 1) + 1}`;
      const ps = {
        enabled: true,
        priority: [],
        farmCampaigns: [],
        excludedCampaigns: [],
        pinnedChannel: null,
        prioritizeEndingSoon: false,
      };
      previewSettings.profiles = [
        ...(previewSettings.profiles ?? []),
        { id, label: cleanLabel, platform, settings: ps },
      ];
      return cloneSettings();
    }),
  removeAccountProfile: (profileId: string) =>
    call<Settings>('remove_account_profile', { profileId }, () => {
      previewSettings.profiles = (previewSettings.profiles ?? []).filter((p) => p.id !== profileId);
      syncPreviewProfiles(previewSettings);
      return cloneSettings();
    }),
  setActiveProfile: (platform: PlatformId, profileId: string) =>
    call<Settings>('set_active_profile', { platform, profileId }, () => {
      if (platform === 'twitch') previewSettings.activeTwitchProfileId = profileId;
      else previewSettings.activeKickProfileId = profileId;
      syncPreviewProfiles(previewSettings);
      return cloneSettings();
    }),
  renameAccountProfile: (profileId: string, label: string) =>
    call<Settings>('rename_account_profile', { profileId, label }, () => {
      const target = previewSettings.profiles?.find((p) => p.id === profileId);
      if (target && label.trim()) target.label = label.trim();
      return cloneSettings();
    }),
  login: (platform: PlatformId, profileId?: string | null) =>
    call<void>('login', { platform, profileId: profileId ?? null }),
  logout: (platform: PlatformId, profileId?: string | null) =>
    call<void>('logout', { platform, profileId: profileId ?? null }),
  setCampaignFarm: (platform: PlatformId, campaignId: string, farm: boolean, profileId?: string | null) =>
    call<Settings>('set_campaign_farm', { platform, campaignId, farm, profileId: profileId ?? null }, cloneSettings),
  setGameFarm: (platform: PlatformId, gameId: string, gameName: string, farm: boolean, profileId?: string | null) =>
    call<Settings>('set_game_farm', { platform, gameId, gameName, farm, profileId: profileId ?? null }, () => {
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
  fetchCampaignDetails: (platform: PlatformId, campaignId: string, profileId?: string | null) =>
    call<Campaign>('fetch_campaign_details', { platform, campaignId, profileId: profileId ?? null }, () => {
      const found = previewSnapshot[platform].campaigns.find((c) => c.id === campaignId);
      if (!found) throw new Error('Campaign not found');
      return structuredClone(found);
    }),
  setPriority: (platform: PlatformId, items: PriorityItem[], profileId?: string | null) =>
    call<Settings>('set_priority', { platform, items, profileId: profileId ?? null }, () => {
      previewSettings[platform].priority = items;
      return cloneSettings();
    }),
  setPrioritizeEndingSoon: (platform: PlatformId, enabled: boolean, profileId?: string | null) =>
    call<Settings>('set_prioritize_ending_soon', { platform, enabled, profileId: profileId ?? null }, () => {
      previewSettings[platform].prioritizeEndingSoon = enabled;
      return cloneSettings();
    }),
  pause: (platform: PlatformId, profileId?: string | null) =>
    call<void>('pause', { platform, profileId: profileId ?? null }, () => {
      previewSnapshot[platform].state = 'Paused';
      previewSnapshot[platform].stateReason = 'Paused by user';
    }),
  resume: (platform: PlatformId, profileId?: string | null) =>
    call<void>('resume', { platform, profileId: profileId ?? null }, () => {
      previewSnapshot[platform].state = 'Mining';
    }),
  switchChannel: (platform: PlatformId, profileId?: string | null) =>
    call<void>('switch_channel', { platform, profileId: profileId ?? null }, () => {
      previewSettings[platform].pinnedChannel = null;
      previewSnapshot[platform].pinnedChannel = null;
    }),
  pinChannel: (platform: PlatformId, channelLogin: string | null, profileId?: string | null) =>
    call<Settings>('pin_channel', { platform, channelLogin, profileId: profileId ?? null }, () => {
      previewSettings[platform].pinnedChannel = channelLogin;
      previewSnapshot[platform].pinnedChannel = channelLogin;
      return cloneSettings();
    }),
  retryPlatform: (platform: PlatformId, profileId?: string | null) =>
    call<void>('retry_platform', { platform, profileId: profileId ?? null }),
  openDropsInventory: (platform: PlatformId, profileId?: string | null) =>
    call<void>('open_drops_inventory', { platform, profileId: profileId ?? null }),
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
      currentVersion: '0.3.0',
      latestVersion: '0.3.0',
      releaseUrl: 'https://github.com/theflash1211/dropilot-releases/releases',
      publishedAt: null,
      notes: null,
      signedUpdateReady: true,
    })),
  installUpdate: () => call<void>('install_update'),
  getEditionInfo: () =>
    call<EditionInfo>('get_edition_info', undefined, () => ({
      edition: 'full',
      isFree: false,
      twitchEnabled: true,
      kickEnabled: true,
      epicEnabled: true,
      multiAccountEnabled: true,
      webhooksEnabled: true,
      quietHoursEnabled: true,
      maxProfilesPerPlatform: 10,
    })),
  isMockAvailable: () => call<boolean>('is_mock_available', undefined, () => true),
  reportUserAgent: (userAgent: string) => call<void>('report_user_agent', { userAgent }),
  hideToTray: () => call<void>('hide_to_tray'),
  getRemoteConfigStatus: () =>
    call<RemoteConfigStatus>('get_remote_config_status', undefined, () => ({
      source: 'bundled',
      fetchedAt: null,
      updatedAt: null,
      overriddenHashes: 0,
      maintenanceBanner: null,
      twitchEnabled: true,
      kickEnabled: true,
      lastError: null,
    })),
  refreshRemoteConfig: () =>
    call<RemoteConfigStatus>('refresh_remote_config', undefined, () => ({
      source: 'remote',
      fetchedAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      overriddenHashes: 11,
      maintenanceBanner: null,
      twitchEnabled: true,
      kickEnabled: true,
      lastError: null,
    })),
  getWebhookSecretsStatus: () =>
    call<WebhookSecretsStatus>('get_webhook_secrets_status', undefined, () => ({
      hasDiscordUrl: false,
    })),
  saveWebhookSecrets: (discordUrl?: string | null) =>
    call<WebhookSecretsStatus>(
      'save_webhook_secrets',
      {
        discordUrl: discordUrl ?? null,
      },
      () => ({
        hasDiscordUrl: Boolean(discordUrl),
      }),
    ),
  testWebhook: (channel: WebhookChannel) =>
    call<string>(
      'test_webhook',
      {
        channel,
      },
      () => 'Test Discord notification sent',
    ),
  getHistoryStats: () =>
    call<HistoryStats>('get_history_stats', undefined, () => ({
      totalClaims: previewSnapshot.recentClaims.length,
      twitchClaims: previewSnapshot.recentClaims.filter((c) => c.platform === 'twitch').length,
      kickClaims: previewSnapshot.recentClaims.filter((c) => c.platform === 'kick').length,
      epicClaims: 0,
      totalWatchMinutes: 145,
      currentStreakDays: 2,
      bestStreakDays: 4,
      topGame: previewSnapshot.recentClaims[0]?.gameName ?? null,
      topGameClaims: previewSnapshot.recentClaims.length,
      dailyActivity: Array.from({ length: 14 }, (_, idx) => {
        const d = new Date(Date.now() - (13 - idx) * 86_400_000);
        return {
          date: d.toISOString().slice(0, 10),
          minutes: idx >= 11 ? 30 + idx * 5 : 0,
          claims: idx === 13 ? 1 : 0,
        };
      }),
      claims: previewSnapshot.recentClaims.map((c) => ({
        ...c,
        status: 'claimed' as const,
      })),
    })),
  exportHistory: (format: 'csv' | 'json') =>
    call<HistoryExportResult>('export_history', { format }, () => ({
      filename: `dropilot-history-preview.${format}`,
      savedPath: `%APPDATA%\\Dropilot\\exports\\dropilot-history-preview.${format}`,
      content: format === 'json' ? '[]' : 'claimed_at,platform,game,campaign,drop,status\n',
    })),
  clearHistory: () => call<void>('clear_history'),
  getEpicFreeGames: () =>
    call<EpicFeedSnapshot>('get_epic_free_games', undefined, () => ({
      games: [],
      fetchedAt: new Date().toISOString(),
      lastError: null,
      source: 'cache',
    })),
  refreshEpicFreeGames: () =>
    call<EpicFeedSnapshot>('refresh_epic_free_games', undefined, () => ({
      games: [],
      fetchedAt: new Date().toISOString(),
      lastError: null,
      source: 'live',
    })),
  openEpicClaim: (gameId: string) =>
    call<void>('open_epic_claim', { gameId }, () => {
      window.open('https://store.epicgames.com/en-US/free-games', '_blank', 'noopener,noreferrer');
    }),
  openEpicLauncher: (gameId?: string | null) =>
    call<boolean>('open_epic_launcher', { gameId: gameId ?? null }, () => {
      window.open('com.epicgames.launcher://store/free-games', '_blank', 'noopener,noreferrer');
      return true;
    }),
  markEpicGameClaimed: (gameId: string, claimed: boolean) =>
    call<EpicFeedSnapshot>(
      'mark_epic_game_claimed',
      { gameId, claimed },
      () => ({
        games: [],
        fetchedAt: new Date().toISOString(),
        lastError: null,
        source: 'cache',
      }),
    ),
};
