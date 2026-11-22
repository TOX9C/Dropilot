<script lang="ts">
  import { onMount } from 'svelte';
  import DisclaimerModal from './components/DisclaimerModal.svelte';
  import Modal from './components/Modal.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Skeleton from './components/Skeleton.svelte';
  import Toast from './components/Toast.svelte';
  import Button from './components/Button.svelte';
  import { api } from './lib/api';
  import {
    accentPreset,
    activeRoute,
    appLogs,
    appSettings,
    appSnapshot,
    initializeAppStores,
    mockFeatureAvailable,
    notifyUi,
    remoteConfigStatus,
    shortcutsOpen,
  } from './lib/stores';
  import type { AccentPreset, NavRoute, ThemeMode } from './lib/types';
  import Dashboard from './views/Dashboard.svelte';
  import Drops from './views/Drops.svelte';
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
    'logs',
    'settings',
  ];

  function applyDomTheme(mode: ThemeMode, sysDark: boolean, accent: AccentPreset) {
    const resolved = mode === 'system' ? (sysDark ? 'dark' : 'light') : mode;
    if (resolved === 'light') {
      document.documentElement.dataset.theme = 'light';
    } else {
      delete document.documentElement.dataset.theme;
    }
    if (accent === 'cyan') {
      delete document.documentElement.dataset.accent;
    } else {
      document.documentElement.dataset.accent = accent;
    }
  }

  $effect(() => {
    const mode = $appSettings?.theme ?? 'dark';
    applyDomTheme(mode, systemDark, $accentPreset);
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
      const allPaused = snap.twitch.state === 'Paused' && snap.kick.state === 'Paused';
      void Promise.all(
        allPaused
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
          ✕
        </button>
      </div>
    {/if}

    {#if $appSnapshot && $appSettings}
      {#key $activeRoute}
        <div class="content-wrap page-enter">
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

<DisclaimerModal
  open={Boolean($appSettings && $appSettings.disclaimerAcceptedVersion < 1)}
  onAccept={handleAcceptDisclaimer}
/>

<Modal open={$shortcutsOpen} title="Keyboard Shortcuts" onclose={() => shortcutsOpen.set(false)}>
  <div class="sc-list">
    <div class="sc-row"><kbd class="mono">Ctrl + 1…7</kbd><span>Switch pages</span></div>
    <div class="sc-row"><kbd class="mono">Ctrl + R</kbd><span>Refresh active platform</span></div>
    <div class="sc-row"><kbd class="mono">Ctrl + P</kbd><span>Pause / Resume all</span></div>
    <div class="sc-row"><kbd class="mono">Alt + Up/Down</kbd><span>Reorder priority item</span></div>
    <div class="sc-row"><kbd class="mono">?</kbd><span>Toggle shortcuts</span></div>
  </div>
  {#snippet footer()}
    <Button variant="primary" size="sm" onclick={() => shortcutsOpen.set(false)}>Close</Button>
  {/snippet}
</Modal>

<Toast />

<style>
  .app-shell {
    display: flex;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
  }

  .main-area {
    flex: 1;
    overflow-y: auto;
    padding: 18px 20px;
    background-image: var(--bg-mesh);
    background-attachment: local;
  }

  .content-wrap {
    max-width: var(--content-max);
    margin: 0 auto;
  }

  .maint-banner {
    max-width: var(--content-max);
    margin: 0 auto 12px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    border-radius: var(--radius-md);
    background: var(--warning-tint);
    border: 1px solid rgba(245, 158, 11, 0.35);
    color: var(--warning-text);
    font-size: var(--text-sm);
  }

  .maint-dismiss {
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 13px;
    padding: 2px 6px;
    border-radius: 6px;
  }

  .maint-dismiss:hover {
    background: rgba(245, 158, 11, 0.2);
  }

  .sc-list { display: flex; flex-direction: column; gap: 6px; }
  .sc-row { display: flex; justify-content: space-between; gap: 12px; font-size: var(--text-sm); }
  .sc-row kbd { padding: 2px 6px; border-radius: 4px; background: var(--surface-2); border: 1px solid var(--border); }

  @media (prefers-reduced-motion: no-preference) {
    .page-enter {
      animation: slide-fade var(--duration-base) var(--ease-out);
    }

    @keyframes slide-fade {
      from {
        opacity: 0;
        transform: translateY(6px);
      }
      to {
        opacity: 1;
        transform: translateY(0);
      }
    }
  }
</style>
