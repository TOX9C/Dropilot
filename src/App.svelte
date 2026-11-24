<script lang="ts">
  import { onMount } from 'svelte';
  import Minimize2 from 'lucide-svelte/icons/minimize-2';
  import Pause from 'lucide-svelte/icons/pause';
  import Play from 'lucide-svelte/icons/play';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Button from './components/Button.svelte';
  import Countdown from './components/Countdown.svelte';
  import DisclaimerModal from './components/DisclaimerModal.svelte';
  import ImageLightbox from './components/ImageLightbox.svelte';
  import Modal from './components/Modal.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Skeleton from './components/Skeleton.svelte';
  import Toast from './components/Toast.svelte';
  import UpdateBanner from './components/UpdateBanner.svelte';
  import { api } from './lib/api';
  import { earliestNextActionAt } from './lib/selectors';
  import {
    activeRoute,
    appLogs,
    appSettings,
    appSnapshot,
    closeLightbox,
    initializeAppStores,
    lightboxImage,
    mockFeatureAvailable,
    notifyUi,
    remoteConfigStatus,
    sessionMinutes,
    shortcutsOpen,
  } from './lib/stores';
  import type { FontFamily, FontScale, NavRoute, ThemeMode } from './lib/types';
  import Dashboard from './views/Dashboard.svelte';
  import Drops from './views/Drops.svelte';
  import EpicGames from './views/EpicGames.svelte';
  import History from './views/History.svelte';
  import Logs from './views/Logs.svelte';
  import Platform from './views/Platform.svelte';
  import Settings from './views/Settings.svelte';

  let systemDark = $state(true);
  let dismissedBanner = $state<string | null>(null);

  const routes: NavRoute[] = [
    'dashboard',
    'drops',
    'history',
    'twitch',
    'kick',
    'epic',
    'logs',
    'settings',
  ];

  const allPaused = $derived(
    Boolean(
      $appSnapshot &&
        $appSnapshot.twitch.state === 'Paused' &&
        $appSnapshot.kick.state === 'Paused',
    ),
  );
  const nextCheckIso = $derived($appSnapshot ? earliestNextActionAt($appSnapshot) : null);

  function applyDomTheme(
    mode: ThemeMode,
    sysDark: boolean,
    fontFamily: FontFamily,
    fontScale: FontScale,
  ) {
    const resolved = mode === 'system' ? (sysDark ? 'dark' : 'light') : mode;
    document.documentElement.dataset.design = 'console';
    document.documentElement.dataset.mode = resolved;
    document.documentElement.dataset.theme = `console-${resolved}`;
    document.documentElement.dataset.font = fontFamily;
    document.documentElement.dataset.fontScale = fontScale;
  }

  $effect(() => {
    const mode = $appSettings?.theme ?? 'dark';
    const fontFamily = $appSettings?.fontFamily ?? 'plex';
    const fontScale = $appSettings?.fontScale ?? 'default';
    applyDomTheme(mode, systemDark, fontFamily, fontScale);
  });

  onMount(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    systemDark = media.matches;
    const onMediaChange = (e: MediaQueryListEvent) => {
      systemDark = e.matches;
    };
    media.addEventListener('change', onMediaChange);

    let cleanupEvents: (() => void) | null = null;
    void initializeAppStores().then((unlisten) => {
      cleanupEvents = unlisten;
    });

    return () => {
      media.removeEventListener('change', onMediaChange);
      if (cleanupEvents) cleanupEvents();
    };
  });

  async function handleAcceptDisclaimer() {
    try {
      const updated = await api.acceptDisclaimer();
      appSettings.set(updated);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleTogglePauseAll() {
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

  async function handleRefreshAll() {
    try {
      await Promise.all([api.retryPlatform('twitch'), api.retryPlatform('kick')]);
      notifyUi('Refreshed Twitch and Kick');
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  function handleGlobalKeydown(ev: KeyboardEvent) {
    const tag = (ev.target as HTMLElement | null)?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

    if (ev.key === '?' && !ev.ctrlKey && !ev.metaKey) {
      ev.preventDefault();
      shortcutsOpen.update((v) => !v);
      return;
    }

    if (!ev.ctrlKey) return;

    const num = Number(ev.key);
    if (num >= 1 && num <= routes.length) {
      ev.preventDefault();
      activeRoute.set(routes[num - 1]);
      return;
    }

    const lower = ev.key.toLowerCase();
    if (lower === 'r') {
      ev.preventDefault();
      if ($activeRoute === 'twitch' || $activeRoute === 'kick') {
        void api.retryPlatform($activeRoute);
      } else {
        void Promise.all([api.retryPlatform('twitch'), api.retryPlatform('kick')]);
      }
      notifyUi('Refreshed');
    } else if (lower === 'p') {
      ev.preventDefault();
      const snap = $appSnapshot;
      if (!snap) return;
      const paused = snap.twitch.state === 'Paused' && snap.kick.state === 'Paused';
      void Promise.all(
        paused
          ? [api.resume('twitch'), api.resume('kick')]
          : [api.pause('twitch'), api.pause('kick')],
      );
    }
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<div class="app-shell">
  <Sidebar
    active={$activeRoute}
    snapshot={$appSnapshot}
    onnavigate={(route) => activeRoute.set(route)}
  />

  <div class="workspace-col">
    {#if $appSnapshot}
      <header class="console-ticker" aria-label="Live status ticker">
        <div class="ticker-group">
          <button type="button" class="ticker-item" onclick={() => activeRoute.set('twitch')}>
            <span class="plat-dot dot-twitch" aria-hidden="true"></span>
            <span class="ticker-k">TWITCH</span>
            <span class="ticker-v">{$appSnapshot.twitch.state.toUpperCase()}</span>
            {#if $appSnapshot.twitch.watching}
              <span class="ticker-dim">· {$appSnapshot.twitch.watching.displayName}</span>
            {/if}
          </button>
          <span class="ticker-sep" aria-hidden="true">|</span>
          <button type="button" class="ticker-item" onclick={() => activeRoute.set('kick')}>
            <span class="plat-dot dot-kick" aria-hidden="true"></span>
            <span class="ticker-k">KICK</span>
            <span class="ticker-v">{$appSnapshot.kick.state.toUpperCase()}</span>
            {#if $appSnapshot.kick.watching}
              <span class="ticker-dim">· {$appSnapshot.kick.watching.displayName}</span>
            {/if}
          </button>
        </div>

        <div class="ticker-group ticker-right tabular">
          <span class="ticker-stat">SESSION <strong>{$sessionMinutes}m</strong></span>
          <span class="ticker-sep" aria-hidden="true">|</span>
          <span class="ticker-stat">SYNC <Countdown targetIso={nextCheckIso} /></span>
          <span class="ticker-sep" aria-hidden="true">|</span>
          <button type="button" class="ticker-btn" onclick={handleRefreshAll} title="Refresh all (Ctrl+R)">
            <RefreshCw size={11} />
            <span>SYNC</span>
          </button>
          <button type="button" class="ticker-btn" class:warn={allPaused} onclick={handleTogglePauseAll} title="Pause / Resume all (Ctrl+P)">
            {#if allPaused}<Play size={11} /><span>RESUME</span>{:else}<Pause size={11} /><span>PAUSE</span>{/if}
          </button>
          <button type="button" class="ticker-btn" onclick={() => void api.hideToTray().catch((e) => notifyUi(String(e), 'error'))} title="Hide to tray">
            <Minimize2 size={11} />
            <span>TRAY</span>
          </button>
        </div>
      </header>
    {/if}

    <main class="main-area">
      {#if $remoteConfigStatus?.maintenanceBanner && $remoteConfigStatus.maintenanceBanner !== dismissedBanner}
        <div class="maint-banner" role="status">
          <span>{$remoteConfigStatus.maintenanceBanner}</span>
          <button
            type="button"
            class="maint-dismiss"
            onclick={() => (dismissedBanner = $remoteConfigStatus?.maintenanceBanner ?? null)}
            aria-label="Dismiss notice"
          >
            Dismiss
          </button>
        </div>
      {/if}

      {#if $appSnapshot && $appSettings}
        {#key $activeRoute}
          <div class="content-wrap">
            {#if $activeRoute === 'dashboard'}
              <Dashboard snapshot={$appSnapshot} settings={$appSettings} logs={$appLogs} />
            {:else if $activeRoute === 'drops'}
              <Drops snapshot={$appSnapshot} settings={$appSettings} />
            {:else if $activeRoute === 'history'}
              <History />
            {:else if $activeRoute === 'twitch'}
              <Platform
                platform="twitch"
                snapshot={$appSnapshot.twitch}
                settings={$appSettings}
              />
            {:else if $activeRoute === 'kick'}
              <Platform
                platform="kick"
                snapshot={$appSnapshot.kick}
                settings={$appSettings}
              />
            {:else if $activeRoute === 'epic'}
              <EpicGames />
            {:else if $activeRoute === 'logs'}
              <Logs logs={$appLogs} />
            {:else if $activeRoute === 'settings'}
              <Settings
                settings={$appSettings}
                snapshot={$appSnapshot}
                mockAvailable={$mockFeatureAvailable}
              />
            {/if}
          </div>
        {/key}
      {:else}
        <div class="content-wrap">
          <Skeleton />
        </div>
      {/if}
    </main>
  </div>
</div>

<DisclaimerModal
  open={Boolean($appSettings && $appSettings.disclaimerAcceptedVersion < 1)}
  onAccept={handleAcceptDisclaimer}
/>

<Modal open={$shortcutsOpen} title="Keyboard Shortcuts" onclose={() => shortcutsOpen.set(false)}>
  <div class="sc-list">
    <div class="sc-row"><kbd class="mono">Ctrl + 1…8</kbd><span>Switch pages</span></div>
    <div class="sc-row"><kbd class="mono">Ctrl + R</kbd><span>Refresh active platform</span></div>
    <div class="sc-row"><kbd class="mono">Ctrl + P</kbd><span>Pause / Resume all</span></div>
    <div class="sc-row"><kbd class="mono">Alt + Up/Down</kbd><span>Reorder priority item</span></div>
    <div class="sc-row"><kbd class="mono">?</kbd><span>Toggle shortcuts</span></div>
  </div>
  {#snippet footer()}
    <Button variant="primary" size="sm" onclick={() => shortcutsOpen.set(false)}>Close</Button>
  {/snippet}
</Modal>

<UpdateBanner />
<Toast />
<ImageLightbox
  open={Boolean($lightboxImage)}
  src={$lightboxImage?.src ?? null}
  title={$lightboxImage?.title ?? ''}
  subtitle={$lightboxImage?.subtitle}
  onclose={closeLightbox}
/>

<style>
  .app-shell {
    display: flex;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
  }

  .workspace-col {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .main-area {
    flex: 1;
    overflow-y: auto;
    padding: var(--content-pad-y, 12px) var(--content-pad-x, 14px);
    background: var(--bg);
  }

  .content-wrap {
    max-width: var(--content-max);
    margin: 0 auto;
  }

  .console-ticker {
    height: 32px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 12px;
    background: var(--sidebar-bg);
    border-bottom: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    overflow-x: auto;
  }

  .ticker-group {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
  }

  .ticker-item {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: none;
    background: transparent;
    padding: 2px 4px;
    color: inherit;
    font: inherit;
    border-radius: 2px;
  }

  .ticker-item:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .ticker-k {
    color: var(--text-faint);
    font-weight: 500;
  }

  .ticker-v {
    color: var(--text);
    font-weight: 600;
  }

  .ticker-dim {
    color: var(--text-muted);
  }

  .ticker-sep {
    color: var(--border-hover);
  }

  .ticker-stat strong {
    color: var(--text);
    font-weight: 600;
  }

  .ticker-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 7px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
  }

  .ticker-btn:hover {
    border-color: var(--border-hover);
    color: var(--text);
    background: var(--surface-2);
  }

  .ticker-btn.warn {
    border-color: var(--warning);
    color: var(--warning-text);
    background: var(--warning-tint);
  }

  .maint-banner {
    max-width: var(--content-max);
    margin: 0 auto 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px;
    border-radius: var(--radius-md);
    background: var(--warning-tint);
    border: 1px solid var(--border);
    color: var(--warning-text);
    font-size: var(--text-sm);
  }

  .maint-dismiss {
    border: 1px solid var(--border);
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: var(--radius-xs);
  }

  .maint-dismiss:hover {
    background: var(--surface-2);
  }

  .sc-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .sc-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    font-size: var(--text-sm);
  }

  .sc-row kbd {
    padding: 2px 7px;
    border-radius: var(--radius-xs);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
</style>
