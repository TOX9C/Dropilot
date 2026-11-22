<script lang="ts">
  import Clock from 'lucide-svelte/icons/clock';
  import Gift from 'lucide-svelte/icons/gift';
  import LogIn from 'lucide-svelte/icons/log-in';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Sparkles from 'lucide-svelte/icons/sparkles';
  import X from 'lucide-svelte/icons/x';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import CampaignCard from '../components/CampaignCard.svelte';
  import FilterBar from '../components/FilterBar.svelte';
  import GamePicker from '../components/GamePicker.svelte';
  import { api } from '../lib/api';
  import { slowNowMs } from '../lib/clock';
  import {
    computeCampaignCounts,
    extractGameOptions,
    filterAndSortCampaigns,
    isCampaignCompleted,
    isCampaignFarmed,
    isGamePrioritizedFor,
    priorityItemMatchesGame,
  } from '../lib/selectors';
  import {
    addWatchedGameAction,
    appSettings,
    dropsViewMode,
    notifyUi,
    toggleCampaignFarmAction,
    toggleGameFarmAction,
    updateSnapshotCampaign,
  } from '../lib/stores';
  import type {
    AppSnapshot,
    Campaign,
    CampaignFilter,
    CampaignSort,
    PlatformFilter,
    PlatformId,
    Settings,
  } from '../lib/types';

  interface Props {
    snapshot: AppSnapshot;
    settings: Settings;
  }

  let { snapshot, settings }: Props = $props();

  let platformFilter = $state<PlatformFilter>('all');
  let filter = $state<CampaignFilter>('all');
  let sort = $state<CampaignSort>('ending_soon');
  let search = $state('');
  let selectedGame = $state('all');
  let watchScope = $state<'both' | 'twitch' | 'kick'>('both');
  let addingWatch = $state(false);

  const allCampaigns = $derived([...snapshot.twitch.campaigns, ...snapshot.kick.campaigns]);

  interface WatchedGameChip {
    key: string;
    name: string;
    twitchId: string | null;
    kickId: string | null;
    hasLiveDrop: boolean;
  }

  const watchedGames = $derived.by<WatchedGameChip[]>(() => {
    const map = new Map<string, WatchedGameChip>();
    for (const plat of ['twitch', 'kick'] as const) {
      for (const item of settings[plat].priority) {
        if (item.kind !== 'game') continue;
        const cleanName = item.gameName.trim();
        const key = (cleanName || item.gameId).toLowerCase();
        if (!key) continue;
        const existing = map.get(key) ?? {
          key,
          name: cleanName || item.gameId,
          twitchId: null,
          kickId: null,
          hasLiveDrop: false,
        };
        if (plat === 'twitch') existing.twitchId = item.gameId;
        if (plat === 'kick') existing.kickId = item.gameId;
        const activeNow = snapshot[plat].campaigns.some(
          (c) => c.status === 'active' && priorityItemMatchesGame(item, c.game.id, c.game.name),
        );
        if (activeNow) existing.hasLiveDrop = true;
        map.set(key, existing);
      }
    }
    return Array.from(map.values());
  });

  const platformCounts = $derived<Record<PlatformFilter, number>>({
    all: allCampaigns.length,
    twitch: snapshot.twitch.campaigns.length,
    kick: snapshot.kick.campaigns.length,
  });

  const scopedCampaigns = $derived(
    platformFilter === 'all' ? allCampaigns : snapshot[platformFilter].campaigns,
  );

  const counts = $derived(computeCampaignCounts(scopedCampaigns, settings, $slowNowMs));
  const games = $derived(extractGameOptions(scopedCampaigns));
  const filteredCampaigns = $derived(
    filterAndSortCampaigns(scopedCampaigns, settings, filter, sort, search, selectedGame, $slowNowMs),
  );

  const endingUnder24h = $derived(
    allCampaigns.filter((c) => {
      if (c.status !== 'active' || isCampaignCompleted(c)) return false;
      const diff = new Date(c.endsAt).getTime() - $slowNowMs;
      return diff > 0 && diff < 24 * 3_600_000;
    }).length,
  );

  const farmingCount = $derived(
    allCampaigns.filter((c) => isCampaignFarmed(c, settings) && !isCampaignCompleted(c)).length,
  );

  const needTwitchNotice = $derived(
    !snapshot.twitch.account && snapshot.twitch.campaigns.length === 0,
  );
  const needKickNotice = $derived(
    !snapshot.kick.account && snapshot.kick.campaigns.length === 0,
  );

  $effect(() => {
    if (selectedGame !== 'all' && !games.some((g) => g.name === selectedGame)) {
      selectedGame = 'all';
    }
  });

  async function refreshAll() {
    try {
      await Promise.all([api.retryPlatform('twitch'), api.retryPlatform('kick')]);
      notifyUi('Refreshed Twitch and Kick drops');
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleLogin(platform: PlatformId) {
    try {
      await api.login(platform);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleToggleFarm(camp: Campaign, farm: boolean) {
    try {
      await toggleCampaignFarmAction(camp, farm, Boolean(snapshot[camp.platform].account));
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleToggleGameFarm(camp: Campaign, farm: boolean) {
    try {
      await toggleGameFarmAction(camp, farm, Boolean(snapshot[camp.platform].account));
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleOpenUrl(url: string) {
    try {
      await api.openUrl(url);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleOpenInventory(platform: PlatformId) {
    try {
      await api.openDropsInventory(platform);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleSelectWatchedGame(gameName: string) {
    const clean = gameName.trim();
    if (!clean || addingWatch) return;
    addingWatch = true;
    try {
      await addWatchedGameAction(clean, watchScope, snapshot);
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      addingWatch = false;
    }
  }

  async function handleRemoveWatchedGame(chip: WatchedGameChip) {
    try {
      let latest = settings;
      if (chip.twitchId !== null) {
        latest = await api.setGameFarm('twitch', chip.twitchId, chip.name, false);
      }
      if (chip.kickId !== null) {
        latest = await api.setGameFarm('kick', chip.kickId, chip.name, false);
      }
      appSettings.set(latest);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleFetchDetails(camp: Campaign) {
    const detailed = await api.fetchCampaignDetails(camp.platform, camp.id);
    updateSnapshotCampaign(detailed);
  }
</script>

<div class="page">
  <header class="page-header">
    <div class="page-title-wrap">
      <div class="page-icon page-icon-pink"><Gift size={18} /></div>
      <div>
        <h1>All Drops</h1>
        <p class="page-subtitle">Browse, filter, and pick Twitch &amp; Kick drops with live end timers</p>
      </div>
    </div>

    <div class="page-actions">
      <Badge tone="accent">{counts.active} active</Badge>
      <Badge tone="success">{farmingCount} farming</Badge>
      {#if endingUnder24h > 0}
        <Badge tone="warning">
          <Clock size={11} />
          <span>{endingUnder24h} ending &lt;24h</span>
        </Badge>
      {/if}
      {#if needTwitchNotice}
        <Button variant="twitch" size="sm" onclick={() => handleLogin('twitch')}>
          <LogIn size={13} />
          <span>Connect Twitch</span>
        </Button>
      {/if}
      {#if needKickNotice}
        <Button variant="kick" size="sm" onclick={() => handleLogin('kick')}>
          <LogIn size={13} />
          <span>Connect Kick</span>
        </Button>
      {/if}
      <Button variant="secondary" size="sm" onclick={refreshAll}>
        <RefreshCw size={14} />
        <span>Refresh</span>
      </Button>
    </div>
  </header>

  <section class="card watchlist-bar" aria-label="Auto-mine game watchlist">
    <div class="wl-top">
      <div class="wl-title">
        <Sparkles size={14} />
        <strong>Auto-Mine Watchlist</strong>
        <span class="wl-sub">Add any game — Dropilot automatically mines it whenever drops go live</span>
      </div>

      <div class="wl-form">
        <div class="wl-picker-wrap">
          <GamePicker
            campaigns={allCampaigns}
            watchedNames={watchedGames.map((w) => w.name)}
            placeholder="Search or add any game (e.g. Escape from Tarkov, Rust, VALORANT)..."
            disabled={addingWatch}
            buttonLabel="Watch game"
            onSelect={handleSelectWatchedGame}
          />
        </div>

        <div class="wl-scope" role="group" aria-label="Watchlist platform">
          <button type="button" class="scope-pill" class:active={watchScope === 'both'} onclick={() => (watchScope = 'both')}>Both</button>
          <button type="button" class="scope-pill" class:active={watchScope === 'twitch'} onclick={() => (watchScope = 'twitch')}>Twitch</button>
          <button type="button" class="scope-pill" class:active={watchScope === 'kick'} onclick={() => (watchScope = 'kick')}>Kick</button>
        </div>
      </div>
    </div>

    {#if watchedGames.length > 0}
      <div class="wl-chips">
        {#each watchedGames as chip (chip.key)}
          <div class="wl-chip" class:wl-chip-live={chip.hasLiveDrop}>
            <span class="wl-dots" aria-hidden="true">
              {#if chip.twitchId !== null}<span class="plat-dot dot-twitch" title="Twitch"></span>{/if}
              {#if chip.kickId !== null}<span class="plat-dot dot-kick" title="Kick"></span>{/if}
            </span>
            <span class="wl-chip-name">{chip.name}</span>
            <span class="wl-chip-status" class:status-live={chip.hasLiveDrop}>
              {chip.hasLiveDrop ? 'Live drop' : 'Waiting'}
            </span>
            <button
              type="button"
              class="wl-chip-rm"
              aria-label={`Remove ${chip.name} from watchlist`}
              onclick={() => handleRemoveWatchedGame(chip)}
            >
              <X size={11} />
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </section>

  <FilterBar
    {filter}
    {sort}
    {search}
    {counts}
    {platformFilter}
    {platformCounts}
    {games}
    {selectedGame}
    viewMode={$dropsViewMode}
    onplatform={(next) => (platformFilter = next)}
    onfilter={(next) => (filter = next)}
    onsort={(next) => (sort = next)}
    onsearch={(next) => (search = next)}
    ongame={(next) => (selectedGame = next)}
    onviewmode={(next) => dropsViewMode.set(next)}
  />

  {#if filteredCampaigns.length === 0}
    <div class="card empty-state">
      <Gift size={24} />
      {#if allCampaigns.length === 0}
        <strong>No drops loaded yet</strong>
        <p>Log in to Twitch or Kick above, or hit Refresh to check for active campaigns.</p>
        <Button variant="secondary" size="sm" onclick={refreshAll}>
          <RefreshCw size={14} /> Refresh
        </Button>
      {:else}
        <strong>No campaigns match these filters</strong>
        <p>Try switching platform, status, or clearing your search query.</p>
        <Button
          variant="secondary"
          size="sm"
          onclick={() => {
            platformFilter = 'all';
            filter = 'all';
            selectedGame = 'all';
            search = '';
          }}
        >
          Reset filters
        </Button>
      {/if}
    </div>
  {:else}
    <div class="drops-grid" class:compact-list={$dropsViewMode === 'compact'}>
      {#each filteredCampaigns as camp (camp.platform + ':' + camp.id)}
        <CampaignCard
          campaign={camp}
          showPlatformBadge={true}
          compact={$dropsViewMode === 'compact'}
          isFarmSelected={isCampaignFarmed(camp, settings)}
          isExcluded={settings[camp.platform].excludedCampaigns.includes(camp.id)}
          isGamePrioritized={isGamePrioritizedFor(camp, settings)}
          onToggleFarm={(farm) => handleToggleFarm(camp, farm)}
          onToggleGameFarm={(farm) => handleToggleGameFarm(camp, farm)}
          onExclude={() => handleToggleFarm(camp, false)}
          onOpenUrl={handleOpenUrl}
          onOpenInventory={() => handleOpenInventory(camp.platform)}
          onFetchDetails={() => handleFetchDetails(camp)}
        />
      {/each}
    </div>
  {/if}
</div>

<style>
  .watchlist-bar {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 14px;
    background: linear-gradient(135deg, rgba(129, 140, 248, 0.10), rgba(168, 85, 247, 0.06) 55%, transparent);
  }

  .wl-top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .wl-title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 7px;
    color: var(--accent-text);
    font-size: var(--text-sm);
  }

  .wl-title strong {
    color: var(--text);
  }

  .wl-sub {
    color: var(--text-muted);
    font-size: var(--text-xs);
  }

  .wl-form {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    flex: 1;
    justify-content: flex-end;
    min-width: 280px;
  }

  .wl-picker-wrap {
    flex: 1;
    min-width: 240px;
    max-width: 400px;
  }

  .wl-scope {
    display: inline-flex;
    align-items: center;
    padding: 2px;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .scope-pill {
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: var(--radius-pill);
  }

  .scope-pill.active {
    background: var(--accent-tint);
    color: var(--accent-text);
  }

  .wl-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .wl-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 12px;
  }

  .wl-chip-live {
    border-color: rgba(52, 211, 153, 0.38);
    background: rgba(52, 211, 153, 0.08);
  }

  .wl-dots {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .wl-chip-name {
    font-weight: 600;
    color: var(--text);
  }

  .wl-chip-status {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-faint);
  }

  .wl-chip-status.status-live {
    color: var(--success-text);
  }

  .wl-chip-rm {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: none;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--text-faint);
  }

  .wl-chip-rm:hover {
    background: var(--surface-hover);
    color: var(--text);
  }

  .drops-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }

  @media (max-width: 780px) {
    .drops-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 520px) {
    .drops-grid {
      grid-template-columns: 1fr;
    }
  }

  .drops-grid.compact-list {
    grid-template-columns: 1fr;
    gap: 6px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--space-2);
    padding: var(--space-8) var(--space-4);
    color: var(--text-faint);
  }

  .empty-state strong {
    color: var(--text);
    font-size: var(--text-base);
  }

  .empty-state p {
    color: var(--text-muted);
    font-size: var(--text-sm);
    max-width: 420px;
    margin-bottom: var(--space-2);
  }
</style>
