import assert from 'node:assert/strict';
import test from 'node:test';
import { normalizeSnapshot } from './snapshot.ts';

function platformSnapshot(platform, login) {
  return {
    platform,
    state: 'Idle',
    stateReason: null,
    account: { id: login, login, displayName: login, avatarUrl: null },
    watching: null,
    currentCampaignId: null,
    currentDropId: null,
    watchingSince: null,
    progressSyncedAt: null,
    nextActionAt: null,
    lastError: null,
    campaigns: [],
    candidateChannels: [],
    pinnedChannel: null,
  };
}

test('rehydrates only active profile snapshots omitted by the backend', () => {
  const twitch = platformSnapshot('twitch', 'active-twitch');
  const kick = platformSnapshot('kick', 'active-kick');
  const inactive = platformSnapshot('twitch', 'inactive-twitch');
  const wire = {
    twitch,
    kick,
    profiles: [
      { profileId: 'active-tw', label: 'Main', platform: 'twitch' },
      { profileId: 'inactive-tw', label: 'Alt', platform: 'twitch', snapshot: inactive },
      { profileId: 'active-kk', label: 'Main', platform: 'kick' },
    ],
    recentClaims: [],
    mockActive: false,
  };

  const normalized = normalizeSnapshot(wire);

  assert.equal(normalized.profiles?.[0].snapshot, twitch);
  assert.equal(normalized.profiles?.[1].snapshot, inactive);
  assert.equal(normalized.profiles?.[2].snapshot, kick);
  assert.notEqual(normalized.profiles?.[0].snapshot, inactive);
});
