<script lang="ts">
  import AlertTriangle from 'lucide-svelte/icons/alert-triangle';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import Gift from 'lucide-svelte/icons/gift';
  import Info from 'lucide-svelte/icons/info';
  import LoaderCircle from 'lucide-svelte/icons/loader-circle';
  import LogIn from 'lucide-svelte/icons/log-in';
  import Pause from 'lucide-svelte/icons/pause';
  import Pin from 'lucide-svelte/icons/pin';
  import Play from 'lucide-svelte/icons/play';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Search from 'lucide-svelte/icons/search';
  import Star from 'lucide-svelte/icons/star';
  import AccountChip from '../components/AccountChip.svelte';
  import Button from '../components/Button.svelte';
  import Countdown from '../components/Countdown.svelte';
  import FilterBar from '../components/FilterBar.svelte';
  import GameDropGrid from '../components/GameDropGrid.svelte';
  import Monogram from '../components/Monogram.svelte';
  import NowWatchingCard from '../components/NowWatchingCard.svelte';
  import PriorityList from '../components/PriorityList.svelte';
  import SectionHeader from '../components/SectionHeader.svelte';
  import StatusPill from '../components/StatusPill.svelte';
  import { api } from '../lib/api';
  import { slowNowMs } from '../lib/clock';
  import { formatRelativeTime, formatViewerCount } from '../lib/format';
  import {
    computeCampaignCounts,
    countSearchMatchesIgnoringFilters,
    extractGameOptions,
    filterAndSortCampaigns,
    groupCampaignsByGame,
    isCampaignActiveNow,
    type GameDropGroup,
  } from '../lib/selectors';
  import {
    appSettings,
    lastSnapshotAt,
    notifyUi,
    toggleCampaignFarmAction,
    toggleGameGroupFarmAction,
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
  let refreshing = $state(false);
  let addingWatch = $state(false);

  $effect(() => {
    void platform;
    filter = 'all';
    selectedGame = 'all';
    search = '';
  });

  const platSettings = $derived(settings[platform]);
  const platName = $derived(platform === 'twitch' ? 'Twitch' : 'Kick');
  const browserDropsUrl = $derived(
    platform === 'twitch'
      ? 'https://www.twitch.tv/drops/campaigns'
      : 'https://kick.com/drops/campaigns',
  );
  const activePinnedChannel = $derived(snapshot.pinnedChannel ?? null);
  const participatingChannels = $derived(
    snapshot.candidateChannels.filter((c) => c.isLive && c.dropsEnabled),
  );

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

  const counts = $derived(computeCampaignCounts(snapshot.campaigns, settings, $slowNowMs));
  const games = $derived(extractGameOptions(snapshot.campaigns));
  const filteredCampaigns = $derived(
    filterAndSortCampaigns(snapshot.campaigns, settings, filter, sort, search, selectedGame, $slowNowMs),
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
  const trimmedSearch = $derived(search.trim());
  const searchMatchesInCatalog = $derived(
    trimmedSearch ? countSearchMatchesIgnoringFilters(snapshot.campaigns, trimmedSearch) : 0,
  );

  async function handleLogin() {
    try { await api.login(platform); } catch (err) { notifyUi(String(err), 'error'); }
  }

  async function handleLogout() {
    try { await api.logout(platform); } catch (err) { notifyUi(String(err), 'error'); }
  }

  async function handleOpenBrowserDrops() {
    try {
      await api.openUrl(browserDropsUrl);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
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

  async function handleToggleCampaignFarm(camp: Campaign, farm: boolean) {
    try {
      await toggleCampaignFarmAction(camp, farm, Boolean(snapshot.account));
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleToggleGameFarm(group: GameDropGroup, farm: boolean) {
    try {
      await toggleGameGroupFarmAction(
        group.campaigns,
        farm,
        Boolean(snapshot.account),
        settings,
      );
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

  async function handleToggleEndingSoon(enabled: boolean) {
    try {
      const updated = await api.setPrioritizeEndingSoon(platform, enabled);
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
    refreshing = true;
    try {
      await api.retryPlatform(platform);
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      refreshing = false;
    }
  }

  async function handleAddWatchedGame(gameName: string) {
    const clean = gameName.trim();
    if (!clean || addingWatch) return;
    addingWatch = true;
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
          ? `Watching "${match?.game.name ?? clean}" on ${platName}`
          : `Added "${clean}" — log in to ${platName} to farm when drops go live.`,
      );
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      addingWatch = false;
    }
  }

  async function handleFetchDetails(camp: Campaign) {
    const detailed = await api.fetchCampaignDetails(platform, camp.id);
    updateSnapshotCampaign(detailed);
  }

  function clearFilters() {
    filter = 'all';
    selectedGame = 'all';
    search = '';
  }
</script>

<div class="page">
  <header class="page-header">
    <div class="page-title-wrap">
      <div>
        <div class="title-inline">
          <span class="plat-dot dot-{platform}" aria-hidden="true"></span>
          <h1>{platName}</h1>
          <StatusPill state={snapshot.state} />
        </div>
      </div>
    </div>

    <div class="page-actions">
      {#if snapshot.account}
        <Button variant="secondary" size="sm" disabled={refreshing} onclick={handleRetry}>
          <RefreshCw size={13} />
          <span>{refreshing ? 'Refreshing...' : 'Refresh'}</span>
        </Button>
        <Button variant="secondary" size="sm" onclick={handleSwitchChannel} disabled={snapshot.state === 'Paused'}>
          <RefreshCw size={13} />
          <span>Switch</span>
        </Button>
        <Button variant="secondary" size="sm" onclick={handlePauseResume}>
          {#if snapshot.state === 'Paused'}<Play size={13} /><span>Resume</span>{:else}<Pause size={13} /><span>Pause</span>{/if}
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
        <strong>Manual claim required</strong>
        <span>{platName} requested a manual verification step to claim a completed drop.</span>
      </div>
      <Button variant="primary" size="sm" onclick={handleOpenInventory}>
        <span>Open drops inventory</span>
      </Button>
    </div>
  {/if}

  {#if snapshot.state === 'LoggedOut' && snapshot.campaigns.length === 0}
    <section class="designed-empty login-empty">
      <span class="empty-ico" aria-hidden="true"><LogIn size={18} /></span>
      <strong>Sign in to {platName}</strong>
      <p>
        Open {platName}'s official sign-in inside Dropilot's isolated window to load active campaigns and track drops.
      </p>
      <div class="login-actions">
        <Button variant="primary" size="md" onclick={handleLogin}>
          <LogIn size={14} />
          <span>Log in to {platName}</span>
        </Button>
        <Button variant="secondary" size="md" onclick={handleOpenBrowserDrops}>
          <ExternalLink size={14} />
          <span>Open {platName} in browser</span>
        </Button>
      </div>
      <div class="session-note">
        <Info size={14} />
        <span>
          <strong>Isolated sign-in:</strong> Dropilot never reads your personal browser profiles. {platName} Drops sync and integrity checks require a separate isolated session, and your token is saved securely in Windows Credential Manager so you only sign in once.
        </span>
      </div>
    </section>
  {:else}
    {#if snapshot.state === 'LoggedOut'}
      <div class="card public-bar">
        <div class="banner-text">
          <strong>Browsing public {platName} drops catalog</strong>
          <span>
            Opens Dropilot's isolated sign-in window (required for {platName} Drops sync; never reads your browser profiles and saves securely in Windows Credential Manager).
          </span>
        </div>
        <div class="public-bar-actions">
          <Button variant="secondary" size="sm" onclick={handleOpenBrowserDrops}>
            <ExternalLink size={13} />
            <span>Open {platName} in browser</span>
          </Button>
          <Button variant="primary" size="sm" onclick={handleLogin}>
            <LogIn size={13} />
            <span>Log in to {platName}</span>
          </Button>
        </div>
      </div>
    {:else}
      <NowWatchingCard
        {platform}
        state={snapshot.state}
        stateReason={snapshot.stateReason}
        lastError={snapshot.lastError}
        nextActionAt={snapshot.nextActionAt}
        watchingSince={snapshot.watchingSince}
        progressSyncedAt={snapshot.progressSyncedAt}
        channel={snapshot.watching}
        campaign={currentCampaign}
        drop={currentDrop}
        candidates={participatingChannels}
        pinnedChannel={activePinnedChannel}
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
            {#if snapshot.state === 'Error' && snapshot.lastError}
              <span class="disc-err">Sync issue</span>
            {:else if !snapshot.account}
              <span>Public catalog</span>
            {:else}
              <span>@{snapshot.account.login}</span>
            {/if}
          </div>
        </div>

        {#if gameGroups.length === 0}
          <div class="designed-empty">
            {#if snapshot.campaigns.length === 0 && (refreshing || snapshot.state === 'Selecting')}
              <span class="empty-ico spin-ico" aria-hidden="true"><LoaderCircle size={18} /></span>
              <strong>Syncing {platName} campaigns...</strong>
              <p>Fetching live and upcoming drop campaigns from {platName}.</p>
            {:else if snapshot.campaigns.length === 0 && snapshot.state === 'Error'}
              <span class="empty-ico err-ico" aria-hidden="true"><AlertTriangle size={18} /></span>
              <strong>Could not load {platName} campaigns</strong>
              <p>{snapshot.lastError ?? 'Platform sync failed.'}</p>
              <Button variant="primary" size="sm" onclick={handleRetry}>
                <RefreshCw size={13} /> Retry now
              </Button>
            {:else if snapshot.campaigns.length === 0}
              <span class="empty-ico" aria-hidden="true"><Gift size={18} /></span>
              <strong>No active or upcoming {platName} campaigns right now</strong>
              <p>
                Last checked {formatRelativeTime($lastSnapshotAt, $slowNowMs)}.
                {#if snapshot.nextActionAt}
                  Next check in <Countdown targetIso={snapshot.nextActionAt} />.
                {/if}
              </p>
              <Button variant="secondary" size="sm" onclick={handleRetry}>
                <RefreshCw size={13} /> Refresh now
              </Button>
            {:else if trimmedSearch}
              <span class="empty-ico" aria-hidden="true"><Search size={18} /></span>
              {#if searchMatchesInCatalog > 0}
                <strong>No "{trimmedSearch}" campaigns match active filters</strong>
                <p>
                  {searchMatchesInCatalog} matching {searchMatchesInCatalog === 1 ? 'campaign is' : 'campaigns are'} hidden by the "{filter !== 'all' ? filter : selectedGame}" filter.
                </p>
                <Button variant="primary" size="sm" onclick={clearFilters}>
                  Clear filters
                </Button>
              {:else}
                <strong>No active or upcoming {platName} drop campaigns for "{trimmedSearch}" right now.</strong>
                <p>
                  Add "{trimmedSearch}" to your {platName} priority watchlist to auto-farm when a drop campaign launches.
                </p>
                <div class="empty-actions">
                  <Button
                    variant="primary"
                    size="sm"
                    disabled={addingWatch}
                    onclick={() => handleAddWatchedGame(trimmedSearch)}
                  >
                    <Star size={13} />
                    <span>Watch "{trimmedSearch}" for future drops</span>
                  </Button>
                  <Button variant="secondary" size="sm" onclick={() => (search = '')}>
                    Clear search
                  </Button>
                </div>
              {/if}
            {:else}
              <span class="empty-ico" aria-hidden="true"><Gift size={18} /></span>
              <strong>No campaigns match these filters</strong>
              <p>Try switching status or clearing your search query.</p>
              <Button variant="secondary" size="sm" onclick={clearFilters}>
                Clear filters
              </Button>
            {/if}
          </div>
        {:else}
          <GameDropGrid
            groups={gameGroups}
            {settings}
            showPlatformBadge={false}
            onToggleGameFarm={handleToggleGameFarm}
            onToggleCampaignFarm={handleToggleCampaignFarm}
            onExcludeCampaign={(camp) => handleToggleCampaignFarm(camp, false)}
            onOpenUrl={handleOpenUrl}
            onOpenInventory={() => handleOpenInventory()}
            onFetchDetails={handleFetchDetails}
          />
        {/if}
      </div>

      <div class="rail-col">
        <PriorityList
          items={platSettings.priority}
          campaigns={snapshot.campaigns}
          {platform}
          prioritizeEndingSoon={platSettings.prioritizeEndingSoon}
          onReorder={handleReorderPriority}
          onAddGame={handleAddWatchedGame}
          onToggleEndingSoon={handleToggleEndingSoon}
        />

        <section class="card candidates-card">
          <SectionHeader title="Participating channels" count={participatingChannels.length} uppercase />

          {#if participatingChannels.length === 0}
            <p class="cand-empty">Live channels participating in the active drop will appear here.</p>
          {:else}
            <div class="cand-list">
              {#each participatingChannels.slice(0, 8) as ch (ch.id + ch.login)}
                {@const pinned = activePinnedChannel?.toLowerCase() === ch.login.toLowerCase()}
                <div class="cand-row">
                  <Monogram src={ch.avatarUrl} name={ch.displayName} width={26} height={26} rounded />
                  <div class="cand-meta">
                    <span class="cand-name">{ch.displayName}</span>
                    <span class="cand-sub tabular">
                      {formatViewerCount(ch.viewerCount)} viewers · Drop active
                    </span>
                  </div>
                  <button
                    type="button"
                    class="pin-btn"
                    class:pinned
                    aria-label={pinned ? `Unpin ${ch.displayName}` : `Pin ${ch.displayName}`}
                    data-tip={pinned ? 'Unpin' : 'Pin channel'}
                    onclick={() => handlePinChannel(pinned ? null : ch.login)}
                  >
                    <Pin size={12} />
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
    gap: 8px;
  }

  .banner-warn {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    padding: 12px 14px;
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    background: var(--warning-tint);
    border: 1px solid var(--warning);
    color: var(--warning-text);
  }

  .banner-text {
    flex: 1;
    min-width: 200px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .login-empty {
    padding: 36px 24px;
    gap: 12px;
  }

  .login-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .session-note {
    margin-top: 4px;
    max-width: 580px;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: var(--text-xs);
    line-height: 1.5;
    text-align: left;
  }

  .session-note :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--accent-text);
  }

  .session-note strong {
    color: var(--text);
    font-size: var(--text-xs);
  }

  .public-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .public-bar strong { font-size: var(--text-base); }
  .public-bar span { font-size: var(--text-xs); color: var(--text-muted); line-height: 1.45; }

  .public-bar-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .workspace-split {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 268px;
    gap: var(--grid-gap, 14px);
    align-items: start;
  }

  @media (min-width: 1400px) {
    .workspace-split {
      grid-template-columns: minmax(0, 1fr) 310px;
    }
  }

  @media (min-width: 1850px) {
    .workspace-split {
      grid-template-columns: minmax(0, 1fr) 350px;
    }
  }

  .campaigns-col,
  .rail-col {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
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
    animation: pl-spin 1s linear infinite;
  }

  .err-ico {
    color: var(--error-text);
    border-color: var(--error);
    background: var(--error-tint);
  }

  @keyframes pl-spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .candidates-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
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
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
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
    font-size: 11px;
    color: var(--text-muted);
  }

  .pin-btn {
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-muted);
    width: 26px;
    height: 26px;
    border-radius: var(--radius-xs);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .pin-btn:hover,
  .pin-btn.pinned {
    color: var(--accent-text);
    border-color: var(--accent);
    background: var(--accent-tint);
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
