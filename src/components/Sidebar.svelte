<script lang="ts">
  import { onMount } from 'svelte';
  import BarChart3 from 'lucide-svelte/icons/bar-chart-3';
  import ChevronRight from 'lucide-svelte/icons/chevron-right';
  import Gift from 'lucide-svelte/icons/gift';
  import LayoutDashboard from 'lucide-svelte/icons/layout-dashboard';
  import SettingsIcon from 'lucide-svelte/icons/settings';
  import Sparkles from 'lucide-svelte/icons/sparkles';
  import Terminal from 'lucide-svelte/icons/terminal';
  import { api } from '../lib/api';
  import { slowNowMs } from '../lib/clock';
  import { isCampaignActiveNow } from '../lib/selectors';
  import { appSettings, epicFeed, notifyUi, sidebarCollapsed, unreadAlertCount } from '../lib/stores';
  import type { AppSnapshot, NavRoute, PlatformId, PlatformSnapshot, ProfileSnapshot } from '../lib/types';
  import BrandMark from './BrandMark.svelte';

  interface Props {
    active: NavRoute;
    snapshot: AppSnapshot | null;
    onnavigate: (route: NavRoute) => void;
  }

  let { active, snapshot, onnavigate }: Props = $props();

  let narrowViewport = $state(false);
  onMount(() => {
    const mq = window.matchMedia('(max-width: 760px)');
    narrowViewport = mq.matches;
    const onChange = (ev: MediaQueryListEvent) => {
      narrowViewport = ev.matches;
    };
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  });

  const effectiveCollapsed = $derived($sidebarCollapsed || narrowViewport);

  const tw = $derived(snapshot?.twitch ?? null);
  const kk = $derived(snapshot?.kick ?? null);

  const dockProfiles = $derived<ProfileSnapshot[]>(
    snapshot?.profiles && snapshot.profiles.length > 0
      ? snapshot.profiles
      : [
          {
            profileId: $appSettings?.activeTwitchProfileId ?? 'default-twitch',
            label: 'Main',
            platform: 'twitch',
            snapshot: tw ?? {
              platform: 'twitch',
              state: 'LoggedOut',
              stateReason: null,
              account: null,
              watching: null,
              currentCampaignId: null,
              currentDropId: null,
              nextActionAt: null,
              lastError: null,
              campaigns: [],
              candidateChannels: [],
            },
          },
          {
            profileId: $appSettings?.activeKickProfileId ?? 'default-kick',
            label: 'Main',
            platform: 'kick',
            snapshot: kk ?? {
              platform: 'kick',
              state: 'LoggedOut',
              stateReason: null,
              account: null,
              watching: null,
              currentCampaignId: null,
              currentDropId: null,
              nextActionAt: null,
              lastError: null,
              campaigns: [],
              candidateChannels: [],
            },
          },
        ],
  );
  const multiTwitch = $derived(dockProfiles.filter((p) => p.platform === 'twitch').length > 1);
  const multiKick = $derived(dockProfiles.filter((p) => p.platform === 'kick').length > 1);

  function isActiveProfile(prof: ProfileSnapshot): boolean {
    const activeId =
      prof.platform === 'twitch'
        ? ($appSettings?.activeTwitchProfileId ?? 'default-twitch')
        : ($appSettings?.activeKickProfileId ?? 'default-kick');
    return prof.profileId === activeId;
  }

  async function selectDockProfile(platform: PlatformId, profileId: string) {
    try {
      const currentActive =
        platform === 'twitch'
          ? ($appSettings?.activeTwitchProfileId ?? 'default-twitch')
          : ($appSettings?.activeKickProfileId ?? 'default-kick');
      if (profileId && currentActive !== profileId) {
        const updated = await api.setActiveProfile(platform, profileId);
        appSettings.set(updated);
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    }
    onnavigate(platform);
  }

  const twActiveCount = $derived(
    tw ? tw.campaigns.filter((c) => isCampaignActiveNow(c, $slowNowMs)).length : 0,
  );
  const kkActiveCount = $derived(
    kk ? kk.campaigns.filter((c) => isCampaignActiveNow(c, $slowNowMs)).length : 0,
  );
  const totalActiveCount = $derived(twActiveCount + kkActiveCount);
  const epicUnclaimedCount = $derived(
    ($epicFeed?.games ?? []).filter((g) => {
      if (g.status !== 'active' || g.claimed) return false;
      const ts = new Date(g.endsAt).getTime();
      return Number.isNaN(ts) || ts > $slowNowMs;
    }).length,
  );

  function stateLabel(snap: PlatformSnapshot | null): string {
    if (!snap) return 'Off';
    return snap.state === 'LoggedOut' ? 'Off' : snap.state;
  }
</script>

<aside class="sidebar" class:collapsed={effectiveCollapsed} aria-label="Main navigation">
  <div class="brand-row">
    <button type="button" class="brand-btn" onclick={() => onnavigate('dashboard')}>
      <BrandMark size={20} showWordmark={!effectiveCollapsed} compact={effectiveCollapsed} />
    </button>

    {#if !narrowViewport}
      <button
        type="button"
        class="collapse-btn"
        aria-label={$sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        data-tip={$sidebarCollapsed ? 'Expand' : 'Collapse'}
        onclick={() => sidebarCollapsed.update((v) => !v)}
      >
        {#if $sidebarCollapsed}
          <ChevronRight size={14} />
        {:else}
          <span style="display:inline-flex;transform:rotate(180deg)"><ChevronRight size={14} /></span>
        {/if}
      </button>
    {/if}
  </div>

  <nav class="nav">
    {#if !effectiveCollapsed}
      <div class="nav-group-label">VIEWS</div>
    {/if}

    <button
      type="button"
      class="nav-item"
      class:active={active === 'dashboard'}
      onclick={() => onnavigate('dashboard')}
      title="Dashboard (Ctrl+1)"
    >
      <span class="nav-ico" aria-hidden="true"><LayoutDashboard size={14} /></span>
      {#if !effectiveCollapsed}
        <span class="nav-label">Dashboard</span>
      {/if}
    </button>

    <button
      type="button"
      class="nav-item"
      class:active={active === 'drops'}
      onclick={() => onnavigate('drops')}
      title="All Drops (Ctrl+2)"
    >
      <span class="nav-ico" aria-hidden="true"><Gift size={14} /></span>
      {#if !effectiveCollapsed}
        <span class="nav-label">All Drops</span>
        {#if totalActiveCount > 0}
          <span class="nav-count tabular">{totalActiveCount}</span>
        {/if}
      {/if}
    </button>

    <button
      type="button"
      class="nav-item"
      class:active={active === 'history'}
      onclick={() => onnavigate('history')}
      title="History & Stats (Ctrl+3)"
    >
      <span class="nav-ico" aria-hidden="true"><BarChart3 size={14} /></span>
      {#if !effectiveCollapsed}
        <span class="nav-label">History</span>
      {/if}
    </button>

    {#if !effectiveCollapsed}
      <div class="nav-group-label">MINERS</div>
    {:else}
      <div class="nav-divider" aria-hidden="true"></div>
    {/if}

    <button
      type="button"
      class="nav-item"
      class:active={active === 'twitch'}
      onclick={() => onnavigate('twitch')}
      title="Twitch (Ctrl+4)"
    >
      <span class="nav-ico" aria-hidden="true"><span class="plat-dot dot-twitch"></span></span>
      {#if !effectiveCollapsed}
        <span class="nav-label">Twitch</span>
        {#if twActiveCount > 0}
          <span class="nav-count tabular">{twActiveCount}</span>
        {/if}
      {/if}
    </button>

    <button
      type="button"
      class="nav-item"
      class:active={active === 'kick'}
      onclick={() => onnavigate('kick')}
      title="Kick (Ctrl+5)"
    >
      <span class="nav-ico" aria-hidden="true"><span class="plat-dot dot-kick"></span></span>
      {#if !effectiveCollapsed}
        <span class="nav-label">Kick</span>
        {#if kkActiveCount > 0}
          <span class="nav-count tabular">{kkActiveCount}</span>
        {/if}
      {/if}
    </button>

    {#if !effectiveCollapsed}
      <div class="nav-group-label">REWARDS</div>
    {:else}
      <div class="nav-divider" aria-hidden="true"></div>
    {/if}

    <button
      type="button"
      class="nav-item"
      class:active={active === 'epic'}
      onclick={() => onnavigate('epic')}
      title="Epic Free Weekly (Ctrl+6)"
    >
      <span class="nav-ico" aria-hidden="true"><Sparkles size={14} /></span>
      {#if !effectiveCollapsed}
        <span class="nav-label">Epic Free</span>
        {#if epicUnclaimedCount > 0}
          <span class="nav-count count-warn tabular">{epicUnclaimedCount}</span>
        {/if}
      {/if}
    </button>

    {#if !effectiveCollapsed}
      <div class="nav-group-label">SYSTEM</div>
    {:else}
      <div class="nav-divider" aria-hidden="true"></div>
    {/if}

    <button
      type="button"
      class="nav-item"
      class:active={active === 'logs'}
      onclick={() => onnavigate('logs')}
      title="Logs (Ctrl+7)"
    >
      <span class="nav-ico" aria-hidden="true"><Terminal size={14} /></span>
      {#if !effectiveCollapsed}
        <span class="nav-label">Logs</span>
        {#if $unreadAlertCount > 0}
          <span class="nav-count count-warn tabular">{$unreadAlertCount}</span>
        {/if}
      {/if}
    </button>

    <button
      type="button"
      class="nav-item"
      class:active={active === 'settings'}
      onclick={() => onnavigate('settings')}
      title="Settings (Ctrl+8)"
    >
      <span class="nav-ico" aria-hidden="true"><SettingsIcon size={14} /></span>
      {#if !effectiveCollapsed}
        <span class="nav-label">Settings</span>
      {/if}
    </button>
  </nav>

  <div class="footer">
    {#if !effectiveCollapsed}
      <div class="accounts-dock">
        {#each dockProfiles as prof, idx (prof.profileId)}
          {@const showTag = prof.platform === 'twitch' ? multiTwitch : multiKick}
          {@const isSelected = isActiveProfile(prof)}
          <button
            type="button"
            class="dock-row"
            class:dock-row-sep={idx > 0}
            class:dock-row-active={showTag && isSelected}
            onclick={() => selectDockProfile(prof.platform, prof.profileId)}
            title={`${prof.platform === 'twitch' ? 'Twitch' : 'Kick'} (${prof.label}) — ${stateLabel(prof.snapshot)}`}
          >
            <span class="plat-dot dot-{prof.platform}" aria-hidden="true"></span>
            <span class="dock-login">
              {prof.snapshot.account
                ? `@${prof.snapshot.account.login}`
                : showTag
                  ? `${prof.platform === 'twitch' ? 'Twitch' : 'Kick'} · ${prof.label}`
                  : `Connect ${prof.platform === 'twitch' ? 'Twitch' : 'Kick'}`}
            </span>
            {#if showTag && prof.snapshot.account}
              <span class="dock-tag">{prof.label}</span>
            {/if}
            <span class="dock-state tabular">{stateLabel(prof.snapshot)}</span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="collapsed-accts">
        {#each dockProfiles as prof (prof.profileId)}
          <button
            type="button"
            class="mini-avatar-btn"
            onclick={() => selectDockProfile(prof.platform, prof.profileId)}
            title={`${prof.platform === 'twitch' ? 'Twitch' : 'Kick'} (${prof.label}: ${stateLabel(prof.snapshot)})`}
          >
            <span class="plat-dot dot-{prof.platform}"></span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    width: var(--sidebar-width);
    flex-shrink: 0;
    background: var(--sidebar-bg);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 10px 8px 8px;
    gap: 8px;
  }

  .sidebar.collapsed {
    width: var(--sidebar-collapsed);
    padding: 10px 6px;
    align-items: center;
  }

  .brand-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 2px 8px;
    border-bottom: 1px solid var(--border);
    width: 100%;
  }

  .sidebar.collapsed .brand-row {
    flex-direction: column;
    gap: 6px;
    padding: 0 0 6px;
  }

  .brand-btn {
    border: none;
    background: transparent;
    padding: 0;
    display: inline-flex;
    align-items: center;
    min-width: 0;
  }

  .collapse-btn {
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-faint);
    width: 22px;
    height: 22px;
    border-radius: var(--radius-xs);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .collapse-btn:hover {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--text);
  }

  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    width: 100%;
  }

  .nav-group-label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    color: var(--text-faint);
    padding: 10px 6px 3px;
  }

  .nav-group-label:first-child {
    padding-top: 2px;
  }

  .nav-divider {
    height: 1px;
    background: var(--border);
    margin: 6px 4px;
  }

  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 500;
    text-align: left;
  }

  .sidebar.collapsed .nav-item {
    justify-content: center;
    padding: 0;
  }

  .nav-ico {
    width: 16px;
    height: 16px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--text-faint);
  }

  .nav-item:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .nav-item:hover .nav-ico {
    color: var(--text-muted);
  }

  .nav-item.active {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--accent-text);
    font-weight: 600;
  }

  .nav-item.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 4px;
    bottom: 4px;
    width: 2px;
    background: var(--accent);
  }

  .nav-item.active .nav-ico {
    color: var(--accent-text);
  }

  .nav-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .nav-count {
    min-width: 26px;
    text-align: center;
    flex-shrink: 0;
    font-size: 10.5px;
    font-weight: 600;
    padding: 0 5px;
    border-radius: var(--radius-xs);
    background: var(--surface);
    color: var(--text-muted);
    line-height: 1.55;
  }

  .nav-item.active .nav-count {
    color: var(--text);
  }

  .count-warn {
    background: var(--warning-tint);
    color: var(--warning-text);
  }

  .footer {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  .accounts-dock {
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-xs);
    background: var(--surface);
    border: 1px solid var(--border);
    overflow: hidden;
  }

  .dock-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 7px;
    width: 100%;
    border: none;
    background: transparent;
    text-align: left;
  }

  .dock-row-sep {
    border-top: 1px solid var(--border);
  }

  .dock-row:hover {
    background: var(--surface-2);
  }

  .dock-row-active {
    background: var(--accent-tint);
  }

  .dock-tag {
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 600;
    padding: 0 4px;
    border-radius: 2px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .dock-login {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 500;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dock-state {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .collapsed-accts {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }

  .mini-avatar-btn {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border);
    background: var(--surface);
    border-radius: var(--radius-xs);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .mini-avatar-btn:hover {
    border-color: var(--border-hover);
  }
</style>
