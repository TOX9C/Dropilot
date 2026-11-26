<script lang="ts">
  import AlertTriangle from 'lucide-svelte/icons/alert-triangle';
  import Clock from 'lucide-svelte/icons/clock';
  import Gift from 'lucide-svelte/icons/gift';
  import LoaderCircle from 'lucide-svelte/icons/loader-circle';
  import LogIn from 'lucide-svelte/icons/log-in';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Search from 'lucide-svelte/icons/search';
  import Star from 'lucide-svelte/icons/star';
  import X from 'lucide-svelte/icons/x';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import FilterBar from '../components/FilterBar.svelte';
  import GameDropGrid from '../components/GameDropGrid.svelte';
  import GamePicker from '../components/GamePicker.svelte';
  import { api } from '../lib/api';
  import { slowNowMs } from '../lib/clock';
  import { formatRelativeTime } from '../lib/format';
  import {
    computeCampaignCounts,
    countSearchMatchesIgnoringFilters,
    extractGameOptions,
    filterAndSortCampaigns,
    groupCampaignsByGame,
    isCampaignActiveNow,
    isCampaignCompleted,
    isCampaignFarmed,
    priorityItemMatchesGame,
    type GameDropGroup,
  } from '../lib/selectors';
  import {
    addWatchedGameAction,
    appSettings,
    editionInfo,
    lastSnapshotAt,
    notifyUi,
    toggleCampaignFarmAction,
    toggleGameGroupFarmAction,
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
  let refreshing = $state(false);

  const allCampaigns = $derived(
    $editionInfo.kickEnabled
      ? [...snapshot.twitch.campaigns, ...snapshot.kick.campaigns]
      : [...snapshot.twitch.campaigns],
  );

  interface WatchedGameChip {
    key: string;
    name: string;
    twitchId: string | null;
    kickId: string | null;
    hasLiveDrop: boolean;
  }

  const watchedGames = $derived.by<WatchedGameChip[]>(() => {
    const map = new Map<string, WatchedGameChip>();
    const activePlats: readonly PlatformId[] = $editionInfo.kickEnabled
      ? ['twitch', 'kick']
      : ['twitch'];
    for (const plat of activePlats) {
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
  const gameGroups = $derived(
    groupCampaignsByGame(filteredCampaigns, settings, $slowNowMs),
  );

  const filteredActiveCount = $derived(
    filteredCampaigns.filter((c) => isCampaignActiveNow(c, $slowNowMs)).length,
  );
  const filteredUpcomingCount = $derived(
    filteredCampaigns.filter((c) => c.status === 'upcoming').length,
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
    $editionInfo.kickEnabled && !snapshot.kick.account && snapshot.kick.campaigns.length === 0,
  );

  const scopedPlatforms = $derived<PlatformId[]>(
    !$editionInfo.kickEnabled
      ? ['twitch']
      : platformFilter === 'all'
        ? ['twitch', 'kick']
        : [platformFilter],
  );
  const scopedErrors = $derived(
    scopedPlatforms
      .map((p) => snapshot[p])
      .filter((s) => s.state === 'Error' && Boolean(s.lastError)),
  );
  const scopedSyncing = $derived(
    refreshing ||
      scopedPlatforms.some((p) => snapshot[p].state === 'Selecting' && snapshot[p].campaigns.length === 0),
  );

  const trimmedSearch = $derived(search.trim());
  const searchMatchesAll = $derived(
    trimmedSearch ? countSearchMatchesIgnoringFilters(allCampaigns, trimmedSearch) : 0,
  );
  const searchMatchesInScope = $derived(
    trimmedSearch ? countSearchMatchesIgnoringFilters(scopedCampaigns, trimmedSearch) : 0,
  );

  $effect(() => {
    if (selectedGame !== 'all' && !games.some((g) => g.name === selectedGame)) {
      selectedGame = 'all';
    }
  });

  async function refreshAll() {
    refreshing = true;
    try {
      if ($editionInfo.kickEnabled) {
        await Promise.all([api.retryPlatform('twitch'), api.retryPlatform('kick')]);
        notifyUi('Refreshed Twitch and Kick drops');
      } else {
        await api.retryPlatform('twitch');
        notifyUi('Refreshed Twitch drops');
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      refreshing = false;
    }
  }

  async function handleLogin(platform: PlatformId) {
    try {
      await api.login(platform);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleToggleCampaignFarm(camp: Campaign, farm: boolean) {
    try {
      await toggleCampaignFarmAction(camp, farm, Boolean(snapshot[camp.platform].account));
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleToggleGameFarm(group: GameDropGroup, farm: boolean) {
    try {
      await toggleGameGroupFarmAction(
        group.campaigns,
        farm,
        Boolean(snapshot[group.platform].account),
        settings,
      );
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

  function clearFilters() {
    platformFilter = 'all';
    filter = 'all';
    selectedGame = 'all';
    search = '';
  }
</script>

<div class="page">
  <header class="page-header">
    <div class="page-title-wrap">
      <h1>All Drops</h1>
    </div>

    <div class="page-actions">
      <Badge tone="neutral">{counts.active} active</Badge>
      <Badge tone="accent">{farmingCount} farming</Badge>
      {#if endingUnder24h > 0}
        <Badge tone="warning">
          <Clock size={11} />
          <span>{endingUnder24h} ending &lt;24h</span>
        </Badge>
      {/if}
      {#if needTwitchNotice}
        <Button
          variant="secondary"
          size="sm"
          title="Opens Dropilot's isolated sign-in window (required for Drops sync; saved once in Credential Manager)"
          onclick={() => handleLogin('twitch')}
        >
          <span class="plat-dot dot-twitch" aria-hidden="true"></span>
          <LogIn size={13} />
          <span>Connect Twitch</span>
        </Button>
      {/if}
      {#if needKickNotice}
        <Button
          variant="secondary"
          size="sm"
          title="Opens Dropilot's isolated sign-in window (required for Drops sync; saved once in Credential Manager)"
          onclick={() => handleLogin('kick')}
        >
          <span class="plat-dot dot-kick" aria-hidden="true"></span>
          <LogIn size={13} />
          <span>Connect Kick</span>
        </Button>
      {/if}
      <Button variant="secondary" size="sm" disabled={refreshing} onclick={refreshAll}>
        <RefreshCw size={13} />
        <span>{refreshing ? 'Refreshing...' : 'Refresh'}</span>
      </Button>
    </div>
  </header>

  <section class="card watchlist-bar" aria-label="Auto-mine game watchlist">
    <div class="wl-top">
      <div class="wl-title">
        <Star size={14} />
        <strong>Game Watchlist</strong>
      </div>

      <div class="wl-form">
        <div class="wl-picker-wrap">
          <GamePicker
            campaigns={allCampaigns}
            watchedNames={watchedGames.map((w) => w.name)}
            placeholder="Search or add any game (e.g. Rust, Counter-Strike 2)..."
            disabled={addingWatch}
            buttonLabel="Watch"
            onSelect={handleSelectWatchedGame}
          />
        </div>

        {#if $editionInfo.kickEnabled}
          <div class="wl-scope" role="group" aria-label="Watchlist platform">
            <button type="button" class="scope-pill" class:active={watchScope === 'both'} onclick={() => (watchScope = 'both')}>Both</button>
            <button type="button" class="scope-pill" class:active={watchScope === 'twitch'} onclick={() => (watchScope = 'twitch')}>Twitch</button>
            <button type="button" class="scope-pill" class:active={watchScope === 'kick'} onclick={() => (watchScope = 'kick')}>Kick</button>
          </div>
        {/if}
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
              {chip.hasLiveDrop ? 'Live' : 'Waiting'}
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
    platformCounts={$editionInfo.kickEnabled ? platformCounts : undefined}
    {games}
    {selectedGame}
    onplatform={$editionInfo.kickEnabled ? (next) => (platformFilter = next) : undefined}
    onfilter={(next) => (filter = next)}
    onsort={(next) => (sort = next)}
    onsearch={(next) => (search = next)}
    ongame={(next) => (selectedGame = next)}
  />

  <div class="discovery-bar tabular" role="status">
    <div class="disc-left">
      <span>
        Showing <strong>{gameGroups.length}</strong> {gameGroups.length === 1 ? 'game' : 'games'}
        ({filteredCampaigns.length} {filteredCampaigns.length === 1 ? 'campaign' : 'campaigns'}:
        {filteredActiveCount} active{filteredUpcomingCount > 0 ? `, ${filteredUpcomingCount} upcoming` : ''})
      </span>
      <span class="disc-sep" aria-hidden="true">·</span>
      <span class="disc-sync">Synced {formatRelativeTime($lastSnapshotAt, $slowNowMs)}</span>
    </div>

    <div class="disc-right">
      {#if platformFilter !== 'kick'}
        <span class="disc-plat">
          <span class="plat-dot dot-twitch" aria-hidden="true"></span>
          <span>
            Twitch:
            {#if snapshot.twitch.state === 'Error'}
              <span class="disc-err">Error</span>
            {:else if !snapshot.twitch.account}
              Public catalog
            {:else}
              @{snapshot.twitch.account.login}
            {/if}
          </span>
        </span>
      {/if}
      {#if $editionInfo.kickEnabled && platformFilter !== 'twitch'}
        <span class="disc-plat">
          <span class="plat-dot dot-kick" aria-hidden="true"></span>
          <span>
            Kick:
            {#if snapshot.kick.state === 'Error'}
              <span class="disc-err">Error</span>
            {:else if !snapshot.kick.account}
              Public catalog
            {:else}
              @{snapshot.kick.account.login}
            {/if}
          </span>
        </span>
      {/if}
    </div>
  </div>

  {#if gameGroups.length === 0}
    <div class="designed-empty">
      {#if scopedCampaigns.length === 0 && scopedSyncing}
        <span class="empty-ico spin-ico" aria-hidden="true"><LoaderCircle size={18} /></span>
        <strong>Syncing drop campaigns...</strong>
        <p>Checking live and upcoming {$editionInfo.kickEnabled ? 'Twitch & Kick' : 'Twitch'} drop campaigns right now.</p>
      {:else if scopedCampaigns.length === 0 && scopedErrors.length > 0}
        <span class="empty-ico err-ico" aria-hidden="true"><AlertTriangle size={18} /></span>
        <strong>Could not load {platformFilter === 'all' ? 'platform' : platformFilter === 'twitch' ? 'Twitch' : 'Kick'} campaigns</strong>
        <p>{scopedErrors.map((e) => `${e.platform === 'twitch' ? 'Twitch' : 'Kick'}: ${e.lastError}`).join(' · ')}</p>
        <Button variant="primary" size="sm" onclick={refreshAll}>
          <RefreshCw size={13} /> Retry sync
        </Button>
      {:else if trimmedSearch}
        <span class="empty-ico" aria-hidden="true"><Search size={18} /></span>
        {#if searchMatchesAll > 0}
          <strong>No "{trimmedSearch}" campaigns match active filters</strong>
          <p>
            {#if searchMatchesInScope === 0 && platformFilter !== 'all'}
              {searchMatchesAll} matching {searchMatchesAll === 1 ? 'campaign exists' : 'campaigns exist'} on the other platform.
            {:else}
              {searchMatchesInScope || searchMatchesAll} matching {searchMatchesAll === 1 ? 'campaign is' : 'campaigns are'} hidden by the current "{filter !== 'all' ? filter : selectedGame}" filter.
            {/if}
          </p>
          <div class="empty-actions">
            <Button variant="primary" size="sm" onclick={clearFilters}>
              Clear filters
            </Button>
          </div>
        {:else}
          <strong>No active or upcoming drop campaigns for "{trimmedSearch}" right now.</strong>
          <p>
            Checked the live {platformFilter === 'all' ? 'Twitch & Kick' : platformFilter === 'twitch' ? 'Twitch' : 'Kick'} catalog — add "{trimmedSearch}" to your Watchlist to auto-farm the moment a campaign launches.
          </p>
          <div class="empty-actions">
            <Button
              variant="primary"
              size="sm"
              disabled={addingWatch}
              onclick={() => handleSelectWatchedGame(trimmedSearch)}
            >
              <Star size={13} />
              <span>Watch "{trimmedSearch}" for future drops</span>
            </Button>
            <Button variant="secondary" size="sm" onclick={() => (search = '')}>
              Clear search
            </Button>
          </div>
        {/if}
      {:else if scopedCampaigns.length === 0}
        <span class="empty-ico" aria-hidden="true"><Gift size={18} /></span>
        <strong>No active or upcoming campaigns on {platformFilter === 'all' ? 'Twitch or Kick' : platformFilter === 'twitch' ? 'Twitch' : 'Kick'} right now</strong>
        <p>
          Last checked {formatRelativeTime($lastSnapshotAt, $slowNowMs)}. Sign in above or refresh to check again.
        </p>
        <Button variant="secondary" size="sm" onclick={refreshAll}>
          <RefreshCw size={13} /> Refresh now
        </Button>
      {:else}
        <span class="empty-ico" aria-hidden="true"><Gift size={18} /></span>
        <strong>No campaigns match these filters</strong>
        <p>Try switching platform, status, or clearing your filters.</p>
        <Button variant="secondary" size="sm" onclick={clearFilters}>
          Clear filters
        </Button>
      {/if}
    </div>
  {:else}
    <GameDropGrid
      groups={gameGroups}
      {settings}
      showPlatformBadge={true}
      onToggleGameFarm={handleToggleGameFarm}
      onToggleCampaignFarm={handleToggleCampaignFarm}
      onExcludeCampaign={(camp) => handleToggleCampaignFarm(camp, false)}
      onOpenUrl={handleOpenUrl}
      onOpenInventory={handleOpenInventory}
      onFetchDetails={handleFetchDetails}
    />
  {/if}
</div>

<style>
  .watchlist-bar {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 11px 14px;
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
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  .wl-title strong {
    color: var(--text);
  }

  .wl-form {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    flex: 1;
    justify-content: flex-end;
    min-width: 260px;
  }

  .wl-picker-wrap {
    flex: 1;
    min-width: 220px;
    max-width: 520px;
  }

  .wl-scope {
    display: inline-flex;
    align-items: center;
    padding: 2px;
    border-radius: var(--radius-sm);
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
    border-radius: var(--radius-xs);
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
    border-color: var(--border-hover);
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
    font-size: 10.5px;
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

  .discovery-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 14px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-muted);
  }

  .disc-left,
  .disc-right {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .disc-left strong {
    color: var(--text);
  }

  .disc-sep {
    color: var(--border-hover);
  }

  .disc-sync {
    color: var(--text-faint);
  }

  .disc-plat {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .disc-err {
    color: var(--error-text);
    font-weight: 600;
  }

  .empty-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin-top: 2px;
  }

  .spin-ico {
    animation: dp-spin 1s linear infinite;
  }

  .err-ico {
    color: var(--error-text);
    border-color: var(--error);
    background: var(--error-tint);
  }

  @keyframes dp-spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
