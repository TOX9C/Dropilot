<script lang="ts">
  import ChevronRight from 'lucide-svelte/icons/chevron-right';
  import Clock from 'lucide-svelte/icons/clock';
  import Gift from 'lucide-svelte/icons/gift';
  import LayoutDashboard from 'lucide-svelte/icons/layout-dashboard';
  import Pause from 'lucide-svelte/icons/pause';
  import Play from 'lucide-svelte/icons/play';
  import SettingsIcon from 'lucide-svelte/icons/settings';
  import { api } from '../lib/api';
  import { earliestNextActionAt } from '../lib/selectors';
  import { notifyUi, sidebarCollapsed, unreadAlertCount } from '../lib/stores';
  import type { AppSnapshot, NavRoute, PlatformSnapshot } from '../lib/types';
  import Badge from './Badge.svelte';
  import Countdown from './Countdown.svelte';
  import Monogram from './Monogram.svelte';

  interface Props {
    active: NavRoute;
    snapshot: AppSnapshot | null;
    onnavigate: (route: NavRoute) => void;
  }

  let { active, snapshot, onnavigate }: Props = $props();

  const tw = $derived(snapshot?.twitch ?? null);
  const kk = $derived(snapshot?.kick ?? null);

  const twActiveCount = $derived(
    tw ? tw.campaigns.filter((c) => c.status === 'active').length : 0,
  );
  const kkActiveCount = $derived(
    kk ? kk.campaigns.filter((c) => c.status === 'active').length : 0,
  );
  const totalActiveCount = $derived(twActiveCount + kkActiveCount);

  const allPaused = $derived(
    Boolean(tw && kk && tw.state === 'Paused' && kk.state === 'Paused'),
  );
  const nextCheckIso = $derived(snapshot ? earliestNextActionAt(snapshot) : null);

  function stateLabel(snap: PlatformSnapshot | null): string {
    if (!snap) return 'Logged out';
    return snap.state === 'LoggedOut' ? 'Logged out' : snap.state;
  }

  function dotClass(snap: PlatformSnapshot | null): string {
    if (!snap) return 'dot-neutral';
    switch (snap.state) {
      case 'Mining':
        return 'dot-success dot-pulse';
      case 'Claiming':
      case 'Selecting':
        return 'dot-info';
      case 'Idle':
      case 'Paused':
        return 'dot-warning';
      case 'Error':
        return 'dot-error';
      default:
        return 'dot-neutral';
    }
  }

  async function togglePauseAll() {
    try {
      if (allPaused) {
        await Promise.all([api.resume('twitch'), api.resume('kick')]);
      } else {
        await Promise.all([api.pause('twitch'), api.pause('kick')]);
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }
</script>

<aside class="sidebar" class:collapsed={$sidebarCollapsed} aria-label="Main navigation">
  <div class="brand-row">
    <div class="brand">
      <svg class="mark" viewBox="0 0 32 32" width="28" height="28" aria-hidden="true">
        <defs>
          <linearGradient id="dp-mark-grad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stop-color="#38bdf8" />
            <stop offset="50%" stop-color="#8b5cf6" />
            <stop offset="100%" stop-color="#ec4899" />
          </linearGradient>
        </defs>
        <rect x="2" y="2" width="28" height="28" rx="9" fill="url(#dp-mark-grad)" />
        <path
          d="M16 7.5C16 7.5 22 14.5 22 18.5C22 21.8137 19.3137 24.5 16 24.5C12.6863 24.5 10 21.8137 10 18.5C10 14.5 16 7.5 16 7.5Z"
          fill="#ffffff"
        />
        <path
          d="M13 19.5L16 16.5L19 19.5"
          stroke="#6e7bff"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          fill="none"
        />
      </svg>
      {#if !$sidebarCollapsed}
        <span class="brand-name">DropPilot</span>
      {/if}
    </div>

    <button
      type="button"
      class="collapse-btn"
      aria-label={$sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
      data-tip={$sidebarCollapsed ? 'Expand' : 'Collapse'}
      onclick={() => sidebarCollapsed.update((v) => !v)}
    >
      {#if $sidebarCollapsed}
        <ChevronRight size={15} />
      {:else}
        <span style="display:inline-flex;transform:rotate(180deg)"><ChevronRight size={15} /></span>
      {/if}
    </button>
  </div>

  <nav class="nav">
    <button
      type="button"
      class="nav-item nav-dashboard"
      class:active={active === 'dashboard'}
      onclick={() => onnavigate('dashboard')}
      title="Dashboard (Ctrl+1)"
    >
      <span class="nav-ico ico-cyan" aria-hidden="true"><LayoutDashboard size={15} /></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">Dashboard</span>
      {/if}
    </button>

    <button
      type="button"
      class="nav-item nav-drops"
      class:active={active === 'drops'}
      onclick={() => onnavigate('drops')}
      title="All Drops (Ctrl+2)"
    >
      <span class="nav-ico ico-pink" aria-hidden="true"><Gift size={15} /></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">All Drops</span>
        {#if totalActiveCount > 0}
          <Badge tone="pink">{totalActiveCount}</Badge>
        {/if}
      {/if}
    </button>

    <button
      type="button"
      class="nav-item nav-twitch"
      class:active={active === 'twitch'}
      onclick={() => onnavigate('twitch')}
      title="Twitch (Ctrl+3)"
    >
      <span class="nav-ico ico-twitch" aria-hidden="true"><span class="plat-dot dot-twitch"></span></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">Twitch</span>
        {#if twActiveCount > 0}
          <Badge tone="twitch">{twActiveCount}</Badge>
        {/if}
      {/if}
    </button>

    <button
      type="button"
      class="nav-item nav-kick"
      class:active={active === 'kick'}
      onclick={() => onnavigate('kick')}
      title="Kick (Ctrl+4)"
    >
      <span class="nav-ico ico-kick" aria-hidden="true"><span class="plat-dot dot-kick"></span></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">Kick</span>
        {#if kkActiveCount > 0}
          <Badge tone="kick">{kkActiveCount}</Badge>
        {/if}
      {/if}
    </button>

    <button
      type="button"
      class="nav-item nav-logs"
      class:active={active === 'logs'}
      onclick={() => onnavigate('logs')}
      title="Logs (Ctrl+5)"
    >
      <span class="nav-ico ico-info" aria-hidden="true"><Clock size={15} /></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">Logs</span>
        {#if $unreadAlertCount > 0}
          <Badge tone="warning">{$unreadAlertCount}</Badge>
        {/if}
      {/if}
    </button>

    <button
      type="button"
      class="nav-item nav-settings"
      class:active={active === 'settings'}
      onclick={() => onnavigate('settings')}
      title="Settings (Ctrl+6)"
    >
      <span class="nav-ico ico-warn" aria-hidden="true"><SettingsIcon size={15} /></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">Settings</span>
      {/if}
    </button>
  </nav>

  <div class="footer">
    {#if !$sidebarCollapsed}
      <button type="button" class="acct-mini acct-twitch" onclick={() => onnavigate('twitch')}>
        <Monogram
          src={tw?.account?.avatarUrl}
          name={tw?.account?.displayName ?? 'TW'}
          width={26}
          height={26}
          rounded
        />
        <div class="acct-meta">
          <span class="acct-login">{tw?.account ? `@${tw.account.login}` : 'Twitch'}</span>
          <span class="acct-state">
            <span class="sdot {dotClass(tw)}" aria-hidden="true"></span>
            <span>Twitch: {stateLabel(tw)}</span>
          </span>
        </div>
      </button>

      <button type="button" class="acct-mini acct-kick" onclick={() => onnavigate('kick')}>
        <Monogram
          src={kk?.account?.avatarUrl}
          name={kk?.account?.displayName ?? 'KK'}
          width={26}
          height={26}
          rounded
        />
        <div class="acct-meta">
          <span class="acct-login">{kk?.account ? `@${kk.account.login}` : 'Kick'}</span>
          <span class="acct-state">
            <span class="sdot {dotClass(kk)}" aria-hidden="true"></span>
            <span>Kick: {stateLabel(kk)}</span>
          </span>
        </div>
      </button>
    {/if}

    <button
      type="button"
      class="pause-all-btn"
      onclick={togglePauseAll}
      title={allPaused ? 'Resume all miners (Ctrl+P)' : 'Pause all miners (Ctrl+P)'}
    >
      {#if allPaused}
        <Play size={14} />
        {#if !$sidebarCollapsed}<span>Resume all</span>{/if}
      {:else}
        <Pause size={14} />
        {#if !$sidebarCollapsed}<span>Pause all</span>{/if}
      {/if}
    </button>

    {#if !$sidebarCollapsed}
      <div class="meta-foot tabular">
        <span>Next check: <Countdown targetIso={nextCheckIso} /></span>
        <span class="ver">v0.1.0</span>
      </div>
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    width: var(--sidebar-width);
    flex-shrink: 0;
    background: var(--sidebar-bg);
    background-image: linear-gradient(180deg, rgba(168, 85, 247, 0.08) 0%, transparent 40%, rgba(56, 189, 248, 0.05) 100%);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: var(--space-3);
    gap: var(--space-3);
  }

  .sidebar.collapsed {
    width: var(--sidebar-collapsed);
    padding: var(--space-3) var(--space-2);
  }

  .brand-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 2px 4px;
  }

  .sidebar.collapsed .brand-row {
    flex-direction: column;
    gap: var(--space-2);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .brand-name {
    font-size: var(--text-md);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .collapse-btn {
    border: none;
    background: transparent;
    color: var(--text-muted);
    width: 26px;
    height: 26px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .collapse-btn:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .nav {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }

  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
    font-size: var(--text-base);
    font-weight: 600;
    text-align: left;
  }

  .nav-ico {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .ico-cyan { background: var(--cyan-tint); color: var(--cyan-text); }
  .ico-pink { background: var(--pink-tint); color: var(--pink-text); }
  .ico-twitch { background: var(--twitch-tint); color: var(--twitch-text); }
  .ico-kick { background: var(--kick-tint); color: var(--kick-text); }
  .ico-info { background: var(--info-tint); color: var(--info-text); }
  .ico-warn { background: var(--warning-tint); color: var(--warning-text); }

  .sidebar.collapsed .nav-item {
    justify-content: center;
    padding: 0;
  }

  .nav-label {
    flex: 1;
  }

  .nav-item:hover {
    background: var(--surface-hover);
    color: var(--text);
  }

  .nav-item.active {
    background: var(--accent-tint);
    border-color: rgba(129, 140, 248, 0.25);
    color: var(--text);
  }

  .nav-item.nav-dashboard.active {
    background: linear-gradient(90deg, rgba(56, 189, 248, 0.20), rgba(129, 140, 248, 0.08));
    border-color: rgba(56, 189, 248, 0.32);
  }
  .nav-item.nav-drops.active {
    background: linear-gradient(90deg, rgba(236, 72, 153, 0.22), rgba(168, 85, 247, 0.09));
    border-color: rgba(236, 72, 153, 0.34);
  }
  .nav-item.nav-twitch.active {
    background: linear-gradient(90deg, rgba(168, 85, 247, 0.24), rgba(168, 85, 247, 0.08));
    border-color: rgba(168, 85, 247, 0.36);
  }
  .nav-item.nav-kick.active {
    background: linear-gradient(90deg, rgba(83, 252, 24, 0.18), rgba(16, 185, 129, 0.08));
    border-color: rgba(83, 252, 24, 0.34);
  }
  .nav-item.nav-logs.active {
    background: linear-gradient(90deg, rgba(56, 189, 248, 0.18), transparent);
    border-color: rgba(56, 189, 248, 0.30);
  }
  .nav-item.nav-settings.active {
    background: linear-gradient(90deg, rgba(251, 191, 36, 0.18), transparent);
    border-color: rgba(251, 191, 36, 0.30);
  }

  .nav-item.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 7px;
    bottom: 7px;
    width: 3px;
    border-radius: var(--radius-pill);
    background: var(--accent);
  }

  .nav-item.nav-dashboard.active::before { background: var(--cyan); }
  .nav-item.nav-drops.active::before { background: var(--pink); }
  .nav-item.nav-twitch.active::before { background: var(--twitch); }
  .nav-item.nav-kick.active::before { background: var(--kick); }
  .nav-item.nav-logs.active::before { background: var(--info); }
  .nav-item.nav-settings.active::before { background: var(--warning); }

  .sidebar.collapsed .plat-dot {
    margin: 0;
  }

  .footer {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }

  .acct-mini {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 6px 8px;
    width: 100%;
    text-align: left;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .acct-twitch {
    border-left: 3px solid var(--twitch);
  }

  .acct-kick {
    border-left: 3px solid var(--kick);
  }

  .acct-mini:hover {
    background: var(--surface-hover);
    border-color: var(--border-hover);
  }

  .acct-meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .acct-login {
    font-size: var(--text-xs);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .acct-state {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .sdot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
    flex-shrink: 0;
  }

  .dot-success { background: var(--success); }
  .dot-info { background: var(--info); }
  .dot-warning { background: var(--warning); }
  .dot-error { background: var(--error); }
  .dot-neutral { background: var(--neutral-dot); }

  .pause-all-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .pause-all-btn:hover {
    background: var(--surface-hover);
    border-color: var(--border-hover);
  }

  .meta-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-faint);
    padding: 0 2px;
  }

  @media (prefers-reduced-motion: no-preference) {
    .nav-item,
    .pause-all-btn {
      transition:
        background-color var(--duration-fast) var(--ease-out),
        color var(--duration-fast) var(--ease-out),
        border-color var(--duration-fast) var(--ease-out);
    }

    .dot-pulse {
      animation: sb-pulse 2s ease-in-out infinite;
    }

    @keyframes sb-pulse {
      0%,
      100% {
        opacity: 1;
      }
      50% {
        opacity: 0.4;
      }
    }
  }
</style>
