<script lang="ts">
  import { onMount } from 'svelte';
  import Bell from 'lucide-svelte/icons/bell';
  import Check from 'lucide-svelte/icons/check';
  import Cloud from 'lucide-svelte/icons/cloud';
  import FolderOpen from 'lucide-svelte/icons/folder-open';
  import Info from 'lucide-svelte/icons/info';
  import LogIn from 'lucide-svelte/icons/log-in';
  import LogOut from 'lucide-svelte/icons/log-out';
  import Monitor from 'lucide-svelte/icons/monitor';
  import Moon from 'lucide-svelte/icons/moon';
  import Palette from 'lucide-svelte/icons/palette';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Send from 'lucide-svelte/icons/send';
  import SettingsIcon from 'lucide-svelte/icons/settings';
  import Shield from 'lucide-svelte/icons/shield';
  import Sliders from 'lucide-svelte/icons/sliders';
  import Trash2 from 'lucide-svelte/icons/trash-2';
  import Zap from 'lucide-svelte/icons/zap';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import CustomSelect, { type SelectOption } from '../components/CustomSelect.svelte';
  import Modal from '../components/Modal.svelte';
  import Monogram from '../components/Monogram.svelte';
  import StatusPill from '../components/StatusPill.svelte';
  import Toggle from '../components/Toggle.svelte';
  import { api } from '../lib/api';
  import { formatExactDateTime } from '../lib/format';
  import { accentPreset, appSettings, notifyUi, remoteConfigStatus } from '../lib/stores';
  import type {
    AccentPreset,
    AppSnapshot,
    Settings,
    ThemeMode,
    UpdateStatus,
    WebhookSecretsStatus,
  } from '../lib/types';

  interface Props {
    settings: Settings;
    snapshot: AppSnapshot;
    mockAvailable: boolean;
  }

  let { settings, snapshot, mockAvailable }: Props = $props();

  type SettingsTab = 'appearance' | 'farming' | 'system' | 'data';

  let activeTab = $state<SettingsTab>('appearance');
  let updateStatus = $state<UpdateStatus | null>(null);
  let checkingUpdate = $state(false);
  let syncingRemote = $state(false);
  let clearModalOpen = $state(false);

  let webhookSecrets = $state<WebhookSecretsStatus>({
    hasDiscordUrl: false,
    hasTelegramToken: false,
  });
  let discordUrlInput = $state('');
  let telegramTokenInput = $state('');
  let telegramChatIdInput = $state('');
  let savingWebhook = $state(false);
  let testingChannel = $state<'discord' | 'telegram' | null>(null);

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
    telegramChatIdInput = settings.webhooks?.telegramChatId ?? '';
    void api
      .getWebhookSecretsStatus()
      .then((st) => {
        webhookSecrets = st;
      })
      .catch(() => {});
  });

  $effect(() => {
    const cur = settings.webhooks?.telegramChatId ?? '';
    if (cur && !telegramChatIdInput) {
      telegramChatIdInput = cur;
    }
  });

  const tabs: { id: SettingsTab; label: string; desc: string }[] = [
    { id: 'appearance', label: 'Appearance', desc: 'Theme & colors' },
    { id: 'farming', label: 'Farming', desc: 'Drop rules & sync' },
    { id: 'system', label: 'System', desc: 'Startup, tray & alerts' },
    { id: 'data', label: 'Data & About', desc: 'Folders, updates & reset' },
  ];

  const themeOptions: {
    id: ThemeMode;
    label: string;
    hint: string;
    bg: string;
    rail: string;
    card: string;
    bar: string;
  }[] = [
    {
      id: 'dark',
      label: 'Obsidian Dark',
      hint: 'Deep carbon surfaces',
      bg: '#080a0f',
      rail: '#0c1018',
      card: '#111622',
      bar: '#06b6d4',
    },
    {
      id: 'light',
      label: 'Clean Light',
      hint: 'Crisp studio alabaster',
      bg: '#f3f6fa',
      rail: '#ffffff',
      card: '#ffffff',
      bar: '#0891b2',
    },
    {
      id: 'system',
      label: 'Match Windows',
      hint: 'Auto light / dark',
      bg: 'linear-gradient(135deg, #080a0f 50%, #f3f6fa 50%)',
      rail: '#171f30',
      card: '#1e2940',
      bar: '#22d3ee',
    },
  ];

  const accentOptions: {
    id: AccentPreset;
    label: string;
    tag: string;
    grad: string;
    ring: string;
  }[] = [
    {
      id: 'cyan',
      label: 'Cyber Cyan',
      tag: 'Default',
      grad: 'linear-gradient(135deg, #06b6d4, #14b8a6)',
      ring: '#22d3ee',
    },
    {
      id: 'emerald',
      label: 'Emerald Forge',
      tag: 'Fresh',
      grad: 'linear-gradient(135deg, #059669, #22c55e)',
      ring: '#34d399',
    },
    {
      id: 'amber',
      label: 'Solar Ember',
      tag: 'Warm',
      grad: 'linear-gradient(135deg, #ea580c, #fbbf24)',
      ring: '#fbbf24',
    },
    {
      id: 'crimson',
      label: 'Crimson Pulse',
      tag: 'Bold',
      grad: 'linear-gradient(135deg, #e11d48, #fb923c)',
      ring: '#fb7185',
    },
    {
      id: 'violet',
      label: 'Royal Violet',
      tag: 'Classic',
      grad: 'linear-gradient(135deg, #6366f1, #ec4899)',
      ring: '#a78bfa',
    },
  ];

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

  async function handleCheckUpdates() {
    checkingUpdate = true;
    try {
      updateStatus = await api.checkUpdate();
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      checkingUpdate = false;
    }
  }

  async function handleClearData() {
    try {
      await api.clearAppData();
      const fresh = await api.getSettings();
      appSettings.set(fresh);
      webhookSecrets = { hasDiscordUrl: false, hasTelegramToken: false };
      discordUrlInput = '';
      telegramTokenInput = '';
      telegramChatIdInput = '';
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
      webhookSecrets = await api.saveWebhookSecrets(val, null);
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

  async function handleSaveTelegram(clear = false) {
    savingWebhook = true;
    try {
      if (clear) {
        webhookSecrets = await api.saveWebhookSecrets(null, '');
        telegramTokenInput = '';
        telegramChatIdInput = '';
        await savePatch((d) => {
          d.webhooks.telegramEnabled = false;
          d.webhooks.telegramChatId = '';
        });
        notifyUi('Telegram credentials removed from Credential Manager');
      } else {
        if (telegramTokenInput.trim()) {
          webhookSecrets = await api.saveWebhookSecrets(null, telegramTokenInput.trim());
          telegramTokenInput = '';
        }
        await savePatch((d) => {
          d.webhooks.telegramChatId = telegramChatIdInput.trim();
          if (webhookSecrets.hasTelegramToken && telegramChatIdInput.trim()) {
            d.webhooks.telegramEnabled = true;
          }
        });
        notifyUi('Telegram alert settings saved');
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      savingWebhook = false;
    }
  }

  async function handleTestWebhook(channel: 'discord' | 'telegram') {
    testingChannel = channel;
    try {
      await api.testWebhook(
        channel,
        channel === 'telegram' ? telegramChatIdInput.trim() : undefined,
      );
      notifyUi(`Test ${channel === 'discord' ? 'Discord' : 'Telegram'} notification sent!`);
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      testingChannel = null;
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
      <div class="page-icon"><SettingsIcon size={18} /></div>
      <div>
        <h1>Settings</h1>
        <p class="page-subtitle">Personalize theme colors, connected accounts, farming rules, and system behavior</p>
      </div>
    </div>
    <div class="page-actions">
      <Badge tone="accent">v0.1.0</Badge>
    </div>
  </header>

  <!-- Connected Accounts Top Strip -->
  <section class="accounts-grid" aria-label="Connected accounts">
    {#each [snapshot.twitch, snapshot.kick] as platSnap (platSnap.platform)}
      {@const isTwitch = platSnap.platform === 'twitch'}
      {@const label = isTwitch ? 'Twitch' : 'Kick'}
      <div class="card acct-card stripe-{platSnap.platform} wash-{platSnap.platform}">
        <div class="acct-left">
          <Monogram
            src={platSnap.account?.avatarUrl}
            name={platSnap.account?.displayName ?? label}
            width={40}
            height={40}
            rounded
          />
          <div class="acct-meta">
            <div class="acct-top">
              <span class="plat-dot dot-{platSnap.platform}" aria-hidden="true"></span>
              <strong>{label}</strong>
              <StatusPill state={platSnap.state} />
            </div>
            <span class="acct-sub">
              {#if platSnap.account}
                Signed in as <strong>@{platSnap.account.displayName}</strong>
              {:else}
                Not connected — sign in to sync &amp; farm drops
              {/if}
            </span>
          </div>
        </div>
        <div class="acct-action">
          {#if platSnap.account}
            <Button variant="secondary" size="sm" onclick={() => api.logout(platSnap.platform)}>
              <LogOut size={13} />
              <span>Log out</span>
            </Button>
          {:else}
            <Button variant={platSnap.platform} size="sm" onclick={() => api.login(platSnap.platform)}>
              <LogIn size={13} />
              <span>Log in</span>
            </Button>
          {/if}
        </div>
      </div>
    {/each}
  </section>

  <!-- Modern Segmented Tab Bar -->
  <div class="seg-tabs" role="tablist" aria-label="Settings categories">
    {#each tabs as tab (tab.id)}
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === tab.id}
        class="seg-tab"
        class:active={activeTab === tab.id}
        onclick={() => (activeTab = tab.id)}
      >
        <span class="seg-ico" aria-hidden="true">
          {#if tab.id === 'appearance'}
            <Palette size={15} />
          {:else if tab.id === 'farming'}
            <Zap size={15} />
          {:else if tab.id === 'system'}
            <Sliders size={15} />
          {:else}
            <Shield size={15} />
          {/if}
        </span>
        <span class="seg-text">
          <span class="seg-label">{tab.label}</span>
          <span class="seg-desc">{tab.desc}</span>
        </span>
      </button>
    {/each}
  </div>

  <!-- Tab Panel: Appearance -->
  {#if activeTab === 'appearance'}
    <div class="panel-stack">
      <section class="card studio-card">
        <div class="studio-head">
          <div>
            <h2>Color Accent Studio</h2>
            <p class="studio-sub">Switch the app accent color instantly across navigation, progress bars, and highlights.</p>
          </div>
          <Badge tone="accent">Instant preview</Badge>
        </div>

        <div class="accent-grid" role="radiogroup" aria-label="Accent color preset">
          {#each accentOptions as opt (opt.id)}
            {@const selected = $accentPreset === opt.id}
            <button
              type="button"
              role="radio"
              aria-checked={selected}
              class="accent-tile"
              class:selected={selected}
              onclick={() => accentPreset.set(opt.id)}
            >
              <span class="accent-orb" style="background:{opt.grad}">
                {#if selected}
                  <Check size={14} />
                {/if}
              </span>
              <span class="accent-meta">
                <span class="accent-name">{opt.label}</span>
                <span class="accent-tag">{opt.tag}</span>
              </span>
            </button>
          {/each}
        </div>
      </section>

      <section class="card studio-card">
        <div class="studio-head">
          <div>
            <h2>Theme Mode</h2>
            <p class="studio-sub">Matches the native Windows title bar and system surfaces automatically.</p>
          </div>
        </div>

        <div class="theme-grid" role="radiogroup" aria-label="Theme mode">
          {#each themeOptions as opt (opt.id)}
            {@const active = settings.theme === opt.id}
            <button
              type="button"
              role="radio"
              aria-checked={active}
              class="theme-card"
              class:active={active}
              onclick={() => savePatch((d) => (d.theme = opt.id))}
            >
              <div class="theme-preview" style="background:{opt.bg}">
                <div class="tp-rail" style="background:{opt.rail}"></div>
                <div class="tp-body">
                  <div class="tp-bar" style="background:{opt.bar}"></div>
                  <div class="tp-box" style="background:{opt.card}"></div>
                </div>
              </div>
              <div class="theme-foot">
                <div>
                  <div class="theme-name">{opt.label}</div>
                  <div class="theme-hint">{opt.hint}</div>
                </div>
                {#if active}
                  <span class="theme-check" aria-hidden="true"><Check size={13} /></span>
                {/if}
              </div>
            </button>
          {/each}
        </div>
      </section>
    </div>
  {:else if activeTab === 'farming'}
    <div class="pref-grid">
      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico ico-accent"><Zap size={16} /></div>
          <Toggle
            checked={settings.autoClaim}
            ariaLabel="Auto-claim completed drops"
            onchange={(v) => savePatch((d) => (d.autoClaim = v))}
          />
        </div>
        <h3>Auto-claim completed drops</h3>
        <p>Automatically claims finished Twitch &amp; Kick rewards as soon as watch time reaches 100%.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico ico-warn"><Shield size={16} /></div>
          <Toggle
            checked={settings.farmUnlinked}
            ariaLabel="Farm even if game account is not linked"
            onchange={(v) => savePatch((d) => (d.farmUnlinked = v))}
          />
        </div>
        <h3>Farm unlinked campaigns</h3>
        <p>Keep off to skip games whose publisher account isn't linked yet during automatic idle fallback.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico ico-info"><Monitor size={16} /></div>
          <Toggle
            checked={settings.compatWatchMode}
            ariaLabel="Compatibility watch mode"
            onchange={(v) => savePatch((d) => (d.compatWatchMode = v))}
          />
        </div>
        <h3>Browser player fallback (off by default)</h3>
        <p>Ultra-light stream-less mode is active by default (0 Mbps video, ~12 MB RAM in tray). Turn on only if a platform stops crediting drops.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico ico-success"><RefreshCw size={16} /></div>
          <div class="stepper tabular">
            <button type="button" class="step-btn" onclick={() => stepRefresh(-5)} aria-label="Decrease refresh minutes">−</button>
            <span class="step-val">{settings.refreshMinutes}m</span>
            <button type="button" class="step-btn" onclick={() => stepRefresh(5)} aria-label="Increase refresh minutes">+</button>
          </div>
        </div>
        <h3>Campaign refresh cadence</h3>
        <p>How often Dropilot reloads the full drops catalog (10–240 min). Progress itself syncs every 60s.</p>
      </div>

      <div class="card pref-card pref-wide">
        <div class="pref-wide-row">
          <div class="pref-wide-text">
            <h3>When nothing in priority list is live</h3>
            <p>Choose whether to stay idle or automatically farm any eligible active campaign while waiting.</p>
          </div>
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
            minMenuWidth={270}
            align="right"
          />
        </div>
      </div>

      <div class="card pref-card pref-wide">
        <div class="pref-wide-row">
          <div class="pref-wide-text">
            <div class="title-with-badge">
              <span class="pref-ico ico-info"><Moon size={15} /></span>
              <div>
                <h3>Quiet hours schedule</h3>
                <p>Automatically pause drop farming during local hours (e.g. while gaming or working) and resume when the window ends.</p>
              </div>
            </div>
          </div>
          <Toggle
            checked={settings.schedule.quietHoursEnabled}
            ariaLabel="Enable quiet hours schedule"
            onchange={(v) => savePatch((d) => (d.schedule.quietHoursEnabled = v))}
          />
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

        <div class="sub-divider"></div>

        <div class="pref-wide-row">
          <div class="pref-wide-text">
            <h3>Humanized polling jitter (±25%)</h3>
            <p>Randomizes watch ticks and background sync intervals instead of rigid 60-second pulses for safer, natural network patterns.</p>
          </div>
          <Toggle
            checked={settings.schedule.humanizedJitter}
            ariaLabel="Humanized polling jitter"
            onchange={(v) => savePatch((d) => (d.schedule.humanizedJitter = v))}
          />
        </div>
      </div>
    </div>
  {:else if activeTab === 'system'}
    <div class="pref-grid">
      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico ico-accent"><Monitor size={16} /></div>
          <Toggle
            checked={settings.autostart}
            ariaLabel="Start with Windows"
            onchange={(v) => savePatch((d) => (d.autostart = v))}
          />
        </div>
        <h3>Start with Windows</h3>
        <p>Launches Dropilot automatically at Windows sign-in via standard HKCU Run entry.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico ico-info"><Sliders size={16} /></div>
          <Toggle
            checked={settings.startMinimized}
            ariaLabel="Start minimized to tray"
            onchange={(v) => savePatch((d) => (d.startMinimized = v))}
          />
        </div>
        <h3>Start minimized to tray</h3>
        <p>Starts quietly in the Windows notification tray when launched automatically on boot.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico ico-success"><Sliders size={16} /></div>
          <Toggle
            checked={settings.closeToTray}
            ariaLabel="Close button minimizes to tray"
            onchange={(v) => savePatch((d) => (d.closeToTray = v))}
          />
        </div>
        <h3>Close to system tray</h3>
        <p>Closes the UI window while keeping lightweight Rust drop miners running in the tray.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico ico-warn"><Bell size={16} /></div>
          <Toggle
            checked={settings.notifications}
            ariaLabel="Desktop notifications"
            onchange={(v) => savePatch((d) => (d.notifications = v))}
          />
        </div>
        <h3>Desktop toast notifications</h3>
        <p>Notifies you when a skin or reward case is claimed, needs manual action, or a session expires.</p>
      </div>

      <div class="card pref-card pref-wide">
        <div class="pref-wide-row">
          <div class="pref-wide-text">
            <h3>Diagnostic log verbosity</h3>
            <p>Keep on Info for normal use; switch to Debug when troubleshooting platform responses.</p>
          </div>
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
            minMenuWidth={250}
            align="right"
          />
        </div>
      </div>

      <div class="card pref-card pref-wide webhook-card">
        <div class="studio-head">
          <div>
            <h3>Discord &amp; Telegram Webhook Alerts</h3>
            <p>Get pinged instantly on your phone when drops are claimed or when Twitch requests manual verification. Secrets stay encrypted in Windows Credential Manager.</p>
          </div>
          <Badge tone="accent">Encrypted</Badge>
        </div>

        <div class="webhook-grid">
          <div class="webhook-box">
            <div class="webhook-box-head">
              <div class="webhook-title-row">
                <strong>Discord Webhook</strong>
                {#if webhookSecrets.hasDiscordUrl}
                  <Badge tone="success">Saved</Badge>
                {:else}
                  <Badge tone="neutral">Not set</Badge>
                {/if}
              </div>
              <Toggle
                checked={settings.webhooks.discordEnabled}
                ariaLabel="Enable Discord webhook alerts"
                onchange={(v) => savePatch((d) => (d.webhooks.discordEnabled = v))}
              />
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
              {#if webhookSecrets.hasDiscordUrl}
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={testingChannel === 'discord'}
                  onclick={() => handleTestWebhook('discord')}
                >
                  <Send size={12} />
                  <span>{testingChannel === 'discord' ? 'Sending...' : 'Test'}</span>
                </Button>
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
          </div>

          <div class="webhook-box">
            <div class="webhook-box-head">
              <div class="webhook-title-row">
                <strong>Telegram Bot</strong>
                {#if webhookSecrets.hasTelegramToken}
                  <Badge tone="success">Token saved</Badge>
                {:else}
                  <Badge tone="neutral">Not set</Badge>
                {/if}
              </div>
              <Toggle
                checked={settings.webhooks.telegramEnabled}
                ariaLabel="Enable Telegram bot alerts"
                onchange={(v) => savePatch((d) => (d.webhooks.telegramEnabled = v))}
              />
            </div>
            <div class="webhook-input-row">
              <input
                type="password"
                class="secret-input"
                bind:value={telegramTokenInput}
                placeholder={webhookSecrets.hasTelegramToken
                  ? 'Bot token stored securely (paste to replace)'
                  : 'Bot token from @BotFather (123456:ABC...)'}
                aria-label="Telegram bot token"
              />
              <input
                type="text"
                class="secret-input chat-id-input"
                bind:value={telegramChatIdInput}
                placeholder="Chat ID"
                aria-label="Telegram chat ID"
              />
              <Button
                variant="primary"
                size="sm"
                disabled={savingWebhook || (!telegramTokenInput.trim() && !telegramChatIdInput.trim())}
                onclick={() => handleSaveTelegram(false)}
              >
                Save
              </Button>
              {#if webhookSecrets.hasTelegramToken}
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={testingChannel === 'telegram'}
                  onclick={() => handleTestWebhook('telegram')}
                >
                  <Send size={12} />
                  <span>{testingChannel === 'telegram' ? 'Sending...' : 'Test'}</span>
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={savingWebhook}
                  onclick={() => handleSaveTelegram(true)}
                >
                  Remove
                </Button>
              {/if}
            </div>
          </div>
        </div>

        <div class="event-toggles">
          <label class="ev-chip">
            <span>Drop Claimed</span>
            <Toggle
              checked={settings.webhooks.onDropClaimed}
              ariaLabel="Notify on drop claimed"
              onchange={(v) => savePatch((d) => (d.webhooks.onDropClaimed = v))}
            />
          </label>
          <label class="ev-chip">
            <span>Action Required / Session Expired</span>
            <Toggle
              checked={settings.webhooks.onActionRequired}
              ariaLabel="Notify on action required"
              onchange={(v) => savePatch((d) => (d.webhooks.onActionRequired = v))}
            />
          </label>
          <label class="ev-chip">
            <span>Campaign Completed</span>
            <Toggle
              checked={settings.webhooks.onCampaignCompleted}
              ariaLabel="Notify on campaign completed"
              onchange={(v) => savePatch((d) => (d.webhooks.onCampaignCompleted = v))}
            />
          </label>
        </div>
      </div>

      {#if mockAvailable}
        <div class="card pref-card pref-wide">
          <div class="pref-wide-row">
            <div class="pref-wide-text">
              <h3>Mock platform mode</h3>
              <p>Simulates campaigns, live channels, progress, and claims offline with zero network calls.</p>
            </div>
            <Toggle
              checked={settings.mockMode}
              ariaLabel="Mock platform mode"
              onchange={(v) => savePatch((d) => (d.mockMode = v))}
            />
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <div class="panel-stack">
      <div class="pref-grid">
        <div class="card pref-card">
          <div class="pref-top">
            <div class="pref-ico ico-accent"><FolderOpen size={16} /></div>
            <div class="inline-actions">
              <Button variant="secondary" size="sm" onclick={() => api.openDataFolder().catch((e) => notifyUi(String(e), 'error'))}>
                Data ↗
              </Button>
              <Button variant="secondary" size="sm" onclick={() => api.openLogFolder().catch((e) => notifyUi(String(e), 'error'))}>
                Logs ↗
              </Button>
            </div>
          </div>
          <h3>Local app folders</h3>
          <p>Inspect saved settings (`%APPDATA%\Dropilot`) or redacted rolling logs (`%LOCALAPPDATA%\Dropilot\logs`).</p>
        </div>

        <div class="card pref-card">
          <div class="pref-top">
            <div class="pref-ico ico-info"><RefreshCw size={16} /></div>
            <div class="inline-actions">
              {#if updateStatus?.available}
                <Button variant="primary" size="sm" onclick={() => api.openUrl(updateStatus!.releaseUrl)}>
                  v{updateStatus.latestVersion} ↗
                </Button>
              {/if}
              <Button variant="secondary" size="sm" disabled={checkingUpdate} onclick={handleCheckUpdates}>
                {checkingUpdate ? 'Checking...' : 'Check'}
              </Button>
              <Toggle
                checked={settings.updateCheck}
                ariaLabel="Check for updates on GitHub"
                onchange={(v) => savePatch((d) => (d.updateCheck = v))}
              />
            </div>
          </div>
          <h3>GitHub update checks</h3>
          <p>Optional check against GitHub Releases. Only opens the release page — never auto-downloads binaries.</p>
        </div>

        <div class="card pref-card pref-wide">
          <div class="pref-wide-row">
            <div class="pref-wide-text">
              <div class="title-with-badge">
                <span class="pref-ico ico-success"><Cloud size={15} /></span>
                <div>
                  <div class="webhook-title-row">
                    <h3>Remote hotfix config</h3>
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
                      <Badge tone="accent">{$remoteConfigStatus.overriddenHashes} GQL hashes</Badge>
                    {/if}
                  </div>
                  <p>
                    Syncs verified Twitch GQL query hashes so Dropilot recovers from platform rotations without waiting for a new installer.
                    {#if $remoteConfigStatus?.fetchedAt}
                      Last synced {formatExactDateTime($remoteConfigStatus.fetchedAt)}.
                    {/if}
                  </p>
                </div>
              </div>
            </div>
            <div class="inline-actions">
              <Button
                variant="secondary"
                size="sm"
                disabled={syncingRemote || !settings.remoteConfigEnabled}
                onclick={handleRefreshRemoteConfig}
              >
                <RefreshCw size={13} />
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
      </div>

      <section class="card about-card">
        <div class="about-head">
          <div class="pref-ico ico-accent"><Info size={16} /></div>
          <div>
            <h2>About Dropilot</h2>
            <p class="studio-sub">Version 0.1.0 · MIT License · Standard user privileges (`asInvoker`)</p>
          </div>
        </div>
        <p class="about-note">
          <strong>Disclaimer:</strong> Using automation tools may violate Twitch or Kick Terms of Service. You are solely responsible for your accounts. Dropilot is not affiliated with, sponsored by, or endorsed by Twitch Interactive, Inc. or Kick Streaming Pty Ltd.
        </p>
        <div class="about-links">
          <Button variant="secondary" size="sm" onclick={() => api.openUrl('https://github.com/theflash1211/dropilot#readme')}>
            README ↗
          </Button>
          <Button variant="secondary" size="sm" onclick={() => api.openUrl('https://github.com/theflash1211/dropilot/blob/main/docs/PRIVACY.md')}>
            Privacy ↗
          </Button>
          <Button variant="secondary" size="sm" onclick={() => api.openUrl('https://github.com/theflash1211/dropilot/blob/main/docs/ENDPOINTS.md')}>
            Endpoints ↗
          </Button>
        </div>
      </section>

      <section class="card danger-card">
        <div class="danger-row">
          <div>
            <h3>Reset &amp; clear all Dropilot data</h3>
            <p>Deletes Credential Manager tokens, Dropilot's isolated WebView2 cookies, and resets preferences to defaults.</p>
          </div>
          <Button variant="danger" size="sm" onclick={() => (clearModalOpen = true)}>
            <Trash2 size={14} />
            <span>Clear app data</span>
          </Button>
        </div>
      </section>
    </div>
  {/if}
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
  .settings-page {
    gap: 16px;
  }

  .accounts-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  @media (max-width: 780px) {
    .accounts-grid {
      grid-template-columns: 1fr;
    }
  }

  .acct-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 16px;
  }

  .acct-left {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .acct-meta {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .acct-top {
    display: flex;
    align-items: center;
    gap: 7px;
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

  .seg-tabs {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    padding: 6px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    border: 1px solid var(--border);
  }

  @media (max-width: 760px) {
    .seg-tabs {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .seg-tab {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    text-align: left;
  }

  .seg-tab:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .seg-tab.active {
    background: var(--accent-tint);
    border-color: var(--border-hover);
    color: var(--text);
  }

  .seg-ico {
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .seg-tab.active .seg-ico {
    background: var(--grad-accent);
    border-color: transparent;
    color: var(--accent-on);
  }

  .seg-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .seg-label {
    font-size: 13px;
    font-weight: 700;
    line-height: 1.2;
  }

  .seg-desc {
    font-size: 11px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .panel-stack {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .studio-card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px;
  }

  .studio-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .studio-sub {
    font-size: 12px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .accent-grid {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 10px;
  }

  @media (max-width: 900px) {
    .accent-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .accent-tile {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--surface-2);
    text-align: left;
  }

  .accent-tile:hover {
    border-color: var(--border-hover);
    background: var(--surface-hover);
  }

  .accent-tile.selected {
    border-color: var(--accent);
    background: var(--accent-tint);
  }

  .accent-orb {
    width: 28px;
    height: 28px;
    border-radius: var(--radius-pill);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: #06131a;
    flex-shrink: 0;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35);
  }

  .accent-meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .accent-name {
    font-size: 12px;
    font-weight: 700;
    color: var(--text);
  }

  .accent-tag {
    font-size: 10px;
    color: var(--text-faint);
  }

  .theme-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }

  @media (max-width: 760px) {
    .theme-grid {
      grid-template-columns: 1fr;
    }
  }

  .theme-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--surface-2);
    text-align: left;
  }

  .theme-card:hover {
    border-color: var(--border-hover);
  }

  .theme-card.active {
    border-color: var(--accent);
    background: var(--accent-tint);
  }

  .theme-preview {
    height: 68px;
    border-radius: 9px;
    border: 1px solid var(--border);
    padding: 8px;
    display: flex;
    gap: 8px;
    overflow: hidden;
  }

  .tp-rail {
    width: 24px;
    border-radius: 5px;
    border: 1px solid rgba(148, 163, 184, 0.16);
  }

  .tp-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .tp-bar {
    height: 8px;
    width: 58%;
    border-radius: var(--radius-pill);
  }

  .tp-box {
    flex: 1;
    border-radius: 6px;
    border: 1px solid rgba(148, 163, 184, 0.16);
  }

  .theme-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 2px;
  }

  .theme-name {
    font-size: 13px;
    font-weight: 700;
    color: var(--text);
  }

  .theme-hint {
    font-size: 11px;
    color: var(--text-muted);
  }

  .theme-check {
    width: 22px;
    height: 22px;
    border-radius: var(--radius-pill);
    background: var(--grad-accent);
    color: var(--accent-on);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .pref-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  @media (max-width: 760px) {
    .pref-grid {
      grid-template-columns: 1fr;
    }
  }

  .pref-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 16px;
  }

  .pref-wide {
    grid-column: 1 / -1;
  }

  .pref-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 2px;
  }

  .pref-ico {
    width: 34px;
    height: 34px;
    border-radius: 10px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    flex-shrink: 0;
  }

  .ico-accent {
    background: var(--accent-tint);
    border-color: var(--border-hover);
    color: var(--accent-text);
  }

  .ico-info {
    background: var(--info-tint);
    border-color: rgba(56, 189, 248, 0.32);
    color: var(--info-text);
  }

  .ico-success {
    background: var(--success-tint);
    border-color: rgba(16, 185, 129, 0.32);
    color: var(--success-text);
  }

  .ico-warn {
    background: var(--warning-tint);
    border-color: rgba(245, 158, 11, 0.32);
    color: var(--warning-text);
  }

  .pref-card h3 {
    font-size: 14px;
    font-weight: 700;
    color: var(--text);
  }

  .pref-card p {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.45;
  }

  .pref-wide-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .pref-wide-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 240px;
    flex: 1;
  }

  .pref-select {
    min-width: 230px;
  }

  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .step-btn {
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font-size: 15px;
    font-weight: 700;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .step-btn:hover {
    background: var(--surface-hover);
    color: var(--accent-text);
  }

  .step-val {
    min-width: 40px;
    text-align: center;
    font-size: 13px;
    font-weight: 700;
  }

  .inline-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .about-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px;
  }

  .about-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .about-note {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .about-links {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .danger-card {
    padding: 16px 18px;
    border-color: rgba(244, 63, 94, 0.34);
    background: linear-gradient(145deg, rgba(244, 63, 94, 0.08) 0%, var(--surface) 75%);
  }

  .danger-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .danger-row h3 {
    font-size: 14px;
    font-weight: 700;
    color: var(--error-text);
  }

  .danger-row p {
    font-size: 12px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .title-with-badge {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  .schedule-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 14px;
    margin-top: 12px;
    padding: 12px;
    border-radius: var(--radius-md);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .sched-field {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sched-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .sub-divider {
    height: 1px;
    background: var(--border);
    margin: 14px 0;
  }

  .webhook-card {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .webhook-grid {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .webhook-box {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-radius: var(--radius-md);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .webhook-box-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .webhook-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .webhook-input-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .secret-input {
    flex: 1;
    min-width: 210px;
    height: 34px;
    padding: 0 11px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    font-size: 12px;
    outline: none;
  }

  .secret-input:focus {
    border-color: var(--border-hover);
  }

  .chat-id-input {
    flex: 0 0 140px;
    min-width: 120px;
  }

  .event-toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .ev-chip {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border-radius: var(--radius-md);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
  }
</style>
