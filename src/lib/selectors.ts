import type {
  AppSnapshot,
  Campaign,
  CampaignFilter,
  CampaignSort,
  Drop,
  PlatformId,
  Settings,
} from './types';

export function isCampaignFarmed(camp: Campaign, settings: Settings): boolean {
  const platSettings = settings[camp.platform];
  if (platSettings.excludedCampaigns.includes(camp.id)) return false;
  if (platSettings.farmCampaigns.includes(camp.id)) return true;
  return platSettings.priority.some((item) =>
    item.kind === 'campaign' ? item.campaignId === camp.id : item.gameId === camp.game.id,
  );
}

export function isGamePrioritizedFor(camp: Campaign, settings: Settings): boolean {
  return settings[camp.platform].priority.some(
    (item) => item.kind === 'game' && item.gameId === camp.game.id,
  );
}

export function isCampaignCompleted(camp: Campaign): boolean {
  return camp.drops.length > 0 && camp.drops.every((d) => d.claimed);
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
  return !camp.requiresLink || (Boolean(camp.linkVerified) && camp.linked);
}

export function isCampaignUnlinked(camp: Campaign): boolean {
  return camp.requiresLink && camp.linkVerified !== false && !camp.linked;
}

export function computeCampaignCounts(
  campaigns: Campaign[],
  settings: Settings,
): Record<CampaignFilter, number> {
  return {
    all: campaigns.length,
    active: campaigns.filter((c) => c.status === 'active').length,
    connected: campaigns.filter((c) => isCampaignConnected(c)).length,
    unlinked: campaigns.filter((c) => isCampaignUnlinked(c)).length,
    farming: campaigns.filter((c) => isCampaignFarmed(c, settings) && !isCampaignCompleted(c)).length,
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
): Campaign[] {
  const q = search.trim().toLowerCase();
  const list = campaigns.filter((c) => {
    if (filter === 'active' && c.status !== 'active') return false;
    if (filter === 'connected' && !isCampaignConnected(c)) return false;
    if (filter === 'unlinked' && !isCampaignUnlinked(c)) return false;
    if (filter === 'upcoming' && c.status !== 'upcoming') return false;
    if (filter === 'farming' && (!isCampaignFarmed(c, settings) || isCampaignCompleted(c))) return false;
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

export interface UpNextItem {
  platform: PlatformId;
  campaign: Campaign;
  drop: Drop;
  priorityRank: number;
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
  const activeCampaigns = allCampaigns.filter((c) => c.status === 'active').length;
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
      if (c.status !== 'active') return false;
      if (new Date(c.endsAt).getTime() <= now) return false;
      if (platSettings.excludedCampaigns.includes(c.id)) return false;
      if (c.requiresLink && !c.linked && !settings.farmUnlinked) return false;
      return c.drops.some((d) => !d.claimed && !d.needsManualClaim);
    });

    const scored = eligible
      .map((camp) => {
        let rank = 900;
        platSettings.priority.forEach((p, idx) => {
          if (p.kind === 'campaign' && p.campaignId === camp.id) rank = Math.min(rank, idx);
          if (p.kind === 'game' && p.gameId === camp.game.id) rank = Math.min(rank, idx);
        });
        if (rank === 900 && platSettings.farmCampaigns.includes(camp.id)) rank = 500;
        if (rank === 900 && settings.idleBehavior !== 'farm_any') return null;
        return { camp, rank };
      })
      .filter((x): x is { camp: Campaign; rank: number } => x !== null)
      .sort((a, b) => a.rank - b.rank || new Date(a.camp.endsAt).getTime() - new Date(b.camp.endsAt).getTime());

    for (const item of scored) {
      for (const drop of item.camp.drops) {
        if (drop.claimed || drop.needsManualClaim) continue;
        // Skip the exact drop currently being mined so "Up next" shows upcoming items
        if (platSnap.currentCampaignId === item.camp.id && platSnap.currentDropId === drop.id) {
          continue;
        }
        out.push({
          platform: plat,
          campaign: item.camp,
          drop,
          priorityRank: item.rank,
        });
        if (out.length >= limit * 2) break;
      }
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
