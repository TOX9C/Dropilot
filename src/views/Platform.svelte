<script lang="ts">
  import AlertTriangle from 'lucide-svelte/icons/alert-triangle';
  import LogIn from 'lucide-svelte/icons/log-in';
  import Pause from 'lucide-svelte/icons/pause';
  import Pin from 'lucide-svelte/icons/pin';
  import Play from 'lucide-svelte/icons/play';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import AccountChip from '../components/AccountChip.svelte';
  import Button from '../components/Button.svelte';
  import CampaignCard from '../components/CampaignCard.svelte';
  import Countdown from '../components/Countdown.svelte';
  import FilterBar from '../components/FilterBar.svelte';
  import Monogram from '../components/Monogram.svelte';
  import NowWatchingCard from '../components/NowWatchingCard.svelte';
  import PriorityList from '../components/PriorityList.svelte';
  import SectionHeader from '../components/SectionHeader.svelte';
  import StatusPill from '../components/StatusPill.svelte';
  import { api } from '../lib/api';
  import { nowMs } from '../lib/clock';
  import { formatRelativeTime, formatViewerCount } from '../lib/format';
  import {
    computeCampaignCounts,
    extractGameOptions,
    filterAndSortCampaigns,
    isCampaignFarmed,
    isGamePrioritizedFor,
  } from '../lib/selectors';
  import {
    appSettings,
    dropsViewMode,
    lastSnapshotAt,
    notifyUi,
    toggleCampaignFarmAction,
    toggleGameFarmAction,
    updateSnapshotCampaign,
  } from '../lib/stores';
  import type {
    Campaign,
    CampaignFilter,
    CampaignSort,
    PlatformId,
    PlatformSnapshot,
    PriorityItem,
    Settings,
  } from '../lib/types';

  interface Props {
    platform: PlatformId;
    snapshot: PlatformSnapshot;
    settings: Settings;
  }

  let { platform, snapshot, settings }: Props = $props();

  let filter = $state<CampaignFilter>('all');
  let sort = $state<CampaignSort>('ending_soon');
  let search = $state('');
  let selectedGame = $state('all');

  $effect(() => {
    void platform;
    filter = 'all';
    selectedGame = 'all';
    search = '';
  });

  const platSettings = $derived(settings[platform]);
  const platName = $derived(platform === 'twitch' ? 'Twitch' : 'Kick');

  const currentCampaign = $derived(
    snapshot.currentCampaignId
      ? snapshot.campaigns.find((c) => c.id === snapshot.currentCampaignId) ?? null
      : null,
  );

  const currentDrop = $derived.by(() => {
    if (!currentCampaign) return null;
    if (snapshot.currentDropId) {
      const exact = currentCampaign.drops.find((d) => d.id === snapshot.currentDropId);
      if (exact) return exact;
    }
    return currentCampaign.drops.find((d) => !d.claimed) ?? null;
  });

  const hasManualClaim = $derived(
    snapshot.campaigns.some((c) => c.drops.some((d) => d.needsManualClaim)),
  );

  const counts = $derived(computeCampaignCounts(snapshot.campaigns, settings, $nowMs));
  const games = $derived(extractGameOptions(snapshot.campaigns));
  const filteredCampaigns = $derived(
    filterAndSortCampaigns(snapshot.campaigns, settings, filter, sort, search, selectedGame, $nowMs),
  );

  async function handleLogin() {
    try { await api.login(platform); } catch (err) { notifyUi(String(err), 'error'); }
  }

  async function handleLogout() {
    try { await api.logout(platform); } catch (err) { notifyUi(String(err), 'error'); }
  }

  async function handlePauseResume() {
    try {
      if (snapshot.state === 'Paused') await api.resume(platform);
      else await api.pause(platform);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleSwitchChannel() {
    try { await api.switchChannel(platform); } catch (err) { notifyUi(String(err), 'error'); }
  }

  async function handlePinChannel(login: string | null) {
    try {
      const updated = await api.pinChannel(platform, login);
      appSettings.set(updated);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleToggleFarm(camp: Campaign, farm: boolean) {
    try {
      await toggleCampaignFarmAction(camp, farm, Boolean(snapshot.account));
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleToggleGameFarm(camp: Campaign, farm: boolean) {
    try {
      await toggleGameFarmAction(camp, farm, Boolean(snapshot.account));
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleReorderPriority(items: PriorityItem[]) {
    try {
      const updated = await api.setPriority(platform, items);
      appSettings.set(updated);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleOpenUrl(url: string) {
    try { await api.openUrl(url); } catch (err) { notifyUi(String(err), 'error'); }
  }

  function handleOpenChannel(login: string) {
    const url = platform === 'twitch' ? `https://www.twitch.tv/${login}` : `https://kick.com/${login}`;
    void handleOpenUrl(url);
  }

  async function handleOpenInventory() {
    try { await api.openDropsInventory(platform); } catch (err) { notifyUi(String(err), 'error'); }
  }

  async function handleRetry() {
    try { await api.retryPlatform(platform); } catch (err) { notifyUi(String(err), 'error'); }
  }

  async function handleAddWatchedGame(gameName: string) {
    const clean = gameName.trim();
    if (!clean) return;
    try {
      const match = snapshot.campaigns.find(
        (c) => c.game.name.trim().toLowerCase() === clean.toLowerCase(),
      );
      const updated = await api.setGameFarm(
        platform,
        match?.game.id ?? '',
        match?.game.name ?? clean,
        true,
      );
      appSettings.set(updated);
      notifyUi(
        snapshot.account
          ? `Watching "${match?.game.name ?? clean}" on ${platName} — will auto-mine whenever a drop goes live.`
          : `Added "${clean}"! Log in to ${platName} to auto-farm when drops go live.`,
      );
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleFetchDetails(camp: Campaign) {
    const detailed = await api.fetchCampaignDetails(platform, camp.id);
    updateSnapshotCampaign(detailed);
  }
</script>

<div class="page">
  <header class="page-header">
    <div class="page-title-wrap">
      <div class="page-icon page-icon-{platform}">
        <span class="plat-dot dot-{platform}" aria-hidden="true"></span>
      </div>
      <div>
        <div class="title-inline">
          <h1>{platName}</h1>
          <StatusPill state={snapshot.state} />
        </div>
        <p class="page-subtitle">Manage {platName} campaigns, priority order, and active stream</p>
      </div>
    </div>

    <div class="page-actions">
      {#if snapshot.account}
        <Button variant="secondary" size="sm" onclick={handleRetry}>
          <RefreshCw size={14} />
          <span>Refresh</span>
        </Button>
        <Button variant="secondary" size="sm" onclick={handleSwitchChannel} disabled={snapshot.state === 'Paused'}>
          <RefreshCw size={14} />
          <span>Switch</span>
        </Button>
        <Button variant="secondary" size="sm" onclick={handlePauseResume}>
          {#if snapshot.state === 'Paused'}<Play size={14} /><span>Resume</span>{:else}<Pause size={14} /><span>Pause</span>{/if}
        </Button>
      {/if}

      <AccountChip
        {platform}
        account={snapshot.account}
        onlogin={handleLogin}
        onlogout={handleLogout}
      />
    </div>
  </header>

  {#if hasManualClaim}
    <div class="banner-warn" role="status">
      <AlertTriangle size={16} />
      <div class="banner-text">
        <strong>Claim needs you</strong>
        <span>{platName} requested a manual verification step to claim a completed drop.</span>
      </div>
      <Button variant="primary" size="sm" onclick={handleOpenInventory}>
        <span>Open drops inventory ↗</span>
      </Button>
    </div>
  {/if}

  {#if snapshot.state === 'LoggedOut' && snapshot.campaigns.length === 0}
    <section class="card logged-out-card stripe-{platform}">
      <div class="lo-icon"><LogIn size={24} /></div>
      <h2>Sign in to {platName}</h2>
      <p>
        Open {platName}'s official site inside DropPilot's isolated window to load active campaigns and track drops.
      </p>
      <Button variant={platform} size="md" onclick={handleLogin}>
        <LogIn size={15} />
        <span>Log in to {platName}</span>
      </Button>
    </section>
  {:else}
    {#if snapshot.state === 'LoggedOut'}
      <div class="card public-bar stripe-{platform} wash-{platform}">
        <div class="banner-text">
          <strong>Browsing public {platName} drops catalog</strong>
          <span>Sign in with your {platName} account to farm drops and sync progress automatically.</span>
        </div>
        <Button variant={platform} size="sm" onclick={handleLogin}>
          <LogIn size={14} />
          <span>Log in to {platName}</span>
        </Button>
      </div>
    {:else}
      <NowWatchingCard
        {platform}
        state={snapshot.state}
        stateReason={snapshot.stateReason}
        lastError={snapshot.lastError}
        nextActionAt={snapshot.nextActionAt}
        channel={snapshot.watching}
        campaign={currentCampaign}
        drop={currentDrop}
        candidates={snapshot.candidateChannels}
        pinnedChannel={platSettings.pinnedChannel}
        onPauseResume={handlePauseResume}
        onSwitchChannel={handleSwitchChannel}
        onPinChannel={handlePinChannel}
        onOpenChannel={handleOpenChannel}
        onOpenInventory={handleOpenInventory}
        onRetry={handleRetry}
      />
    {/if}

    <div class="workspace-split">
      <div class="campaigns-col">
        <FilterBar
          {filter}
          {sort}
          {search}
          {counts}
          {games}
          {selectedGame}
          viewMode={$dropsViewMode}
          onfilter={(next) => (filter = next)}
          onsort={(next) => (sort = next)}
          onsearch={(next) => (search = next)}
          ongame={(next) => (selectedGame = next)}
          onviewmode={(next) => dropsViewMode.set(next)}
        />

        {#if filteredCampaigns.length === 0}
          <div class="card empty-campaigns">
            {#if snapshot.campaigns.length === 0}
              <p class="empty-msg">
                No campaigns right now. Last checked {formatRelativeTime($lastSnapshotAt, $nowMs)}.
                {#if snapshot.nextActionAt}
                  Next check in <Countdown targetIso={snapshot.nextActionAt} />.
                {/if}
              </p>
              <Button variant="secondary" size="sm" onclick={handleRetry}>
                <RefreshCw size={14} /> Refresh now
              </Button>
            {:else}
              <p class="empty-msg">No campaigns match your current filter or search query.</p>
              <Button variant="secondary" size="sm" onclick={() => { filter = 'all'; selectedGame = 'all'; search = ''; }}>
                Show all campaigns
              </Button>
            {/if}
          </div>
        {:else}
          <div class="campaigns-grid" class:compact-list={$dropsViewMode === 'compact'}>
            {#each filteredCampaigns as camp (camp.id)}
              <CampaignCard
                campaign={camp}
                showPlatformBadge={false}
                compact={$dropsViewMode === 'compact'}
                isFarmSelected={isCampaignFarmed(camp, settings)}
                isExcluded={platSettings.excludedCampaigns.includes(camp.id)}
                isGamePrioritized={isGamePrioritizedFor(camp, settings)}
                onToggleFarm={(farm) => handleToggleFarm(camp, farm)}
                onToggleGameFarm={(farm) => handleToggleGameFarm(camp, farm)}
                onExclude={() => handleToggleFarm(camp, false)}
                onOpenUrl={handleOpenUrl}
                onOpenInventory={handleOpenInventory}
                onFetchDetails={() => handleFetchDetails(camp)}
              />
            {/each}
          </div>
        {/if}
      </div>

      <div class="rail-col">
        <PriorityList
          items={platSettings.priority}
          campaigns={snapshot.campaigns}
          {platform}
          onReorder={handleReorderPriority}
          onAddGame={handleAddWatchedGame}
        />

        <section class="card candidates-card">
          <SectionHeader title="Candidate channels" count={snapshot.candidateChannels.length} uppercase />

          {#if snapshot.candidateChannels.length === 0}
            <p class="cand-empty">Live candidate streams for the active campaign will appear here.</p>
          {:else}
            <div class="cand-list">
              {#each snapshot.candidateChannels.slice(0, 8) as ch (ch.id + ch.login)}
                {@const pinned = platSettings.pinnedChannel?.toLowerCase() === ch.login.toLowerCase()}
                <div class="cand-row">
                  <Monogram src={ch.avatarUrl} name={ch.displayName} width={28} height={28} rounded />
                  <div class="cand-meta">
                    <span class="cand-name">{ch.displayName}</span>
                    <span class="cand-sub tabular">
                      {formatViewerCount(ch.viewerCount)} viewers{#if ch.dropsEnabled} · Drops{/if}
                    </span>
                  </div>
                  <button
                    type="button"
                    class="pin-btn"
                    class:pinned={pinned}
                    aria-label={pinned ? `Unpin ${ch.displayName}` : `Pin ${ch.displayName}`}
                    data-tip={pinned ? 'Unpin' : 'Pin channel'}
                    onclick={() => handlePinChannel(pinned ? null : ch.login)}
                  >
                    <Pin size={13} />
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </section>
      </div>
    </div>
  {/if}
</div>

<style>
  .title-inline {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .banner-warn {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    background: var(--warning-tint);
    border: 1px solid rgba(245, 181, 68, 0.35);
    color: var(--warning-text);
  }

  .banner-text {
    flex: 1;
    min-width: 200px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .logged-out-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--space-3);
    padding: var(--space-8) var(--space-6);
  }

  .lo-icon {
    color: var(--text-faint);
  }

  .public-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .public-bar strong {
    font-size: var(--text-base);
  }

  .public-bar span {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .logged-out-card p {
    color: var(--text-muted);
    font-size: var(--text-sm);
    max-width: 420px;
  }

  .workspace-split {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 240px;
    gap: 12px;
    align-items: start;
  }

  .campaigns-col,
  .rail-col {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .campaigns-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  @media (max-width: 660px) {
    .campaigns-grid {
      grid-template-columns: 1fr;
    }
  }

  .campaigns-grid.compact-list {
    grid-template-columns: 1fr;
    gap: 6px;
  }

  .empty-campaigns {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--space-3);
    padding: var(--space-8) var(--space-4);
    color: var(--text-faint);
  }

  .empty-msg {
    color: var(--text-muted);
    font-size: var(--text-sm);
    max-width: 420px;
  }

  .candidates-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .cand-empty {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .cand-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .cand-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 5px 8px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }

  .cand-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .cand-name {
    font-size: var(--text-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cand-sub {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .pin-btn {
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-muted);
    width: 26px;
    height: 26px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .pin-btn:hover,
  .pin-btn.pinned {
    color: var(--accent-text);
    border-color: var(--accent);
  }

  @media (max-width: 899px) {
    .workspace-split {
      grid-template-columns: 1fr;
    }
    .rail-col {
      order: -1;
    }
  }
</style>
