export type PlatformId = 'twitch' | 'kick';

export type MinerState =
  | 'LoggedOut'
  | 'Idle'
  | 'Selecting'
  | 'Mining'
  | 'Claiming'
  | 'Paused'
  | 'Error';

export type CampaignStatus = 'active' | 'upcoming' | 'expired';

export type ThemeMode = 'dark' | 'light' | 'system';

export type FontFamily = 'plex' | 'inter' | 'system' | 'mono';

export type FontScale = 'compact' | 'default' | 'comfortable' | 'large';

export type IdleBehavior = 'stay_idle' | 'farm_any';

export type LogLevel = 'info' | 'warn' | 'error' | 'debug';

export type LogLevelSetting = 'info' | 'debug';

export interface Game {
  id: string;
  name: string;
  slug?: string | null;
  boxArtUrl: string | null;
}

export interface ChannelRef {
  id: string;
  login: string;
  displayName: string;
}

export interface Channel {
  id: string;
  login: string;
  displayName: string;
  avatarUrl: string | null;
  gameName: string;
  viewerCount: number;
  isLive: boolean;
  dropsEnabled: boolean;
  isFollowed: boolean;
}

export interface Reward {
  name: string;
  imageUrl: string | null;
}

export interface Drop {
  id: string;
  instanceId: string | null;
  name: string;
  rewards: Reward[];
  requiredMinutes: number;
  currentMinutes: number;
  claimable: boolean;
  claimed: boolean;
  needsManualClaim: boolean;
}

export interface Campaign {
  id: string;
  platform: PlatformId;
  game: Game;
  name: string;
  startsAt: string;
  endsAt: string;
  status: CampaignStatus;
  requiresLink: boolean;
  linked: boolean;
  linkVerified?: boolean;
  linkUrl: string | null;
  allowedChannels: ChannelRef[] | null;
  drops: Drop[];
}

export interface Account {
  id: string;
  login: string;
  displayName: string;
  avatarUrl: string | null;
}

export interface PlatformSnapshot {
  platform: PlatformId;
  state: MinerState;
  stateReason: string | null;
  account: Account | null;
  watching: Channel | null;
  currentCampaignId: string | null;
  currentDropId: string | null;
  watchingSince?: string | null;
  progressSyncedAt?: string | null;
  nextActionAt: string | null;
  lastError: string | null;
  campaigns: Campaign[];
  candidateChannels: Channel[];
  pinnedChannel?: string | null;
}

export interface RecentClaim {
  id: string;
  platform: PlatformId;
  gameName: string;
  campaignName: string;
  dropName: string;
  rewardImageUrl: string | null;
  claimedAt: string;
}

export interface AppSnapshot {
  twitch: PlatformSnapshot;
  kick: PlatformSnapshot;
  recentClaims: RecentClaim[];
  mockActive: boolean;
}

export type PriorityItem =
  | { kind: 'game'; gameId: string; gameName: string }
  | { kind: 'campaign'; campaignId: string; campaignName: string };

export interface PlatformSettings {
  enabled: boolean;
  priority: PriorityItem[];
  farmCampaigns: string[];
  excludedCampaigns: string[];
  pinnedChannel: string | null;
  prioritizeEndingSoon: boolean;
}

export interface WindowBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ScheduleSettings {
  quietHoursEnabled: boolean;
  quietStartMinutes: number;
  quietEndMinutes: number;
  humanizedJitter: boolean;
}

export interface WebhookSettings {
  discordEnabled: boolean;
  onDropClaimed: boolean;
  onActionRequired: boolean;
  onCampaignCompleted: boolean;
}

export interface WebhookSecretsStatus {
  hasDiscordUrl: boolean;
}

export type WebhookChannel = 'discord';

export interface RemoteConfigStatus {
  source: 'remote' | 'cache' | 'bundled';
  fetchedAt: string | null;
  updatedAt: string | null;
  overriddenHashes: number;
  maintenanceBanner: string | null;
  twitchEnabled: boolean;
  kickEnabled: boolean;
  lastError: string | null;
}

export type ClaimRecordStatus = 'claimed' | 'manual_required';

export interface ClaimHistoryEntry {
  id: string;
  platform: PlatformId;
  gameName: string;
  campaignName: string;
  dropName: string;
  rewardImageUrl: string | null;
  claimedAt: string;
  status: ClaimRecordStatus;
}

export interface DailyActivityPoint {
  date: string;
  minutes: number;
  claims: number;
}

export interface HistoryStats {
  totalClaims: number;
  twitchClaims: number;
  kickClaims: number;
  totalWatchMinutes: number;
  currentStreakDays: number;
  bestStreakDays: number;
  topGame: string | null;
  topGameClaims: number;
  dailyActivity: DailyActivityPoint[];
  claims: ClaimHistoryEntry[];
}

export interface HistoryExportResult {
  filename: string;
  savedPath: string;
  content: string;
}

export interface Settings {
  schemaVersion: number;
  theme: ThemeMode;
  fontFamily: FontFamily;
  fontScale: FontScale;
  autostart: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  notifications: boolean;
  autoClaim: boolean;
  idleBehavior: IdleBehavior;
  refreshMinutes: number;
  farmUnlinked: boolean;
  compatWatchMode: boolean;
  updateCheck: boolean;
  remoteConfigEnabled: boolean;
  schedule: ScheduleSettings;
  webhooks: WebhookSettings;
  disclaimerAcceptedVersion: number;
  logLevel: LogLevelSetting;
  mockMode: boolean;
  windowBounds: WindowBounds | null;
  twitch: PlatformSettings;
  kick: PlatformSettings;
}

export interface LogEntry {
  id: number;
  timestamp: string;
  level: LogLevel;
  platform: PlatformId | null;
  message: string;
}

export interface UpdateStatus {
  available: boolean;
  currentVersion: string;
  latestVersion: string;
  releaseUrl: string;
  publishedAt: string | null;
  notes: string | null;
  signedUpdateReady: boolean;
}

export interface UpdateProgressEvent {
  stage: 'downloading' | 'installing' | 'restarting' | 'error';
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number | null;
}

export type NavRoute =
  | 'dashboard'
  | 'drops'
  | 'history'
  | 'twitch'
  | 'kick'
  | 'logs'
  | 'settings';

export type PlatformFilter = 'all' | 'twitch' | 'kick';

export type CampaignFilter =
  | 'all'
  | 'active'
  | 'connected'
  | 'unlinked'
  | 'farming'
  | 'claimable'
  | 'upcoming'
  | 'completed';

export type CampaignSort = 'ending_soon' | 'progress' | 'shortest' | 'rewards' | 'name';

export type DropsViewMode = 'grid' | 'compact';

export interface ToastItem {
  id: number;
  message: string;
  kind: 'info' | 'error';
}
