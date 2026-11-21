<script lang="ts">
  import Clock from 'lucide-svelte/icons/clock';
  import Gift from 'lucide-svelte/icons/gift';
  import LogIn from 'lucide-svelte/icons/log-in';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import CampaignCard from '../components/CampaignCard.svelte';
  import FilterBar from '../components/FilterBar.svelte';
  import { api } from '../lib/api';
  import { nowMs } from '../lib/clock';
  import {
    computeCampaignCounts,
    extractGameOptions,
    filterAndSortCampaigns,
    isCampaignCompleted,
    isCampaignFarmed,
    isGamePrioritizedFor,
  } from '../lib/selectors';
  import {
    dropsViewMode,
    notifyUi,
    toggleCampaignFarmAction,
    toggleGameFarmAction,
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

  const allCampaigns = $derived([...snapshot.twitch.campaigns, ...snapshot.kick.campaigns]);

  const platformCounts = $derived<Record<PlatformFilter, number>>({
    all: allCampaigns.length,
    twitch: snapshot.twitch.campaigns.length,
    kick: snapshot.kick.campaigns.length,
  });

  const scopedCampaigns = $derived(
    platformFilter === 'all' ? allCampaigns : snapshot[platformFilter].campaigns,
  );

  const counts = $derived(computeCampaignCounts(scopedCampaigns, settings));
  const games = $derived(extractGameOptions(scopedCampaigns));
  const filteredCampaigns = $derived(
    filterAndSortCampaigns(scopedCampaigns, settings, filter, sort, search, selectedGame),
  );

  const endingUnder24h = $derived(
    allCampaigns.filter((c) => {
      if (c.status !== 'active' || isCampaignCompleted(c)) return false;
      const diff = new Date(c.endsAt).getTime() - $nowMs;
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
        />
      {/each}
    </div>
  {/if}
</div>

<style>
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
