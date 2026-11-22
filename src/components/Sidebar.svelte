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
    if (!snap) return 'Off';
    return snap.state === 'LoggedOut' ? 'Off' : snap.state;
  }

  function stateTone(snap: PlatformSnapshot | null): string {
    if (!snap) return 'tone-neutral';
    switch (snap.state) {
      case 'Mining':
        return 'tone-success';
      case 'Claiming':
      case 'Selecting':
        return 'tone-info';
      case 'Idle':
      case 'Paused':
        return 'tone-warning';
      case 'Error':
        return 'tone-error';
      default:
        return 'tone-neutral';
    }
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
      <svg class="mark" viewBox="0 0 32 32" width="30" height="30" aria-hidden="true">
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
        <div class="brand-text">
          <span class="brand-name">DropPilot</span>
          <span class="brand-sub">Auto Drops</span>
        </div>
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
        <ChevronRight size={14} />
      {:else}
        <span style="display:inline-flex;transform:rotate(180deg)"><ChevronRight size={14} /></span>
      {/if}
    </button>
  </div>

  <nav class="nav">
    {#if !$sidebarCollapsed}
      <div class="nav-group-label">Overview</div>
    {/if}

    <button
      type="button"
      class="nav-item nav-dashboard"
      class:active={active === 'dashboard'}
      onclick={() => onnavigate('dashboard')}
      title="Dashboard (Ctrl+1)"
    >
      <span class="nav-ico ico-cyan" aria-hidden="true"><LayoutDashboard size={16} /></span>
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
      <span class="nav-ico ico-pink" aria-hidden="true"><Gift size={16} /></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">All Drops</span>
        {#if totalActiveCount > 0}
          <span class="nav-count count-pink tabular">{totalActiveCount}</span>
        {/if}
      {/if}
    </button>

    {#if !$sidebarCollapsed}
      <div class="nav-group-label">Platforms</div>
    {:else}
      <div class="nav-divider" aria-hidden="true"></div>
    {/if}

    <button
      type="button"
      class="nav-item nav-twitch"
      class:active={active === 'twitch'}
      onclick={() => onnavigate('twitch')}
      title="Twitch (Ctrl+3)"
    >
      <span class="nav-ico" aria-hidden="true"><span class="plat-pip pip-twitch"></span></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">Twitch</span>
        {#if twActiveCount > 0}
          <span class="nav-count count-twitch tabular">{twActiveCount}</span>
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
      <span class="nav-ico" aria-hidden="true"><span class="plat-pip pip-kick"></span></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">Kick</span>
        {#if kkActiveCount > 0}
          <span class="nav-count count-kick tabular">{kkActiveCount}</span>
        {/if}
      {/if}
    </button>

    {#if !$sidebarCollapsed}
      <div class="nav-group-label">System</div>
    {:else}
      <div class="nav-divider" aria-hidden="true"></div>
    {/if}

    <button
      type="button"
      class="nav-item nav-logs"
      class:active={active === 'logs'}
      onclick={() => onnavigate('logs')}
      title="Logs (Ctrl+5)"
    >
      <span class="nav-ico ico-info" aria-hidden="true"><Clock size={16} /></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">Logs</span>
        {#if $unreadAlertCount > 0}
          <span class="nav-count count-warn tabular">{$unreadAlertCount}</span>
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
      <span class="nav-ico ico-warn" aria-hidden="true"><SettingsIcon size={16} /></span>
      {#if !$sidebarCollapsed}
        <span class="nav-label">Settings</span>
      {/if}
    </button>
  </nav>

  <div class="footer">
    {#if !$sidebarCollapsed}
      <div class="accounts-dock">
        <button type="button" class="dock-row" onclick={() => onnavigate('twitch')}>
          <div class="avatar-wrap">
            <Monogram
              src={tw?.account?.avatarUrl}
              name={tw?.account?.displayName ?? 'TW'}
              width={26}
              height={26}
              rounded
            />
            <span class="avatar-dot {dotClass(tw)}" aria-hidden="true"></span>
          </div>
          <div class="dock-meta">
            <span class="dock-login">{tw?.account ? `@${tw.account.login}` : 'Connect Twitch'}</span>
            <span class="dock-plat">Twitch</span>
          </div>
          <span class="dock-state {stateTone(tw)}">{stateLabel(tw)}</span>
        </button>

        <button type="button" class="dock-row dock-row-sep" onclick={() => onnavigate('kick')}>
          <div class="avatar-wrap">
            <Monogram
              src={kk?.account?.avatarUrl}
              name={kk?.account?.displayName ?? 'KK'}
              width={26}
              height={26}
              rounded
            />
            <span class="avatar-dot {dotClass(kk)}" aria-hidden="true"></span>
          </div>
          <div class="dock-meta">
            <span class="dock-login">{kk?.account ? `@${kk.account.login}` : 'Connect Kick'}</span>
            <span class="dock-plat">Kick</span>
          </div>
          <span class="dock-state {stateTone(kk)}">{stateLabel(kk)}</span>
        </button>
      </div>
    {:else}
      <div class="collapsed-accts">
        <button
          type="button"
          class="mini-avatar-btn"
          onclick={() => onnavigate('twitch')}
          title={`Twitch (${stateLabel(tw)})`}
        >
          <div class="avatar-wrap">
            <Monogram src={tw?.account?.avatarUrl} name={tw?.account?.displayName ?? 'TW'} width={26} height={26} rounded />
            <span class="avatar-dot {dotClass(tw)}" aria-hidden="true"></span>
          </div>
        </button>
        <button
          type="button"
          class="mini-avatar-btn"
          onclick={() => onnavigate('kick')}
          title={`Kick (${stateLabel(kk)})`}
        >
          <div class="avatar-wrap">
            <Monogram src={kk?.account?.avatarUrl} name={kk?.account?.displayName ?? 'KK'} width={26} height={26} rounded />
            <span class="avatar-dot {dotClass(kk)}" aria-hidden="true"></span>
          </div>
        </button>
      </div>
    {/if}

    <button
      type="button"
      class="pause-all-btn"
      class:is-paused={allPaused}
      onclick={togglePauseAll}
      title={allPaused ? 'Resume all miners (Ctrl+P)' : 'Pause all miners (Ctrl+P)'}
    >
      {#if allPaused}
        <Play size={13} />
        {#if !$sidebarCollapsed}<span>Resume all</span>{/if}
      {:else}
        <Pause size={13} />
        {#if !$sidebarCollapsed}<span>Pause all</span>{/if}
      {/if}
    </button>

    {#if !$sidebarCollapsed}
      <div class="meta-foot tabular">
        <span>Next sync: <Countdown targetIso={nextCheckIso} /></span>
        <span class="ver">v0.1.0</span>
      </div>
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    width: var(--sidebar-width);
    flex-shrink: 0;
    background: linear-gradient(180deg, #10152a 0%, #0b0e1d 55%, #090c18 100%);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 14px 12px 12px;
    gap: 12px;
  }

  :global(:root[data-theme="light"]) .sidebar {
    background: var(--sidebar-bg);
  }

  .sidebar.collapsed {
    width: var(--sidebar-collapsed);
    padding: 12px 8px;
    align-items: center;
  }

  .brand-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 2px 4px 6px;
    width: 100%;
  }

  .sidebar.collapsed .brand-row {
    flex-direction: column;
    gap: 8px;
    padding: 0 0 4px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .brand-text {
    display: flex;
    flex-direction: column;
    line-height: 1.1;
  }

  .brand-name {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--text);
  }

  .brand-sub {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--text-faint);
    margin-top: 2px;
  }

  .collapse-btn {
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-faint);
    width: 26px;
    height: 26px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
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
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
    padding: 10px 10px 4px;
  }

  .nav-group-label:first-child {
    padding-top: 2px;
  }

  .nav-divider {
    height: 1px;
    background: var(--border);
    margin: 6px 8px;
  }

  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 11px;
    border: 1px solid transparent;
    border-radius: 10px;
    background: transparent;
    color: var(--text-muted);
    font-size: 13px;
    font-weight: 600;
    text-align: left;
  }

  .nav-ico {
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    opacity: 0.85;
  }

  .ico-cyan { color: var(--cyan-text); }
  .ico-pink { color: var(--pink-text); }
  .ico-info { color: var(--info-text); }
  .ico-warn { color: var(--warning-text); }

  .plat-pip {
    width: 9px;
    height: 9px;
    border-radius: var(--radius-pill);
  }

  .pip-twitch {
    background: var(--twitch);
    box-shadow: 0 0 8px rgba(168, 85, 247, 0.7);
  }

  .pip-kick {
    background: var(--kick);
    box-shadow: 0 0 8px rgba(83, 252, 24, 0.55);
  }

  .sidebar.collapsed .nav-item {
    justify-content: center;
    padding: 0;
  }

  .nav-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .nav-count {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: var(--radius-pill);
    line-height: 1.5;
  }

  .count-pink {
    background: var(--pink-tint);
    color: var(--pink-text);
  }

  .count-twitch {
    background: var(--twitch-tint);
    color: var(--twitch-text);
  }

  .count-kick {
    background: var(--kick-tint);
    color: var(--kick-text);
  }

  .count-warn {
    background: var(--warning-tint);
    color: var(--warning-text);
  }

  .nav-item:hover {
    background: rgba(255, 255, 255, 0.04);
    color: var(--text);
  }

  :global(:root[data-theme="light"]) .nav-item:hover {
    background: var(--surface-2);
  }

  .nav-item:hover .nav-ico {
    opacity: 1;
  }

  .nav-item.active {
    color: var(--text);
    background: var(--accent-tint);
    border-color: rgba(129, 140, 248, 0.22);
  }

  .nav-item.active .nav-ico {
    opacity: 1;
  }

  .nav-item.nav-dashboard.active {
    background: linear-gradient(90deg, rgba(56, 189, 248, 0.16) 0%, rgba(129, 140, 248, 0.05) 100%);
    border-color: rgba(56, 189, 248, 0.28);
  }

  .nav-item.nav-drops.active {
    background: linear-gradient(90deg, rgba(236, 72, 153, 0.17) 0%, rgba(168, 85, 247, 0.05) 100%);
    border-color: rgba(236, 72, 153, 0.28);
  }

  .nav-item.nav-twitch.active {
    background: linear-gradient(90deg, rgba(168, 85, 247, 0.19) 0%, rgba(168, 85, 247, 0.05) 100%);
    border-color: rgba(168, 85, 247, 0.30);
  }

  .nav-item.nav-kick.active {
    background: linear-gradient(90deg, rgba(83, 252, 24, 0.14) 0%, rgba(16, 185, 129, 0.05) 100%);
    border-color: rgba(83, 252, 24, 0.28);
  }

  .nav-item.nav-logs.active {
    background: linear-gradient(90deg, rgba(56, 189, 248, 0.15) 0%, transparent 100%);
    border-color: rgba(56, 189, 248, 0.26);
  }

  .nav-item.nav-settings.active {
    background: linear-gradient(90deg, rgba(251, 191, 36, 0.15) 0%, transparent 100%);
    border-color: rgba(251, 191, 36, 0.26);
  }

  .nav-item.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
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

  .footer {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }

  .accounts-dock {
    display: flex;
    flex-direction: column;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--border);
    overflow: hidden;
  }

  :global(:root[data-theme="light"]) .accounts-dock {
    background: var(--surface-2);
  }

  .dock-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 9px;
    width: 100%;
    border: none;
    background: transparent;
    text-align: left;
  }

  .dock-row-sep {
    border-top: 1px solid var(--border);
  }

  .dock-row:hover {
    background: var(--surface-hover);
  }

  .avatar-wrap {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
  }

  .avatar-dot {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
    border: 2px solid #0d1224;
  }

  :global(:root[data-theme="light"]) .avatar-dot {
    border-color: var(--surface);
  }

  .dot-success { background: var(--success); }
  .dot-info { background: var(--info); }
  .dot-warning { background: var(--warning); }
  .dot-error { background: var(--error); }
  .dot-neutral { background: var(--neutral-dot); }

  .dock-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.2;
  }

  .dock-login {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dock-plat {
    font-size: 10px;
    color: var(--text-faint);
  }

  .dock-state {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: var(--radius-pill);
    flex-shrink: 0;
  }

  .tone-success {
    background: var(--success-tint);
    color: var(--success-text);
  }

  .tone-info {
    background: var(--info-tint);
    color: var(--info-text);
  }

  .tone-warning {
    background: var(--warning-tint);
    color: var(--warning-text);
  }

  .tone-error {
    background: var(--error-tint);
    color: var(--error-text);
  }

  .tone-neutral {
    background: var(--neutral-tint);
    color: var(--neutral-text);
  }

  .collapsed-accts {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }

  .mini-avatar-btn {
    border: 1px solid transparent;
    background: transparent;
    padding: 3px;
    border-radius: var(--radius-pill);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .mini-avatar-btn:hover {
    background: var(--surface-2);
    border-color: var(--border);
  }

  .pause-all-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    width: 100%;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
  }

  :global(:root[data-theme="light"]) .pause-all-btn {
    background: var(--surface-2);
  }

  .pause-all-btn:hover {
    background: var(--surface-hover);
    border-color: var(--border-hover);
  }

  .pause-all-btn.is-paused {
    background: var(--warning-tint);
    border-color: rgba(251, 191, 36, 0.35);
    color: var(--warning-text);
  }

  .meta-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 10px;
    color: var(--text-faint);
    padding: 0 3px;
  }

  @media (prefers-reduced-motion: no-preference) {
    .nav-item,
    .pause-all-btn,
    .dock-row {
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
