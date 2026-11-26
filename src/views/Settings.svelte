<script lang="ts">
  import { onMount } from 'svelte';
  import Bell from 'lucide-svelte/icons/bell';
  import Check from 'lucide-svelte/icons/check';
  import Cloud from 'lucide-svelte/icons/cloud';
  import Database from 'lucide-svelte/icons/database';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import FolderOpen from 'lucide-svelte/icons/folder-open';
  import Info from 'lucide-svelte/icons/info';
  import Link2 from 'lucide-svelte/icons/link-2';
  import LogIn from 'lucide-svelte/icons/log-in';
  import LogOut from 'lucide-svelte/icons/log-out';
  import Palette from 'lucide-svelte/icons/palette';
  import Pencil from 'lucide-svelte/icons/pencil';
  import Plus from 'lucide-svelte/icons/plus';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Send from 'lucide-svelte/icons/send';
  import Sliders from 'lucide-svelte/icons/sliders';
  import Trash2 from 'lucide-svelte/icons/trash-2';
  import Users from 'lucide-svelte/icons/users';
  import Zap from 'lucide-svelte/icons/zap';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import CustomSelect, { type SelectOption } from '../components/CustomSelect.svelte';
  import Modal from '../components/Modal.svelte';
  import Monogram from '../components/Monogram.svelte';
  import ProgressBar from '../components/ProgressBar.svelte';
  import StatusPill from '../components/StatusPill.svelte';
  import Toggle from '../components/Toggle.svelte';
  import { api } from '../lib/api';
  import { formatExactDateTime } from '../lib/format';
  import {
    appSettings,
    checkForAppUpdate,
    checkingForUpdate,
    editionInfo,
    installAppUpdate,
    installingUpdate,
    notifyUi,
    remoteConfigStatus,
    updateInfo,
  } from '../lib/stores';
  import type {
    AppSnapshot,
    FontFamily,
    FontScale,
    PlatformId,
    ProfileSnapshot,
    Settings,
    ThemeMode,
    WebhookSecretsStatus,
  } from '../lib/types';

  interface Props {
    settings: Settings;
    snapshot: AppSnapshot;
    mockAvailable: boolean;
  }

  let { settings, snapshot, mockAvailable }: Props = $props();

  type SettingsTab = 'general' | 'accounts' | 'farming' | 'alerts' | 'appearance' | 'data';

  let activeTab = $state<SettingsTab>('general');
  let syncingRemote = $state(false);
  let clearModalOpen = $state(false);

  let webhookSecrets = $state<WebhookSecretsStatus>({
    hasDiscordUrl: false,
  });
  let discordUrlInput = $state('');
  let savingWebhook = $state(false);
  let testingDiscord = $state(false);
  let webhookFeedback = $state<{ ok: boolean; text: string } | null>(null);
  let addingPlatform = $state<PlatformId | null>(null);
  let newProfileLabel = $state('');
  let savingProfile = $state(false);
  let renamingProfileId = $state<string | null>(null);
  let renameValue = $state('');

  const profileCards = $derived<ProfileSnapshot[]>(
    snapshot.profiles && snapshot.profiles.length > 0
      ? snapshot.profiles.filter((p) => $editionInfo.kickEnabled || p.platform !== 'kick')
      : [
          {
            profileId: settings.activeTwitchProfileId ?? 'default-twitch',
            label: 'Main',
            platform: 'twitch',
            snapshot: snapshot.twitch,
          },
          ...($editionInfo.kickEnabled
            ? [
                {
                  profileId: settings.activeKickProfileId ?? 'default-kick',
                  label: 'Main',
                  platform: 'kick' as const,
                  snapshot: snapshot.kick,
                },
              ]
            : []),
        ],
  );
  const twitchProfileCount = $derived(
    profileCards.filter((p) => p.platform === 'twitch').length,
  );
  const kickProfileCount = $derived(
    profileCards.filter((p) => p.platform === 'kick').length,
  );

  function isActiveProfileCard(card: ProfileSnapshot): boolean {
    return card.platform === 'twitch'
      ? card.profileId === (settings.activeTwitchProfileId ?? 'default-twitch')
      : card.profileId === (settings.activeKickProfileId ?? 'default-kick');
  }

  function isDefaultProfileId(profileId: string): boolean {
    return profileId === 'default-twitch' || profileId === 'default-kick';
  }

  async function handleAddProfile(ev: SubmitEvent) {
    ev.preventDefault();
    if (!addingPlatform || savingProfile) return;
    savingProfile = true;
    const plat = addingPlatform;
    try {
      const updated = await api.addAccountProfile(plat, newProfileLabel.trim());
      appSettings.set(updated);
      addingPlatform = null;
      newProfileLabel = '';
      notifyUi(`Added ${plat === 'twitch' ? 'Twitch' : 'Kick'} account`);
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      savingProfile = false;
    }
  }

  async function handleRemoveProfile(card: ProfileSnapshot) {
    try {
      const updated = await api.removeAccountProfile(card.profileId);
      appSettings.set(updated);
      notifyUi(`Removed ${card.platform === 'twitch' ? 'Twitch' : 'Kick'} account "${card.label}"`);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleSetActiveProfile(card: ProfileSnapshot) {
    try {
      const updated = await api.setActiveProfile(card.platform, card.profileId);
      appSettings.set(updated);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleRenameProfile(ev: SubmitEvent) {
    ev.preventDefault();
    if (!renamingProfileId) return;
    const clean = renameValue.trim();
    if (!clean) {
      notifyUi('Account label cannot be empty', 'error');
      return;
    }
    try {
      const updated = await api.renameAccountProfile(renamingProfileId, clean);
      appSettings.set(updated);
      renamingProfileId = null;
      renameValue = '';
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  const timeOptions: SelectOption[] = Array.from({ length: 48 }, (_, idx) => {
    const mins = idx * 30;
    const h = String(Math.floor(mins / 60)).padStart(2, '0');
    const m = String(mins % 60).padStart(2, '0');
    return {
      value: String(mins),
      label: `${h}:${m}`,
    };
  });

  onMount(() => {
    void api
      .getWebhookSecretsStatus()
      .then((st) => {
        webhookSecrets = st;
      })
      .catch(() => {});
  });

  const tabs = $derived<{ id: SettingsTab; label: string; hint: string }[]>([
    { id: 'general', label: 'General & Startup', hint: 'Tray, boot & logs' },
    {
      id: 'accounts',
      label: $editionInfo.multiAccountEnabled ? 'Accounts' : 'Account',
      hint: $editionInfo.multiAccountEnabled ? 'Isolated profiles' : 'Connected session',
    },
    {
      id: 'farming',
      label: $editionInfo.quietHoursEnabled ? 'Mining & Schedule' : 'Drop Farming',
      hint: $editionInfo.quietHoursEnabled ? 'Rules & quiet hours' : 'Rules & cadence',
    },
    ...($editionInfo.webhooksEnabled
      ? [{ id: 'alerts' as const, label: 'Discord Webhooks', hint: 'Alerts & triggers' }]
      : []),
    { id: 'appearance', label: 'Appearance', hint: 'Theme & typography' },
    { id: 'data', label: 'Data & Updates', hint: 'Storage, OTA & reset' },
  ]);

  $effect(() => {
    if (activeTab === 'alerts' && !$editionInfo.webhooksEnabled) {
      activeTab = 'general';
    }
  });

  const themeOptions: {
    id: ThemeMode;
    label: string;
    hint: string;
  }[] = [
    { id: 'dark', label: 'Dark', hint: 'Low-glare terminal surfaces' },
    { id: 'light', label: 'Light', hint: 'Crisp high-contrast surfaces' },
    { id: 'system', label: 'Match Windows', hint: 'Follows OS dark/light mode' },
  ];

  const fontOptions: {
    id: FontFamily;
    label: string;
    hint: string;
    sampleSans: string;
    sampleMono: string;
  }[] = [
    {
      id: 'plex',
      label: 'IBM Plex',
      hint: 'Default Console pairing (Sans + Mono)',
      sampleSans: '"IBM Plex Sans", "Segoe UI", sans-serif',
      sampleMono: '"IBM Plex Mono", Consolas, monospace',
    },
    {
      id: 'inter',
      label: 'Inter + JetBrains',
      hint: 'Clean geometric UI + sharp tabular mono',
      sampleSans: '"Inter", "Segoe UI", sans-serif',
      sampleMono: '"JetBrains Mono", Consolas, monospace',
    },
    {
      id: 'system',
      label: 'Windows System',
      hint: 'Native Segoe UI Variable + Cascadia Mono',
      sampleSans: '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
      sampleMono: '"Cascadia Mono", Consolas, monospace',
    },
    {
      id: 'mono',
      label: 'JetBrains Mono',
      hint: 'Full terminal monospace typography',
      sampleSans: '"JetBrains Mono", Consolas, monospace',
      sampleMono: '"JetBrains Mono", Consolas, monospace',
    },
  ];

  const fontScaleOptions: { id: FontScale; label: string; pct: string }[] = [
    { id: 'compact', label: 'Compact', pct: '95%' },
    { id: 'default', label: 'Default', pct: '100%' },
    { id: 'comfortable', label: 'Comfortable', pct: '108%' },
    { id: 'large', label: 'Large', pct: '115%' },
  ];

  const activeFontLabel = $derived(
    fontOptions.find((f) => f.id === (settings.fontFamily ?? 'plex'))?.label ?? 'IBM Plex',
  );
  const activeScaleLabel = $derived(
    fontScaleOptions.find((s) => s.id === (settings.fontScale ?? 'default'))?.pct ?? '100%',
  );

  async function savePatch(mutator: (draft: Settings) => void) {
    const draft: Settings = structuredClone(settings);
    mutator(draft);
    draft.refreshMinutes = Math.max(10, Math.min(240, Math.round(draft.refreshMinutes || 30)));
    try {
      const saved = await api.updateSettings(draft);
      appSettings.set(saved);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  function stepRefresh(delta: number) {
    const next = Math.max(10, Math.min(240, (settings.refreshMinutes || 30) + delta));
    void savePatch((d) => (d.refreshMinutes = next));
  }

  function handleCheckUpdates() {
    void checkForAppUpdate(false);
  }

  async function handleClearData() {
    try {
      await api.clearAppData();
      const fresh = await api.getSettings();
      appSettings.set(fresh);
      webhookSecrets = { hasDiscordUrl: false };
      discordUrlInput = '';
      clearModalOpen = false;
      notifyUi('Cleared Dropilot accounts and local WebView2 data');
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleRefreshRemoteConfig() {
    syncingRemote = true;
    try {
      const next = await api.refreshRemoteConfig();
      remoteConfigStatus.set(next);
      if (next.lastError) {
        notifyUi(`Remote config sync issue: ${next.lastError}`, 'error');
      } else {
        notifyUi(`Remote config synced (${next.overriddenHashes} active GQL hashes)`);
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      syncingRemote = false;
    }
  }

  async function handleSaveDiscordUrl(clear = false) {
    savingWebhook = true;
    try {
      const val = clear ? '' : discordUrlInput.trim();
      webhookSecrets = await api.saveWebhookSecrets(val);
      discordUrlInput = '';
      if (clear) {
        await savePatch((d) => (d.webhooks.discordEnabled = false));
        notifyUi('Discord webhook removed from Credential Manager');
      } else {
        await savePatch((d) => (d.webhooks.discordEnabled = true));
        notifyUi('Discord webhook saved securely in Credential Manager');
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      savingWebhook = false;
    }
  }

  async function handleTestDiscordWebhook() {
    testingDiscord = true;
    webhookFeedback = null;
    try {
      if (discordUrlInput.trim()) {
        webhookSecrets = await api.saveWebhookSecrets(discordUrlInput.trim());
        discordUrlInput = '';
        await savePatch((d) => (d.webhooks.discordEnabled = true));
      }
      const resultMsg = await api.testWebhook('discord');
      const okMsg = resultMsg || 'Test Discord notification sent';
      webhookFeedback = { ok: true, text: okMsg };
      notifyUi(okMsg);
    } catch (err) {
      const errMsg = String(err);
      webhookFeedback = { ok: false, text: errMsg };
      notifyUi(errMsg, 'error');
    } finally {
      testingDiscord = false;
    }
  }

  function snapToHalfHour(mins: number): string {
    const snapped = Math.floor((mins % 1440) / 30) * 30;
    return String(snapped);
  }
</script>

<div class="page settings-page">
  <header class="page-header">
    <div class="page-title-wrap">
      <div>
        <h1>Settings</h1>
        <p class="page-subtitle">
          {$editionInfo.multiAccountEnabled
            ? 'Configure startup, tray behavior, isolated accounts, farming rules, and appearance'
            : 'Configure startup, tray behavior, account session, farming rules, and appearance'}
        </p>
      </div>
    </div>
    <div class="page-actions">
      <Badge tone="neutral">v{__APP_VERSION__}</Badge>
    </div>
  </header>

  <div class="settings-shell">
    <!-- Category Navigation (left rail on desktop, horizontal strip on compact/half-screen) -->
    <div class="settings-nav" role="tablist" aria-label="Settings categories">
      {#each tabs as tab (tab.id)}
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === tab.id}
          class="nav-pill"
          class:active={activeTab === tab.id}
          onclick={() => (activeTab = tab.id)}
        >
          <span class="nav-pill-ico" aria-hidden="true">
            {#if tab.id === 'general'}
              <Sliders size={14} />
            {:else if tab.id === 'accounts'}
              <Users size={14} />
            {:else if tab.id === 'farming'}
              <Zap size={14} />
            {:else if tab.id === 'alerts'}
              <Bell size={14} />
            {:else if tab.id === 'appearance'}
              <Palette size={14} />
            {:else}
              <Database size={14} />
            {/if}
          </span>
          <span class="nav-pill-text">
            <span class="nav-pill-label">{tab.label}</span>
            <span class="nav-pill-hint">{tab.hint}</span>
          </span>
        </button>
      {/each}
    </div>

    <!-- Active Category Content -->
    <div class="settings-content" role="tabpanel">
      {#if activeTab === 'general'}
        <section class="settings-group">
          <div class="group-head">
            <span class="section-label">STARTUP &amp; WINDOW</span>
          </div>
          <div class="card group-card">
            <div class="setting-row">
              <div class="setting-meta">
                <div class="setting-title-line">
                  <strong>Start in system tray</strong>
                  <Badge tone={settings.startInTray ? 'accent' : 'neutral'}>
                    {settings.startInTray ? 'Tray first' : 'Window'}
                  </Badge>
                </div>
                <p>Keep the main window hidden on launch so Dropilot runs quietly in the tray with zero window flash.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.startInTray}
                  ariaLabel="Start in system tray"
                  onchange={(v) => savePatch((d) => (d.startInTray = v))}
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Close button minimizes to tray</strong>
                <p>Closes the UI WebView2 window while keeping lightweight Rust drop miners active in the tray.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.closeToTray}
                  ariaLabel="Close button minimizes to tray"
                  onchange={(v) => savePatch((d) => (d.closeToTray = v))}
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Start with Windows</strong>
                <p>Launches Dropilot automatically at Windows sign-in via standard HKCU Run entry.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.autostart}
                  ariaLabel="Start with Windows"
                  onchange={(v) => savePatch((d) => (d.autostart = v))}
                />
              </div>
            </div>

            <div class="setting-row" class:is-dimmed={settings.startInTray}>
              <div class="setting-meta">
                <strong>Start minimized on Windows boot</strong>
                <p>
                  {settings.startInTray
                    ? 'Already covered while "Start in system tray" is enabled.'
                    : 'Only hides the window when launched automatically at Windows sign-in.'}
                </p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.startMinimized}
                  disabled={settings.startInTray}
                  ariaLabel="Start minimized on Windows boot"
                  onchange={(v) => savePatch((d) => (d.startMinimized = v))}
                />
              </div>
            </div>
          </div>
        </section>

        <section class="settings-group">
          <div class="group-head">
            <span class="section-label">NOTIFICATIONS &amp; DIAGNOSTICS</span>
          </div>
          <div class="card group-card">
            <div class="setting-row">
              <div class="setting-meta">
                <strong>Desktop toast notifications</strong>
                <p>Notifies you when a drop is claimed, requires manual action, expires, or an Epic free game unlocks.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.notifications}
                  ariaLabel="Desktop notifications"
                  onchange={(v) => savePatch((d) => (d.notifications = v))}
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Diagnostic log verbosity</strong>
                <p>Keep on Info for normal use; switch to Debug when troubleshooting platform responses.</p>
              </div>
              <div class="setting-ctrl">
                <CustomSelect
                  options={[
                    {
                      value: 'info',
                      label: 'Info (Recommended)',
                      subtitle: 'Clean activity and claim summary',
                      dotTone: 'info',
                    },
                    {
                      value: 'debug',
                      label: 'Debug (Verbose)',
                      subtitle: 'Detailed diagnostic trace for troubleshooting',
                      dotTone: 'warn',
                    },
                  ] satisfies SelectOption[]}
                  value={settings.logLevel}
                  onchange={(val) => savePatch((d) => (d.logLevel = val as Settings['logLevel']))}
                  ariaLabel="Log level"
                  size="md"
                  minMenuWidth={240}
                  align="right"
                />
              </div>
            </div>

            {#if mockAvailable}
              <div class="setting-row">
                <div class="setting-meta">
                  <strong>Mock platform mode</strong>
                  <p>Simulates campaigns, live channels, progress, and claims offline with zero network calls.</p>
                </div>
                <div class="setting-ctrl">
                  <Toggle
                    checked={settings.mockMode}
                    ariaLabel="Mock platform mode"
                    onchange={(v) => savePatch((d) => (d.mockMode = v))}
                  />
                </div>
              </div>
            {/if}
          </div>
        </section>
      {:else if activeTab === 'accounts'}
        <section class="settings-group">
          <div class="group-head group-head-actions">
            <div class="group-head-left">
              <span class="section-label">
                {$editionInfo.multiAccountEnabled ? 'ISOLATED PROFILES' : 'CONNECTED ACCOUNT'}
              </span>
              {#if $editionInfo.multiAccountEnabled}
                <span class="group-count tabular">
                  {$editionInfo.kickEnabled
                    ? `${twitchProfileCount} Twitch · ${kickProfileCount} Kick`
                    : `${twitchProfileCount} Twitch`}
                </span>
              {/if}
            </div>
            {#if $editionInfo.multiAccountEnabled}
              <div class="inline-actions">
                <Button
                  variant="secondary"
                  size="sm"
                  onclick={() => {
                    addingPlatform = 'twitch';
                    newProfileLabel = '';
                  }}
                >
                  <Plus size={12} />
                  <span>Add Twitch</span>
                </Button>
                {#if $editionInfo.kickEnabled}
                  <Button
                    variant="secondary"
                    size="sm"
                    onclick={() => {
                      addingPlatform = 'kick';
                      newProfileLabel = '';
                    }}
                  >
                    <Plus size={12} />
                    <span>Add Kick</span>
                  </Button>
                {/if}
              </div>
            {/if}
          </div>

          {#if addingPlatform}
            <form class="card add-profile-bar" onsubmit={handleAddProfile}>
              <span class="plat-dot dot-{addingPlatform}" aria-hidden="true"></span>
              <span class="add-prof-title">
                New {addingPlatform === 'twitch' ? 'Twitch' : 'Kick'} account
              </span>
              <input
                type="text"
                class="secret-input add-prof-input"
                bind:value={newProfileLabel}
                placeholder="Label (e.g. Alt, Drops #2)"
                maxlength={24}
                onkeydown={(e) => e.key === 'Escape' && (addingPlatform = null)}
              />
              <Button type="submit" variant="primary" size="sm" disabled={savingProfile}>
                {savingProfile ? 'Adding...' : 'Create profile'}
              </Button>
              <Button type="button" variant="ghost" size="sm" onclick={() => (addingPlatform = null)}>
                Cancel
              </Button>
            </form>
          {/if}

          <div class="card group-card">
            {#each profileCards as card (card.profileId)}
              {@const platSnap = card.snapshot}
              {@const isTwitch = card.platform === 'twitch'}
              {@const label = isTwitch ? 'Twitch' : 'Kick'}
              {@const platCount = isTwitch ? twitchProfileCount : kickProfileCount}
              {@const isActive = isActiveProfileCard(card)}
              {@const isDefault = isDefaultProfileId(card.profileId)}
              {@const browserUrl = isTwitch
                ? 'https://www.twitch.tv/drops/campaigns'
                : 'https://kick.com/drops/campaigns'}
              <div class="acct-row" class:acct-row-active={platCount > 1 && isActive}>
                <div class="acct-left">
                  <Monogram
                    src={platSnap.account?.avatarUrl}
                    name={platSnap.account?.displayName ?? `${label} ${card.label}`}
                    width={34}
                    height={34}
                  />
                  <div class="acct-meta">
                    <div class="acct-top">
                      <span class="plat-dot dot-{card.platform}" aria-hidden="true"></span>
                      <strong>{label}</strong>
                      {#if $editionInfo.multiAccountEnabled}
                        {#if renamingProfileId === card.profileId}
                          <form class="rename-inline" onsubmit={handleRenameProfile}>
                            <input
                              type="text"
                              class="rename-input"
                              bind:value={renameValue}
                              maxlength={24}
                              aria-label="Account label"
                              onkeydown={(e) => e.key === 'Escape' && (renamingProfileId = null)}
                            />
                            <Button type="submit" variant="primary" size="sm">Save</Button>
                          </form>
                        {:else}
                          <Badge tone="neutral">{card.label}</Badge>
                          <button
                            type="button"
                            class="rename-btn"
                            title="Rename profile"
                            aria-label={`Rename ${card.label}`}
                            onclick={() => {
                              renamingProfileId = card.profileId;
                              renameValue = card.label;
                            }}
                          >
                            <Pencil size={11} />
                          </button>
                        {/if}
                      {/if}
                      {#if platCount > 1 && isActive}
                        <Badge tone="accent">Active</Badge>
                      {/if}
                      <StatusPill state={platSnap.state} compact />
                    </div>
                    <span class="acct-sub">
                      {#if platSnap.account}
                        Signed in as <strong>@{platSnap.account.displayName}</strong>
                      {:else}
                        Not connected — sign in with your {label} account
                      {/if}
                    </span>
                    {#if $editionInfo.multiAccountEnabled}
                      <span class="acct-iso tabular">
                        {isDefault
                          ? 'Isolated WebView2 · Default session'
                          : `Isolated WebView2 · WebView2-Profiles/${card.profileId}`}
                      </span>
                    {/if}
                  </div>
                </div>

                <div class="acct-action">
                  {#if platCount > 1 && !isActive}
                    <Button
                      variant="ghost"
                      size="sm"
                      title={`Switch active ${label} view to ${card.label}`}
                      onclick={() => handleSetActiveProfile(card)}
                    >
                      Use
                    </Button>
                  {/if}
                  <Button
                    variant="ghost"
                    size="sm"
                    title={`Open ${label} Drops in your default browser`}
                    onclick={() => api.openUrl(browserUrl).catch((e) => notifyUi(String(e), 'error'))}
                  >
                    <ExternalLink size={13} />
                    <span>Browser</span>
                  </Button>
                  {#if platSnap.account}
                    <Button
                      variant="secondary"
                      size="sm"
                      onclick={() => api.logout(card.platform, card.profileId)}
                    >
                      <LogOut size={13} />
                      <span>Log out</span>
                    </Button>
                  {:else}
                    <Button
                      variant="primary"
                      size="sm"
                      title={`Opens Dropilot's isolated ${label} sign-in window (${card.label})`}
                      onclick={() => api.login(card.platform, card.profileId)}
                    >
                      <LogIn size={13} />
                      <span>Log in</span>
                    </Button>
                  {/if}
                  {#if $editionInfo.multiAccountEnabled}
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={platCount <= 1}
                      ariaLabel={`Remove ${label} account ${card.label}`}
                      title={platCount <= 1
                        ? `At least one ${label} account must remain configured`
                        : `Remove ${label} account "${card.label}" and delete its isolated session`}
                      onclick={() => handleRemoveProfile(card)}
                    >
                      <Trash2 size={13} />
                      <span>Remove</span>
                    </Button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>

          {#if $editionInfo.multiAccountEnabled}
            <div class="session-info-bar">
              <Info size={13} />
              <span>
                <strong>Isolated session sign-in:</strong> Dropilot uses dedicated WebView2 folders under <code>WebView2-Profiles</code> and separate Windows Credential Manager entries instead of reading Chrome, Edge, or Firefox profiles, so multiple Twitch &amp; Kick accounts can farm simultaneously.
              </span>
            </div>
          {/if}
        </section>
      {:else if activeTab === 'farming'}
        <section class="settings-group">
          <div class="group-head">
            <span class="section-label">DROP FARMING RULES</span>
          </div>
          <div class="card group-card">
            <div class="setting-row">
              <div class="setting-meta">
                <strong>Auto-claim completed drops</strong>
                <p>
                  Automatically claims finished {$editionInfo.kickEnabled ? 'Twitch & Kick' : 'Twitch'} rewards as soon as watch time reaches 100%.
                </p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.autoClaim}
                  ariaLabel="Auto-claim completed drops"
                  onchange={(v) => savePatch((d) => (d.autoClaim = v))}
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Farm unlinked campaigns</strong>
                <p>Keep off to skip games whose publisher account isn't linked yet during automatic idle fallback.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.farmUnlinked}
                  ariaLabel="Farm even if game account is not linked"
                  onchange={(v) => savePatch((d) => (d.farmUnlinked = v))}
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>When nothing in priority list is live</strong>
                <p>Choose whether to stay idle or automatically farm any eligible active campaign while waiting.</p>
              </div>
              <div class="setting-ctrl">
                <CustomSelect
                  options={[
                    {
                      value: 'stay_idle',
                      label: 'Stay idle (default)',
                      subtitle: 'Wait quietly until a priority drop goes live',
                      dotTone: 'muted',
                    },
                    {
                      value: 'farm_any',
                      label: 'Farm any available campaign',
                      subtitle: 'Auto-mine other active drops while waiting',
                      dotTone: 'live',
                    },
                  ] satisfies SelectOption[]}
                  value={settings.idleBehavior}
                  onchange={(val) =>
                    savePatch((d) => (d.idleBehavior = val as Settings['idleBehavior']))}
                  ariaLabel="Idle fallback behavior"
                  size="md"
                  minMenuWidth={260}
                  align="right"
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Campaign refresh cadence</strong>
                <p>How often Dropilot reloads the full drops catalog (10–240 min). Watch progress syncs every 60s.</p>
              </div>
              <div class="setting-ctrl">
                <div class="stepper tabular">
                  <button type="button" class="step-btn" onclick={() => stepRefresh(-5)} aria-label="Decrease refresh minutes">−</button>
                  <span class="step-val">{settings.refreshMinutes}m</span>
                  <button type="button" class="step-btn" onclick={() => stepRefresh(5)} aria-label="Increase refresh minutes">+</button>
                </div>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Browser player fallback</strong>
                <p>Ultra-light stream-less mode is active by default (0 Mbps video, ~12 MB RAM in tray). Enable only if a platform stops crediting drops.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.compatWatchMode}
                  ariaLabel="Compatibility watch mode"
                  onchange={(v) => savePatch((d) => (d.compatWatchMode = v))}
                />
              </div>
            </div>
          </div>
        </section>

        {#if $editionInfo.quietHoursEnabled}
          <section class="settings-group">
            <div class="group-head">
              <span class="section-label">SCHEDULE &amp; TIMING</span>
            </div>
            <div class="card group-card">
              <div class="setting-row setting-row-col">
                <div class="setting-row-top">
                  <div class="setting-meta">
                    <strong>Quiet hours schedule</strong>
                    <p>Automatically pause drop farming during local hours and resume when the window ends.</p>
                  </div>
                  <div class="setting-ctrl">
                    <Toggle
                      checked={settings.schedule.quietHoursEnabled}
                      ariaLabel="Enable quiet hours schedule"
                      onchange={(v) => savePatch((d) => (d.schedule.quietHoursEnabled = v))}
                    />
                  </div>
                </div>

                {#if settings.schedule.quietHoursEnabled}
                  <div class="schedule-row">
                    <div class="sched-field">
                      <span class="sched-label">Pause at</span>
                      <CustomSelect
                        options={timeOptions}
                        value={snapToHalfHour(settings.schedule.quietStartMinutes)}
                        onchange={(val) =>
                          savePatch((d) => (d.schedule.quietStartMinutes = Number(val)))}
                        ariaLabel="Quiet hours start time"
                        size="sm"
                      />
                    </div>
                    <div class="sched-field">
                      <span class="sched-label">Resume at</span>
                      <CustomSelect
                        options={timeOptions}
                        value={snapToHalfHour(settings.schedule.quietEndMinutes)}
                        onchange={(val) =>
                          savePatch((d) => (d.schedule.quietEndMinutes = Number(val)))}
                        ariaLabel="Quiet hours end time"
                        size="sm"
                      />
                    </div>
                    {#if settings.schedule.quietStartMinutes > settings.schedule.quietEndMinutes}
                      <Badge tone="accent">Overnight window</Badge>
                    {:else if settings.schedule.quietStartMinutes === settings.schedule.quietEndMinutes}
                      <Badge tone="warning">Start &amp; end match (inactive)</Badge>
                    {/if}
                  </div>
                {/if}
              </div>

              <div class="setting-row">
                <div class="setting-meta">
                  <strong>Humanized polling jitter (±25%)</strong>
                  <p>Randomizes watch ticks and background sync intervals instead of rigid 60-second pulses.</p>
                </div>
                <div class="setting-ctrl">
                  <Toggle
                    checked={settings.schedule.humanizedJitter}
                    ariaLabel="Humanized polling jitter"
                    onchange={(v) => savePatch((d) => (d.schedule.humanizedJitter = v))}
                  />
                </div>
              </div>
            </div>
          </section>
        {/if}
      {:else if activeTab === 'alerts'}
        <section class="settings-group">
          <div class="group-head group-head-actions">
            <span class="section-label">DISCORD WEBHOOK</span>
            <Badge tone="neutral">Encrypted in Credential Manager</Badge>
          </div>
          <div class="card group-card">
            <div class="setting-row setting-row-col">
              <div class="setting-row-top">
                <div class="setting-meta">
                  <div class="setting-title-line">
                    <strong>Enable Discord webhook alerts</strong>
                    {#if webhookSecrets.hasDiscordUrl}
                      <Badge tone="success">Saved</Badge>
                    {:else}
                      <Badge tone="neutral">Not set</Badge>
                    {/if}
                  </div>
                  <p>Receive rich Discord embeds when drops are claimed, sessions expire, or Epic free games unlock.</p>
                </div>
                <div class="setting-ctrl">
                  <Toggle
                    checked={settings.webhooks.discordEnabled}
                    ariaLabel="Enable Discord webhook alerts"
                    onchange={(v) => savePatch((d) => (d.webhooks.discordEnabled = v))}
                  />
                </div>
              </div>

              <div class="webhook-input-row">
                <input
                  type="password"
                  class="secret-input"
                  bind:value={discordUrlInput}
                  placeholder={webhookSecrets.hasDiscordUrl
                    ? 'Webhook URL stored securely (paste new URL to replace)'
                    : 'https://discord.com/api/webhooks/...'}
                  aria-label="Discord webhook URL"
                />
                <Button
                  variant="primary"
                  size="sm"
                  disabled={savingWebhook || !discordUrlInput.trim()}
                  onclick={() => handleSaveDiscordUrl(false)}
                >
                  Save
                </Button>
                {#if webhookSecrets.hasDiscordUrl || discordUrlInput.trim()}
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={testingDiscord || savingWebhook}
                    onclick={handleTestDiscordWebhook}
                  >
                    <Send size={12} />
                    <span>{testingDiscord ? 'Sending...' : 'Test'}</span>
                  </Button>
                {/if}
                {#if webhookSecrets.hasDiscordUrl}
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={savingWebhook}
                    onclick={() => handleSaveDiscordUrl(true)}
                  >
                    Remove
                  </Button>
                {/if}
              </div>

              {#if webhookFeedback}
                {@const urlMatch = webhookFeedback.text.match(/https:\/\/discord\.com\/channels\/\d+\/\d+\/\d+/)}
                <div class="webhook-feedback-row">
                  <p class="webhook-feedback" class:fb-ok={webhookFeedback.ok} class:fb-err={!webhookFeedback.ok}>
                    {urlMatch ? webhookFeedback.text.replace(/\s*·\s*https:\/\/discord\.com\/channels\/\d+\/\d+\/\d+/, '') : webhookFeedback.text}
                  </p>
                  {#if urlMatch}
                    <button
                      type="button"
                      class="fb-open-link"
                      onclick={() => api.openUrl(urlMatch[0]).catch((e) => notifyUi(String(e), 'error'))}
                    >
                      Open in Discord
                    </button>
                  {/if}
                </div>
              {/if}
            </div>
          </div>
        </section>

        <section class="settings-group">
          <div class="group-head">
            <span class="section-label">WEBHOOK EVENT TRIGGERS</span>
          </div>
          <div class="card group-card">
            <div class="setting-row">
              <div class="setting-meta">
                <strong>Drop Claimed</strong>
                <p>Ping Discord whenever Twitch or Kick claims a completed drop reward.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.webhooks.onDropClaimed}
                  ariaLabel="Notify on drop claimed"
                  onchange={(v) => savePatch((d) => (d.webhooks.onDropClaimed = v))}
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Action Required / Session Expired</strong>
                <p>Alert when manual claim or account re-authentication is needed.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.webhooks.onActionRequired}
                  ariaLabel="Notify on action required"
                  onchange={(v) => savePatch((d) => (d.webhooks.onActionRequired = v))}
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Campaign Completed</strong>
                <p>Send a summary notification when every drop in a campaign finishes.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.webhooks.onCampaignCompleted}
                  ariaLabel="Notify on campaign completed"
                  onchange={(v) => savePatch((d) => (d.webhooks.onCampaignCompleted = v))}
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Epic Free Game Live</strong>
                <p>Ping when weekly free games unlock on the Epic Games Store.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.webhooks.onEpicFreeGames}
                  ariaLabel="Notify when Epic free games go live"
                  onchange={(v) => savePatch((d) => (d.webhooks.onEpicFreeGames = v))}
                />
              </div>
            </div>
          </div>
        </section>
      {:else if activeTab === 'appearance'}
        <section class="settings-group">
          <div class="group-head">
            <span class="section-label">COLOR MODE</span>
          </div>
          <div class="card studio-card">
            <div class="mode-grid" role="radiogroup" aria-label="Color mode">
              {#each themeOptions as opt (opt.id)}
                {@const active = settings.theme === opt.id}
                <button
                  type="button"
                  role="radio"
                  aria-checked={active}
                  class="mode-btn"
                  class:active
                  onclick={() => savePatch((d) => (d.theme = opt.id))}
                >
                  <div class="mode-text">
                    <strong>{opt.label}</strong>
                    <span>{opt.hint}</span>
                  </div>
                  {#if active}
                    <span class="mode-check" aria-hidden="true"><Check size={14} /></span>
                  {/if}
                </button>
              {/each}
            </div>
          </div>
        </section>

        <section class="settings-group">
          <div class="group-head">
            <span class="section-label">TYPOGRAPHY &amp; TEXT SCALE</span>
          </div>
          <div class="card studio-card">
            <div class="font-grid" role="radiogroup" aria-label="Font family preset">
              {#each fontOptions as fopt (fopt.id)}
                {@const active = (settings.fontFamily ?? 'plex') === fopt.id}
                <button
                  type="button"
                  role="radio"
                  aria-checked={active}
                  class="font-btn"
                  class:active
                  onclick={() => savePatch((d) => (d.fontFamily = fopt.id))}
                >
                  <div class="font-btn-top">
                    <strong>{fopt.label}</strong>
                    {#if active}
                      <span class="mode-check" aria-hidden="true"><Check size={14} /></span>
                    {/if}
                  </div>
                  <span class="font-hint">{fopt.hint}</span>
                  <div class="font-sample-row">
                    <span class="sample-sans" style:font-family={fopt.sampleSans}>Rust Drops</span>
                    <span class="sample-mono" style:font-family={fopt.sampleMono}>01:45 · 63%</span>
                  </div>
                </button>
              {/each}
            </div>

            <div class="scale-row">
              <div class="scale-meta">
                <strong>Text size</strong>
                <span>Scales UI text, timers, and card metadata cleanly across resolutions.</span>
              </div>
              <div class="scale-pills" role="radiogroup" aria-label="Text size scale">
                {#each fontScaleOptions as sopt (sopt.id)}
                  {@const active = (settings.fontScale ?? 'default') === sopt.id}
                  <button
                    type="button"
                    role="radio"
                    aria-checked={active}
                    class="scale-pill"
                    class:active
                    onclick={() => savePatch((d) => (d.fontScale = sopt.id))}
                  >
                    <span>{sopt.label}</span>
                    <span class="scale-pct tabular">{sopt.pct}</span>
                  </button>
                {/each}
              </div>
            </div>
          </div>
        </section>

        <section class="settings-group" aria-label="Typography live preview">
          <div class="group-head group-head-actions">
            <span class="section-label">LIVE PREVIEW</span>
            <Badge tone="accent">{activeFontLabel} · {activeScaleLabel}</Badge>
          </div>
          <div class="card studio-card">
            <div class="preview-surface">
              <div class="pv-top">
                <div class="pv-plat">
                  <span class="plat-dot dot-twitch" aria-hidden="true"></span>
                  <span class="pv-plat-name tabular">TWITCH</span>
                  <span class="pv-acct tabular">@demo_pilot</span>
                </div>
                <span class="pv-status tabular">MINING</span>
              </div>

              <div class="pv-camp">
                <div class="pv-camp-head">
                  <strong>Rust — Twitch Rivals Drops</strong>
                  <span class="pv-link">
                    <Link2 size={11} />
                    <span>Linked</span>
                  </span>
                </div>
                <span class="pv-reward">Reward: Cobalt Garage Door</span>
              </div>

              <div class="pv-prog">
                <div class="pv-prog-row tabular">
                  <span>19m / 30m</span>
                  <strong>63% · 11m 00s left</strong>
                </div>
                <ProgressBar value={19} max={30} platform="twitch" tall label="Preview progress" />
              </div>

              <div class="pv-log mono">
                <span class="pv-log-ts">[10:15:42]</span>
                <span class="pv-log-lvl">INFO</span>
                <span>Credited +1m watch time for Rust</span>
              </div>
            </div>
          </div>
        </section>
      {:else}
        <section class="settings-group">
          <div class="group-head">
            <span class="section-label">UPDATES &amp; HOTFIXES</span>
          </div>
          <div class="card group-card">
            <div class="setting-row">
              <div class="setting-meta">
                <strong>Signed auto-updater</strong>
                <p>Checks cryptographically verified releases on startup and every 4 hours.</p>
              </div>
              <div class="setting-ctrl inline-actions">
                {#if $updateInfo?.available}
                  {#if $updateInfo.signedUpdateReady}
                    <Button
                      variant="primary"
                      size="sm"
                      disabled={$installingUpdate}
                      onclick={() => void installAppUpdate()}
                    >
                      {$installingUpdate ? 'Updating...' : `Install v${$updateInfo.latestVersion}`}
                    </Button>
                  {:else}
                    <Button variant="primary" size="sm" onclick={() => api.openUrl($updateInfo!.releaseUrl)}>
                      v{$updateInfo.latestVersion}
                    </Button>
                  {/if}
                {/if}
                <Button variant="secondary" size="sm" disabled={$checkingForUpdate} onclick={handleCheckUpdates}>
                  <RefreshCw size={12} />
                  <span>{$checkingForUpdate ? 'Checking...' : 'Check'}</span>
                </Button>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <strong>Check for updates automatically</strong>
                <p>Periodically verifies if a newer signed Dropilot release is available.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.updateCheck}
                  ariaLabel="Check for signed updates"
                  onchange={(v) => savePatch((d) => (d.updateCheck = v))}
                />
              </div>
            </div>

            <div class="setting-row" class:is-dimmed={!settings.updateCheck}>
              <div class="setting-meta">
                <strong>Auto-install signed updates</strong>
                <p>Downloads and applies verified updates seamlessly without manual installer steps.</p>
              </div>
              <div class="setting-ctrl">
                <Toggle
                  checked={settings.autoInstallUpdates}
                  disabled={!settings.updateCheck}
                  ariaLabel="Auto-install signed updates"
                  onchange={(v) => savePatch((d) => (d.autoInstallUpdates = v))}
                />
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-meta">
                <div class="setting-title-line">
                  <strong>Remote hotfix config</strong>
                  <Badge
                    tone={$remoteConfigStatus?.source === 'remote'
                      ? 'success'
                      : $remoteConfigStatus?.source === 'cache'
                        ? 'accent'
                        : 'neutral'}
                  >
                    {$remoteConfigStatus?.source ?? 'bundled'}
                  </Badge>
                  {#if $remoteConfigStatus && $remoteConfigStatus.overriddenHashes > 0}
                    <Badge tone="neutral">{$remoteConfigStatus.overriddenHashes} GQL hashes</Badge>
                  {/if}
                </div>
                <p>
                  Syncs verified Twitch GQL query hashes so Dropilot recovers from platform rotations automatically.
                  {#if $remoteConfigStatus?.fetchedAt}
                    Last synced {formatExactDateTime($remoteConfigStatus.fetchedAt)}.
                  {/if}
                </p>
              </div>
              <div class="setting-ctrl inline-actions">
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={syncingRemote || !settings.remoteConfigEnabled}
                  onclick={handleRefreshRemoteConfig}
                >
                  <Cloud size={12} />
                  <span>{syncingRemote ? 'Syncing...' : 'Sync now'}</span>
                </Button>
                <Toggle
                  checked={settings.remoteConfigEnabled}
                  ariaLabel="Enable remote hotfix config"
                  onchange={(v) => savePatch((d) => (d.remoteConfigEnabled = v))}
                />
              </div>
            </div>
          </div>
        </section>

        <section class="settings-group">
          <div class="group-head">
            <span class="section-label">STORAGE &amp; ABOUT</span>
          </div>
          <div class="card group-card">
            <div class="setting-row">
              <div class="setting-meta">
                <strong>Local app folders</strong>
                <p>Inspect saved settings (<code>%APPDATA%\Dropilot</code>) or redacted rolling logs (<code>%LOCALAPPDATA%\Dropilot\logs</code>).</p>
              </div>
              <div class="setting-ctrl inline-actions">
                <Button variant="secondary" size="sm" onclick={() => api.openDataFolder().catch((e) => notifyUi(String(e), 'error'))}>
                  <FolderOpen size={12} />
                  <span>Data</span>
                </Button>
                <Button variant="secondary" size="sm" onclick={() => api.openLogFolder().catch((e) => notifyUi(String(e), 'error'))}>
                  <FolderOpen size={12} />
                  <span>Logs</span>
                </Button>
              </div>
            </div>

            <div class="setting-row setting-row-col">
              <div class="setting-row-top">
                <div class="setting-meta">
                  <strong>Dropilot v{__APP_VERSION__}</strong>
                  <p>Standard user privileges (<code>asInvoker</code>)</p>
                </div>
                <div class="setting-ctrl inline-actions">
                  <Button variant="secondary" size="sm" onclick={() => api.openUrl('https://github.com/theflash1211/dropilot-releases#readme')}>
                    README
                  </Button>
                  <Button variant="secondary" size="sm" onclick={() => api.openUrl('https://github.com/theflash1211/dropilot-releases/blob/main/PRIVACY.md')}>
                    Privacy
                  </Button>
                  <Button variant="secondary" size="sm" onclick={() => api.openUrl('https://github.com/theflash1211/dropilot-releases/blob/main/ENDPOINTS.md')}>
                    Endpoints
                  </Button>
                </div>
              </div>
              <p class="about-note">
                <strong>Disclaimer:</strong> Using automation tools may violate {$editionInfo.kickEnabled ? 'Twitch or Kick' : 'Twitch'} Terms of Service. You are solely responsible for your accounts. Dropilot is not affiliated with, sponsored by, or endorsed by {$editionInfo.kickEnabled ? 'Twitch Interactive, Inc. or Kick Streaming Pty Ltd.' : 'Twitch Interactive, Inc. or Epic Games, Inc.'}
              </p>
            </div>
          </div>
        </section>

        <section class="settings-group">
          <div class="group-head">
            <span class="section-label danger-label">DANGER ZONE</span>
          </div>
          <div class="card danger-card">
            <div class="setting-row">
              <div class="setting-meta">
                <strong class="danger-title">Reset &amp; clear all Dropilot data</strong>
                <p>Deletes Credential Manager tokens, Dropilot's isolated WebView2 cookies, and resets preferences to defaults.</p>
              </div>
              <div class="setting-ctrl">
                <Button variant="danger" size="sm" onclick={() => (clearModalOpen = true)}>
                  <Trash2 size={13} />
                  <span>Clear app data</span>
                </Button>
              </div>
            </div>
          </div>
        </section>
      {/if}
    </div>
  </div>
</div>

<Modal open={clearModalOpen} title="Clear all Dropilot data?" onclose={() => (clearModalOpen = false)}>
  <p>
    This signs out of Twitch and Kick, deletes stored tokens from Windows Credential Manager, clears Dropilot's isolated WebView2 cookies, and resets settings to defaults.
  </p>
  <p style="margin-top: 8px; color: var(--text-muted); font-size: 13px;">
    Your personal web browsers (Chrome, Edge, Firefox) are never touched.
  </p>
  {#snippet footer()}
    <Button variant="secondary" size="sm" onclick={() => (clearModalOpen = false)}>Cancel</Button>
    <Button variant="danger" size="sm" onclick={handleClearData}>Clear app data</Button>
  {/snippet}
</Modal>

<style>
  .settings-shell {
    display: grid;
    grid-template-columns: 210px minmax(0, 1fr);
    gap: clamp(12px, 1.5vw, 20px);
    align-items: start;
  }

  .settings-nav {
    position: sticky;
    top: 4px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 6px;
    border-radius: var(--radius-md);
    background: var(--surface);
    border: 1px solid var(--border);
  }

  .nav-pill {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    text-align: left;
    min-width: 0;
  }

  .nav-pill:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .nav-pill.active {
    background: var(--surface-2);
    border-color: var(--border-hover);
    color: var(--text);
  }

  .nav-pill-ico {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-xs);
    background: var(--bg);
    border: 1px solid var(--border);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .nav-pill.active .nav-pill-ico {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-on);
  }

  .nav-pill-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .nav-pill-label {
    font-size: var(--text-sm);
    font-weight: 600;
    line-height: 1.2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .nav-pill-hint {
    font-size: 10.5px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  @media (max-width: 860px) {
    .settings-shell {
      grid-template-columns: 1fr;
    }

    .settings-nav {
      position: static;
      flex-direction: row;
      flex-wrap: wrap;
      gap: 4px;
      padding: 5px;
    }

    .nav-pill {
      flex: 1 1 auto;
      padding: 7px 10px;
    }

    .nav-pill-hint {
      display: none;
    }
  }

  .settings-content {
    display: flex;
    flex-direction: column;
    gap: 18px;
    min-width: 0;
    max-width: 920px;
  }

  .settings-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .group-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 2px;
  }

  .group-head-left {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .group-count {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
  }

  .group-card {
    padding: 0;
    overflow: hidden;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
  }

  .setting-row:last-child {
    border-bottom: none;
  }

  .setting-row-col {
    flex-direction: column;
    align-items: stretch;
    gap: 10px;
  }

  .setting-row-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .setting-row.is-dimmed {
    opacity: 0.65;
  }

  @media (max-width: 600px) {
    .setting-row,
    .setting-row-top {
      flex-wrap: wrap;
      gap: 10px;
    }

    .setting-ctrl {
      margin-left: auto;
    }
  }

  .setting-meta {
    flex: 1;
    min-width: 200px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .setting-title-line {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }

  .setting-meta strong {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--text);
  }

  .setting-meta p {
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.45;
    max-width: 66ch;
  }

  .setting-ctrl {
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }

  .inline-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  /* Accounts */
  .add-profile-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
  }

  .add-prof-title {
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .add-prof-input {
    max-width: 220px;
  }

  .acct-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
  }

  .acct-row:last-child {
    border-bottom: none;
  }

  .acct-row-active {
    background: var(--accent-tint);
  }

  .acct-left {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    flex: 1 1 260px;
  }

  .acct-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .acct-top {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }

  .acct-sub {
    font-size: 12px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .acct-sub strong {
    color: var(--text);
  }

  .acct-iso {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .acct-action {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    flex-shrink: 0;
  }

  .rename-btn {
    border: none;
    background: transparent;
    color: var(--text-faint);
    padding: 2px;
    border-radius: 2px;
    display: inline-flex;
    align-items: center;
  }

  .rename-btn:hover {
    color: var(--text);
    background: var(--surface-2);
  }

  .rename-inline {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .rename-input {
    height: 22px;
    width: 110px;
    padding: 0 6px;
    border-radius: var(--radius-xs);
    border: 1px solid var(--accent);
    background: var(--bg);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .session-info-bar {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 9px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.45;
  }

  .session-info-bar :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--accent-text);
  }

  .session-info-bar strong {
    color: var(--text);
  }

  /* Stepper & Schedule */
  .stepper {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    overflow: hidden;
  }

  .step-btn {
    width: 28px;
    height: 28px;
    border: none;
    background: transparent;
    color: var(--text);
    font-weight: 600;
  }

  .step-btn:hover {
    background: var(--surface-hover);
  }

  .step-val {
    min-width: 44px;
    text-align: center;
    font-size: 12px;
    font-weight: 600;
  }

  .schedule-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .sched-field {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .sched-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }

  /* Webhooks */
  .webhook-input-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .secret-input {
    flex: 1;
    min-width: 160px;
    background: var(--surface-2);
  }

  .webhook-feedback-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .webhook-feedback {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.4;
    word-break: break-word;
  }

  .webhook-feedback.fb-ok {
    color: var(--success-text);
  }

  .webhook-feedback.fb-err {
    color: var(--error-text);
  }

  .fb-open-link {
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--accent-text);
    border-radius: var(--radius-xs);
    padding: 2px 8px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 600;
    white-space: nowrap;
  }

  .fb-open-link:hover {
    border-color: var(--accent);
  }

  /* Appearance */
  .studio-card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px;
  }

  .mode-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }

  @media (max-width: 640px) {
    .mode-grid {
      grid-template-columns: 1fr;
    }
  }

  .mode-btn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
    text-align: left;
  }

  .mode-btn:hover {
    border-color: var(--border-hover);
  }

  .mode-btn.active {
    border-color: var(--accent);
    background: var(--surface);
  }

  .mode-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .mode-text strong {
    font-size: var(--text-sm);
    color: var(--text);
  }

  .mode-text span {
    font-size: 11px;
    color: var(--text-muted);
  }

  .mode-check {
    color: var(--accent-text);
    display: inline-flex;
    flex-shrink: 0;
  }

  .font-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  @media (max-width: 600px) {
    .font-grid {
      grid-template-columns: 1fr;
    }
  }

  .font-btn {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
    text-align: left;
  }

  .font-btn:hover {
    border-color: var(--border-hover);
  }

  .font-btn.active {
    border-color: var(--accent);
    background: var(--surface);
  }

  .font-btn-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .font-btn-top strong {
    font-size: var(--text-sm);
    color: var(--text);
  }

  .font-hint {
    font-size: 11px;
    color: var(--text-muted);
  }

  .font-sample-row {
    margin-top: 3px;
    padding: 5px 8px;
    border-radius: var(--radius-xs);
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .sample-sans {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
  }

  .sample-mono {
    font-size: 11px;
    color: var(--accent-text);
  }

  .scale-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }

  .scale-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .scale-meta strong {
    font-size: var(--text-sm);
    color: var(--text);
  }

  .scale-meta span {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .scale-pills {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 3px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .scale-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .scale-pill:hover {
    color: var(--text);
  }

  .scale-pill.active {
    background: var(--accent-tint);
    border-color: var(--accent);
    color: var(--accent-text);
  }

  .scale-pct {
    font-size: 10.5px;
    opacity: 0.85;
  }

  .preview-surface {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-radius: var(--radius-sm);
    background: var(--bg);
    border: 1px solid var(--border);
  }

  .pv-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .pv-plat {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .pv-plat-name {
    font-size: var(--text-xs);
    font-weight: 700;
    color: var(--text);
  }

  .pv-acct {
    font-size: var(--text-xs);
    color: var(--text-faint);
  }

  .pv-status {
    padding: 2px 7px;
    border-radius: var(--radius-pill);
    background: var(--success-tint);
    color: var(--success-text);
    font-size: 10px;
    font-weight: 600;
  }

  .pv-camp {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .pv-camp-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .pv-camp-head strong {
    font-family: var(--font-display);
    font-size: var(--text-md);
    color: var(--text);
  }

  .pv-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    background: var(--success-tint);
    color: var(--success-text);
    font-size: 10.5px;
    font-weight: 600;
  }

  .pv-reward {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }

  .pv-prog {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .pv-prog-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .pv-prog-row strong {
    color: var(--text);
  }

  .pv-log {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 7px;
    padding: 6px 8px;
    border-radius: var(--radius-xs);
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 3px solid var(--info);
    font-size: var(--text-xs);
    color: var(--text);
  }

  .pv-log-ts {
    color: var(--text-faint);
  }

  .pv-log-lvl {
    color: var(--info-text);
    font-weight: 600;
  }

  /* About & Danger */
  .about-note {
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }

  .danger-label {
    color: var(--error-text);
  }

  .danger-card {
    padding: 0;
    border-color: var(--error);
    overflow: hidden;
  }

  .danger-title {
    color: var(--error-text) !important;
  }
</style>
