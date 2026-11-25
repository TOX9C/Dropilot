import type { AppSnapshot, PlatformSnapshot, ProfileSnapshot } from './types';

export type WireProfileSnapshot = Omit<ProfileSnapshot, 'snapshot'> & {
  snapshot?: PlatformSnapshot;
};

export type WireAppSnapshot = Omit<AppSnapshot, 'profiles'> & {
  profiles?: WireProfileSnapshot[];
};

export function normalizeSnapshot(wire: WireAppSnapshot): AppSnapshot {
  return {
    ...wire,
    profiles: wire.profiles?.map((profile) => ({
      ...profile,
      snapshot: profile.snapshot ?? wire[profile.platform],
    })),
  };
}
