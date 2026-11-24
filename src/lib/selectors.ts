import type {
  AppSnapshot,
  Campaign,
  CampaignFilter,
  CampaignSort,
  Drop,
  EpicFreeGame,
  Game,
  PlatformId,
  PriorityItem,
  Settings,
} from './types';

export function epicGameArtCandidates(
  game: Pick<EpicFreeGame, 'bannerUrl' | 'boxArtUrl'>,
): string[] {
  const out: string[] = [];
  if (game.bannerUrl) out.push(game.bannerUrl);
  if (game.boxArtUrl && game.boxArtUrl !== game.bannerUrl) {
    out.push(game.boxArtUrl);
  }
  return out;
}

export function priorityItemMatchesGame(
  item: PriorityItem,
  gameId: string,
  gameName: string,
): item is { kind: 'game'; gameId: string; gameName: string } {
  if (item.kind !== 'game') return false;
  const cleanId = gameId.trim();
  const cleanName = gameName.trim().toLowerCase();
  if (cleanId && item.gameId === cleanId) return true;
  return Boolean(cleanName && item.gameName.trim().toLowerCase() === cleanName);
}

export function isCampaignFarmed(camp: Campaign, settings: Settings): boolean {
  const platSettings = settings[camp.platform];
  if (platSettings.excludedCampaigns.includes(camp.id)) return false;
  if (platSettings.farmCampaigns.includes(camp.id)) return true;
  return platSettings.priority.some((item) =>
    item.kind === 'campaign'
      ? item.campaignId === camp.id
      : priorityItemMatchesGame(item, camp.game.id, camp.game.name),
  );
}

export function isGamePrioritizedFor(camp: Campaign, settings: Settings): boolean {
  return settings[camp.platform].priority.some((item) =>
    priorityItemMatchesGame(item, camp.game.id, camp.game.name),
  );
}

export function isCampaignCompleted(camp: Campaign): boolean {
  return camp.drops.length > 0 && camp.drops.every((d) => d.claimed);
}

export function isCampaignActiveNow(camp: Campaign, now = Date.now()): boolean {
  if (camp.status !== 'active') return false;
  const endTs = new Date(camp.endsAt).getTime();
  return Number.isNaN(endTs) || endTs > now;
}

export function hasClaimableDrop(camp: Campaign): boolean {
  return camp.drops.some((d) => (d.claimable && !d.claimed) || d.needsManualClaim);
}

export function campaignProgressRatio(camp: Campaign): number {
  const req = camp.drops.reduce((s, d) => s + d.requiredMinutes, 0);
  const cur = camp.drops.reduce((s, d) => s + (d.claimed ? d.requiredMinutes : d.currentMinutes), 0);
  return req > 0 ? cur / req : 0;
}

export function campaignRemainingMinutes(camp: Campaign): number {
  if (camp.drops.length === 0) return Number.MAX_SAFE_INTEGER;
  return camp.drops.reduce(
    (sum, d) => (d.claimed ? sum : sum + Math.max(0, d.requiredMinutes - d.currentMinutes)),
    0,
  );
}

export function isCampaignConnected(camp: Campaign): boolean {
  return !camp.requiresLink || camp.linked;
}

export function isCampaignUnlinked(camp: Campaign): boolean {
  return camp.requiresLink && !camp.linked;
}

export function computeCampaignCounts(
  campaigns: Campaign[],
  settings: Settings,
  now = Date.now(),
): Record<CampaignFilter, number> {
  const liveOrCompleted = campaigns.filter(
    (c) => isCampaignActiveNow(c, now) || c.status === 'upcoming' || isCampaignCompleted(c) || hasClaimableDrop(c),
  );
  return {
    all: liveOrCompleted.length,
    active: campaigns.filter((c) => isCampaignActiveNow(c, now)).length,
    connected: liveOrCompleted.filter((c) => isCampaignConnected(c)).length,
    unlinked: liveOrCompleted.filter((c) => isCampaignUnlinked(c)).length,
    farming: campaigns.filter((c) => isCampaignActiveNow(c, now) && isCampaignFarmed(c, settings) && !isCampaignCompleted(c)).length,
    claimable: campaigns.filter((c) => hasClaimableDrop(c)).length,
    upcoming: campaigns.filter((c) => c.status === 'upcoming').length,
    completed: campaigns.filter((c) => isCampaignCompleted(c)).length,
  };
}

export function extractGameOptions(campaigns: Campaign[]): Array<{ name: string; count: number }> {
  const map = new Map<string, number>();
  for (const camp of campaigns) {
    const name = camp.game.name.trim();
    if (!name) continue;
    map.set(name, (map.get(name) ?? 0) + 1);
  }
  return Array.from(map.entries())
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

export function filterAndSortCampaigns(
  campaigns: Campaign[],
  settings: Settings,
  filter: CampaignFilter,
  sort: CampaignSort,
  search: string,
  selectedGame = 'all',
  now = Date.now(),
): Campaign[] {
  const q = search.trim().toLowerCase();
  const list = campaigns.filter((c) => {
    const activeNow = isCampaignActiveNow(c, now);
    const keepInGeneral = activeNow || c.status === 'upcoming' || isCampaignCompleted(c) || hasClaimableDrop(c);
    if (filter === 'all' && !keepInGeneral) return false;
    if (filter === 'active' && !activeNow) return false;
    if (filter === 'connected' && (!keepInGeneral || !isCampaignConnected(c))) return false;
    if (filter === 'unlinked' && (!keepInGeneral || !isCampaignUnlinked(c))) return false;
    if (filter === 'upcoming' && c.status !== 'upcoming') return false;
    if (filter === 'farming' && (!activeNow || !isCampaignFarmed(c, settings) || isCampaignCompleted(c))) return false;
    if (filter === 'claimable' && !hasClaimableDrop(c)) return false;
    if (filter === 'completed' && !isCampaignCompleted(c)) return false;
    if (selectedGame !== 'all' && c.game.name !== selectedGame) return false;
    if (q) {
      const matchesMeta =
        c.name.toLowerCase().includes(q) || c.game.name.toLowerCase().includes(q);
      const matchesDrop = c.drops.some(
        (d) =>
          d.name.toLowerCase().includes(q) ||
          d.rewards.some((r) => r.name.toLowerCase().includes(q)),
      );
      return matchesMeta || matchesDrop;
    }
    return true;
  });

  return [...list].sort((a, b) => {
    if (sort === 'name') {
      return a.game.name.localeCompare(b.game.name) || a.name.localeCompare(b.name);
    }
    if (sort === 'progress') {
      return campaignProgressRatio(b) - campaignProgressRatio(a);
    }
    if (sort === 'shortest') {
      return campaignRemainingMinutes(a) - campaignRemainingMinutes(b);
    }
    if (sort === 'rewards') {
      return b.drops.length - a.drops.length;
    }
    return new Date(a.endsAt).getTime() - new Date(b.endsAt).getTime();
  });
}

export interface GameDropGroup {
  key: string;
  platform: PlatformId;
  game: Game;
  campaigns: Campaign[];
  totalDrops: number;
  claimedDrops: number;
  claimableCount: number;
  soonestEndsAt: string;
  soonestStartsAt: string;
  hasActive: boolean;
  hasUpcoming: boolean;
  allCompleted: boolean;
  linkState: 'ready' | 'linked' | 'unlinked' | 'partial';
  linkedCount: number;
  linkRequiredCount: number;
  connectTargetUrl: string;
  isGamePrioritized: boolean;
  isFarming: boolean;
  hasExcluded: boolean;
}

export function groupCampaignsByGame(
  campaigns: Campaign[],
  settings: Settings,
  now = Date.now(),
): GameDropGroup[] {
  const buckets = new Map<string, Campaign[]>();
  for (const camp of campaigns) {
    const cleanName = camp.game.name.trim().toLowerCase();
    const cleanId = camp.game.id.trim().toLowerCase();
    const gameKey = cleanName || cleanId || camp.id.toLowerCase();
    const key = `${camp.platform}:${gameKey}`;
    const existing = buckets.get(key);
    if (existing) {
      existing.push(camp);
    } else {
      buckets.set(key, [camp]);
    }
  }

  const out: GameDropGroup[] = [];
  for (const [key, list] of buckets.entries()) {
    const first = list[0];
    const bestBoxArt = list.find((c) => Boolean(c.game.boxArtUrl))?.game.boxArtUrl ?? null;
    const game: Game = {
      ...first.game,
      boxArtUrl: bestBoxArt,
    };

    let totalDrops = 0;
    let claimedDrops = 0;
    let claimableCount = 0;
    let hasActive = false;
    let hasUpcoming = false;
    let allCompleted = true;

    let soonestEndsAt = first.endsAt;
    let soonestEndTs = Date.parse(first.endsAt);
    let soonestStartsAt = first.startsAt;
    let soonestStartTs = Date.parse(first.startsAt);

    for (const c of list) {
      totalDrops += c.drops.length;
      for (const d of c.drops) {
        if (d.claimed) claimedDrops += 1;
        if ((d.claimable && !d.claimed) || d.needsManualClaim) claimableCount += 1;
      }
      if (isCampaignActiveNow(c, now)) hasActive = true;
      if (c.status === 'upcoming') hasUpcoming = true;
      if (!isCampaignCompleted(c)) allCompleted = false;

      const endTs = Date.parse(c.endsAt);
      if (Number.isFinite(endTs) && (!Number.isFinite(soonestEndTs) || endTs < soonestEndTs)) {
        soonestEndTs = endTs;
        soonestEndsAt = c.endsAt;
      }
      const startTs = Date.parse(c.startsAt);
      if (Number.isFinite(startTs) && (!Number.isFinite(soonestStartTs) || startTs < soonestStartTs)) {
        soonestStartTs = startTs;
        soonestStartsAt = c.startsAt;
      }
    }

    const linkRequired = list.filter((c) => c.requiresLink);
    const linkedCount = linkRequired.filter((c) => c.linked).length;
    const linkRequiredCount = linkRequired.length;
    const linkState: GameDropGroup['linkState'] =
      linkRequiredCount === 0
        ? 'ready'
        : linkedCount === linkRequiredCount
          ? 'linked'
          : linkedCount === 0
            ? 'unlinked'
            : 'partial';

    const unlinkedCamp = linkRequired.find((c) => !c.linked);
    const defaultLinkUrl =
      first.platform === 'twitch'
        ? 'https://www.twitch.tv/settings/connections'
        : 'https://kick.com/drops/inventory';
    const connectTargetUrl =
      unlinkedCamp?.linkUrl || list.find((c) => c.linkUrl)?.linkUrl || defaultLinkUrl;

    const isGamePrioritized = list.some((c) => isGamePrioritizedFor(c, settings));
    const eligible = list.filter((c) => !isCampaignCompleted(c));
    const targetList = eligible.length > 0 ? eligible : list;
    const isFarming =
      targetList.length > 0 && targetList.every((c) => isCampaignFarmed(c, settings));
    const hasExcluded = list.some((c) =>
      settings[c.platform].excludedCampaigns.includes(c.id),
    );

    out.push({
      key,
      platform: first.platform,
      game,
      campaigns: list,
      totalDrops,
      claimedDrops,
      claimableCount,
      soonestEndsAt,
      soonestStartsAt,
      hasActive,
      hasUpcoming,
      allCompleted,
      linkState,
      linkedCount,
      linkRequiredCount,
      connectTargetUrl,
      isGamePrioritized,
      isFarming,
      hasExcluded,
    });
  }

  return out;
}

export function countSearchMatchesIgnoringFilters(
  campaigns: Campaign[],
  search: string,
): number {
  const q = search.trim().toLowerCase();
  if (!q) return 0;
  return campaigns.filter((c) => {
    const matchesMeta =
      c.name.toLowerCase().includes(q) || c.game.name.toLowerCase().includes(q);
    const matchesDrop = c.drops.some(
      (d) =>
        d.name.toLowerCase().includes(q) ||
        d.rewards.some((r) => r.name.toLowerCase().includes(q)),
    );
    return matchesMeta || matchesDrop;
  }).length;
}

export interface UpNextItem {
  platform: PlatformId;
  campaign: Campaign;
  drop: Drop;
  priorityRank: number;
  totalDrops: number;
  claimedDrops: number;
  remainingDrops: number;
  currentMinutes: number;
  requiredMinutes: number;
}

export interface EndingSoonItem {
  platform: PlatformId;
  campaign: Campaign;
  msRemaining: number;
  urgent: boolean;
}

export interface DashboardStats {
  claimedToday: number;
  claimedTotal: number;
  activeCampaigns: number;
  claimableNow: number;
  sessionMinutes: number;
  nextRefreshIso: string | null;
}

const PALETTE: Array<{ bg: string; fg: string }> = [
  { bg: '#2B3058', fg: '#B9C4FF' },
  { bg: '#3B2854', fg: '#D4B5FF' },
  { bg: '#1F3E37', fg: '#7CE8BA' },
  { bg: '#47331D', fg: '#FFD085' },
  { bg: '#1D394F', fg: '#8ECAFF' },
  { bg: '#4A242D', fg: '#FF9EAA' },
];

export function monogramFor(name: string): { initials: string; bg: string; fg: string } {
  const clean = name.trim();
  if (!clean) return { initials: 'DP', bg: PALETTE[0].bg, fg: PALETTE[0].fg };
  const parts = clean.split(/[\s_-]+/).filter(Boolean);
  const initials =
    parts.length >= 2
      ? (parts[0][0] + parts[1][0]).toUpperCase()
      : clean.slice(0, 2).toUpperCase();

  let hash = 0;
  for (let i = 0; i < clean.length; i++) {
    hash = (hash * 31 + clean.charCodeAt(i)) | 0;
  }
  const slot = PALETTE[Math.abs(hash) % PALETTE.length];
  return { initials, bg: slot.bg, fg: slot.fg };
}

export function earliestNextActionAt(snapshot: AppSnapshot): string | null {
  const candidates = [snapshot.twitch.nextActionAt, snapshot.kick.nextActionAt]
    .filter((v): v is string => Boolean(v))
    .map((iso) => ({ iso, ts: new Date(iso).getTime() }))
    .filter((x) => !Number.isNaN(x.ts))
    .sort((a, b) => a.ts - b.ts);
  return candidates[0]?.iso ?? null;
}

export function computeDashboardStats(
  snapshot: AppSnapshot,
  sessionMinutes: number,
  now = Date.now(),
): DashboardStats {
  const startOfDay = new Date(now);
  startOfDay.setHours(0, 0, 0, 0);
  const dayMs = startOfDay.getTime();

  const claimedToday = snapshot.recentClaims.filter((c) => {
    const ts = new Date(c.claimedAt).getTime();
    return !Number.isNaN(ts) && ts >= dayMs;
  }).length;

  const allCampaigns = [...snapshot.twitch.campaigns, ...snapshot.kick.campaigns];
  const activeCampaigns = allCampaigns.filter((c) => isCampaignActiveNow(c, now)).length;
  const claimableNow = allCampaigns.reduce(
    (sum, c) => sum + c.drops.filter((d) => (d.claimable && !d.claimed) || d.needsManualClaim).length,
    0,
  );

  return {
    claimedToday,
    claimedTotal: snapshot.recentClaims.length,
    activeCampaigns,
    claimableNow,
    sessionMinutes,
    nextRefreshIso: earliestNextActionAt(snapshot),
  };
}

export function computeUpNextQueue(
  snapshot: AppSnapshot,
  settings: Settings,
  now = Date.now(),
  limit = 4,
): UpNextItem[] {
  const out: UpNextItem[] = [];

  for (const plat of ['twitch', 'kick'] as const) {
    const platSnap = snapshot[plat];
    const platSettings = settings[plat];
    const eligible = platSnap.campaigns.filter((c) => {
      if (!isCampaignActiveNow(c, now)) return false;
      if (platSettings.excludedCampaigns.includes(c.id)) return false;
      const explicitlyChosen = isCampaignFarmed(c, settings);
      if (c.requiresLink && !c.linked && !settings.farmUnlinked && !(plat === 'twitch' && explicitlyChosen)) {
        return false;
      }
      return c.drops.length === 0 || c.drops.some((d) => !d.claimed && !d.needsManualClaim);
    });

    const scored = eligible
      .map((camp) => {
        let rank = 900;
        platSettings.priority.forEach((p, idx) => {
          if (p.kind === 'campaign' && p.campaignId === camp.id) rank = Math.min(rank, idx);
          if (priorityItemMatchesGame(p, camp.game.id, camp.game.name)) rank = Math.min(rank, idx);
        });
        if (rank === 900 && platSettings.farmCampaigns.includes(camp.id)) rank = 500;
        if (rank === 900 && settings.idleBehavior !== 'farm_any') return null;
        return { camp, rank };
      })
      .filter((x): x is { camp: Campaign; rank: number } => x !== null)
      .sort((a, b) => a.rank - b.rank || new Date(a.camp.endsAt).getTime() - new Date(b.camp.endsAt).getTime());

    for (const item of scored) {
      // Skip the campaign currently being farmed on this platform; it is already shown in Now Farming.
      if (platSnap.currentCampaignId === item.camp.id && platSnap.watching) {
        continue;
      }

      if (item.camp.drops.length === 0) {
        out.push({
          platform: plat,
          campaign: item.camp,
          drop: {
            id: `${item.camp.id}-summary`,
            instanceId: null,
            name: item.camp.name,
            rewards: [{ name: item.camp.name, imageUrl: item.camp.game.boxArtUrl }],
            requiredMinutes: 60,
            currentMinutes: 0,
            claimable: false,
            claimed: false,
            needsManualClaim: false,
          },
          priorityRank: item.rank,
          totalDrops: 0,
          claimedDrops: 0,
          remainingDrops: 0,
          currentMinutes: 0,
          requiredMinutes: 60,
        });
        if (out.length >= limit * 2) break;
        continue;
      }

      const sortedDrops = [...item.camp.drops].sort(
        (a, b) => a.requiredMinutes - b.requiredMinutes,
      );
      const remaining = sortedDrops.filter((d) => !d.claimed && !d.needsManualClaim);
      const nextDrop = remaining[0];
      if (!nextDrop) continue;

      const totalDrops = sortedDrops.length;
      const claimedDrops = sortedDrops.filter((d) => d.claimed).length;
      const requiredMinutes = Math.max(
        1,
        ...sortedDrops.map((d) => Math.max(1, d.requiredMinutes)),
      );
      const maxRemainingMins = Math.max(
        0,
        ...remaining.map((d) => Math.max(0, d.requiredMinutes - d.currentMinutes)),
      );
      const currentMinutes = Math.max(0, requiredMinutes - maxRemainingMins);

      out.push({
        platform: plat,
        campaign: item.camp,
        drop: nextDrop,
        priorityRank: item.rank,
        totalDrops,
        claimedDrops,
        remainingDrops: remaining.length,
        currentMinutes,
        requiredMinutes,
      });
      if (out.length >= limit * 2) break;
    }
  }

  return out
    .sort((a, b) => a.priorityRank - b.priorityRank)
    .slice(0, limit);
}

export function computeEndingSoon(
  snapshot: AppSnapshot,
  now = Date.now(),
  windowHours = 48,
): EndingSoonItem[] {
  const maxMs = windowHours * 3_600_000;
  const items: EndingSoonItem[] = [];

  for (const plat of ['twitch', 'kick'] as const) {
    for (const camp of snapshot[plat].campaigns) {
      if (camp.status !== 'active') continue;
      if (isCampaignCompleted(camp)) continue;
      const msRemaining = new Date(camp.endsAt).getTime() - now;
      if (msRemaining > 0 && msRemaining <= maxMs) {
        items.push({
          platform: plat,
          campaign: camp,
          msRemaining,
          urgent: msRemaining <= 12 * 3_600_000,
        });
      }
    }
  }

  return items.sort((a, b) => a.msRemaining - b.msRemaining).slice(0, 6);
}

export interface LiveSkinMilestone {
  id: string;
  skinName: string;
  imageUrl: string | null;
  requiredMinutes: number;
  liveMinutes: number;
  remainingSeconds: number;
  claimed: boolean;
  claimable: boolean;
}

export interface LiveCampaignProgress {
  isLiveTicking: boolean;
  isMultiDrop: boolean;
  totalDrops: number;
  completedDrops: number;
  nextSkin: LiveSkinMilestone | null;
  wholeRequiredMinutes: number;
  wholeLiveMinutes: number;
  wholeRemainingSeconds: number;
  milestones: LiveSkinMilestone[];
}

export function getCampaignTotalMinutes(campaign: Campaign): number {
  if (campaign.drops.length === 0) return 0;
  return Math.max(...campaign.drops.map((d) => d.requiredMinutes));
}

export function computeLiveCampaignProgress(
  campaign: Campaign | null,
  activeDropId: string | null,
  isMining: boolean,
  watchingSinceIso: string | null | undefined,
  progressSyncedAtIso: string | null | undefined,
  nowMs: number,
): LiveCampaignProgress | null {
  if (!campaign || campaign.drops.length === 0) return null;

  let liveDeltaMins = 0;
  if (isMining) {
    const syncTs = progressSyncedAtIso ? new Date(progressSyncedAtIso).getTime() : NaN;
    const watchTs = watchingSinceIso ? new Date(watchingSinceIso).getTime() : NaN;
    const anchorTs =
      !Number.isNaN(syncTs) && !Number.isNaN(watchTs)
        ? Math.max(syncTs, watchTs)
        : !Number.isNaN(syncTs)
          ? syncTs
          : !Number.isNaN(watchTs)
            ? watchTs
            : NaN;
    if (!Number.isNaN(anchorTs) && nowMs > anchorTs) {
      // Smooth live interpolation across 5m-10m server progress polls so the timer never stalls.
      liveDeltaMins = Math.min(10.5, (nowMs - anchorTs) / 60_000);
    }
  }

  const milestones: LiveSkinMilestone[] = [...campaign.drops]
    .sort((a, b) => a.requiredMinutes - b.requiredMinutes)
    .map((d) => {
      const req = Math.max(1, d.requiredMinutes);
      const baseCur = d.claimed ? req : d.currentMinutes;
      const liveMinutes = d.claimed || d.claimable ? req : Math.min(req, baseCur + liveDeltaMins);
      const remainingSeconds = d.claimed || d.claimable ? 0 : Math.max(0, (req - liveMinutes) * 60);
      const skinName = d.rewards[0]?.name || d.name;
      const imageUrl = d.rewards[0]?.imageUrl ?? null;
      return {
        id: d.id,
        skinName,
        imageUrl,
        requiredMinutes: req,
        liveMinutes,
        remainingSeconds,
        claimed: d.claimed,
        claimable: d.claimable || (!d.claimed && liveMinutes >= req),
      };
    });

  const unclaimed = milestones.filter((m) => !m.claimed);
  const completedDrops = milestones.length - unclaimed.length;

  let nextSkin: LiveSkinMilestone | null = null;
  if (unclaimed.length > 0) {
    const sortedBySoonest = [...unclaimed].sort(
      (a, b) =>
        a.remainingSeconds - b.remainingSeconds ||
        (a.id === activeDropId ? -1 : b.id === activeDropId ? 1 : 0),
    );
    nextSkin = sortedBySoonest[0] ?? null;
  } else {
    nextSkin = milestones[milestones.length - 1] ?? null;
  }

  // Concurrent time-based tiers (Twitch & Kick) accumulate watch time simultaneously up to the max milestone.
  const wholeRequiredMinutes = Math.max(...milestones.map((m) => m.requiredMinutes));
  const wholeRemainingSeconds =
    unclaimed.length > 0 ? Math.max(...unclaimed.map((m) => m.remainingSeconds)) : 0;
  const wholeLiveMinutes = Math.max(0, wholeRequiredMinutes - wholeRemainingSeconds / 60);

  return {
    isLiveTicking: isMining && unclaimed.length > 0,
    isMultiDrop: milestones.length > 1,
    totalDrops: milestones.length,
    completedDrops,
    nextSkin,
    wholeRequiredMinutes,
    wholeLiveMinutes,
    wholeRemainingSeconds,
    milestones,
  };
}
