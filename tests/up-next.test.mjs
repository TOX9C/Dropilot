import test from 'node:test';
import assert from 'node:assert/strict';

import { computeUpNextQueue } from '../src/lib/selectors.ts';

const NOW = Date.parse('2026-01-01T00:00:00Z');
const NAMES = ['Overwatch', 'Rust', 'VALORANT', 'Fortnite', 'Escape from Tarkov'];

function makeCampaign(id, gameId, gameName, endHours, requiredMinutes = 60, currentMinutes = 15) {
  return {
    id,
    platform: 'twitch',
    game: { id: gameId, name: gameName, boxArtUrl: null },
    name: `${gameName} Campaign`,
    startsAt: new Date(NOW - 3_600_000).toISOString(),
    endsAt: new Date(NOW + endHours * 3_600_000).toISOString(),
    status: 'active',
    requiresLink: true,
    linked: true,
    linkVerified: true,
    linkUrl: null,
    allowedChannels: null,
    drops: [{
      id: `${id}-drop`,
      instanceId: null,
      name: 'Skin',
      rewards: [{ name: 'Skin', imageUrl: null }],
      requiredMinutes,
      currentMinutes,
      claimable: false,
      claimed: false,
      needsManualClaim: false,
    }],
  };
}

function makeSettings(campaigns, prioritizeEndingSoon) {
  const twitch = {
    enabled: true,
    priority: campaigns.map((campaign) => ({
      kind: 'game',
      gameId: campaign.game.id,
      gameName: campaign.game.name,
    })),
    farmCampaigns: [],
    excludedCampaigns: [],
    pinnedChannel: null,
    prioritizeEndingSoon,
  };
  return {
    farmUnlinked: false,
    idleBehavior: 'stay_idle',
    twitch,
    kick: { ...twitch, priority: [] },
  };
}

function makeSnapshot(campaigns) {
  const platform = (id, activeCampaigns) => ({
    platform: id,
    state: 'Idle',
    stateReason: null,
    account: null,
    watching: null,
    currentCampaignId: null,
    currentDropId: null,
    nextActionAt: null,
    lastError: null,
    campaigns: activeCampaigns,
    candidateChannels: [],
    pinnedChannel: null,
  });
  return {
    twitch: platform('twitch', campaigns),
    kick: platform('kick', []),
    recentClaims: [],
    mockActive: true,
  };
}

function queueIds(campaigns, prioritizeEndingSoon, limit = 4) {
  return computeUpNextQueue(
    makeSnapshot(campaigns),
    makeSettings(campaigns, prioritizeEndingSoon),
    NOW,
    limit,
  ).map((item) => item.campaign.id);
}

test('Up next applies the ending-soon boost and preserves strict order when disabled', () => {
  const endHours = [120, 96, 72, 48, 4];
  const campaigns = NAMES.map((name, index) =>
    makeCampaign(`g${index + 1}`, String(index + 1), name, endHours[index]));

  assert.deepEqual(queueIds(campaigns, true), ['g5', 'g1', 'g2', 'g3']);
  assert.deepEqual(queueIds(campaigns, false), ['g1', 'g2', 'g3', 'g4']);
});

test('Up next protects tight higher priorities and does not boost unwinnable campaigns', () => {
  const tightTop = makeCampaign('top', '1', 'Overwatch', 3, 150, 0);
  const urgentLow = makeCampaign('low', '2', 'Escape from Tarkov', 2, 90, 0);
  assert.deepEqual(queueIds([tightTop, urgentLow], true, 2), ['top', 'low']);

  const roomyTop = makeCampaign('top', '1', 'Overwatch', 10, 150, 0);
  assert.deepEqual(queueIds([roomyTop, urgentLow], true, 2), ['low', 'top']);

  const unwinnable = {
    ...urgentLow,
    endsAt: new Date(NOW + 15 * 60_000).toISOString(),
  };
  assert.deepEqual(queueIds([roomyTop, unwinnable], true, 2), ['top', 'low']);
});
