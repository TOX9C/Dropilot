<script lang="ts">
  import Check from 'lucide-svelte/icons/check';
  import X from 'lucide-svelte/icons/x';
  import { api } from '../lib/api';
  import { activeRoute, appSettings, checklistDismissed, notifyUi } from '../lib/stores';
  import type { AppSnapshot, Settings } from '../lib/types';
  import Badge from './Badge.svelte';
  import Button from './Button.svelte';
  import SectionHeader from './SectionHeader.svelte';

  interface Props {
    snapshot: AppSnapshot;
    settings: Settings;
  }

  let { snapshot, settings }: Props = $props();

  const hasPriority = $derived(
    settings.twitch.priority.length > 0 || settings.kick.priority.length > 0,
  );

  const items = $derived([
    {
      id: 'twitch',
      label: 'Log in to Twitch',
      done: Boolean(snapshot.twitch.account),
      actionLabel: 'Log in',
      run: () => api.login('twitch'),
    },
    {
      id: 'kick',
      label: 'Log in to Kick',
      done: Boolean(snapshot.kick.account),
      actionLabel: 'Log in',
      run: () => api.login('kick'),
    },
    {
      id: 'priority',
      label: 'Add a game or campaign to priority',
      done: hasPriority,
      actionLabel: 'Browse Twitch',
      run: () => {
        activeRoute.set(snapshot.twitch.account ? 'twitch' : 'kick');
      },
    },
    {
      id: 'notify',
      label: 'Enable desktop notifications',
      done: settings.notifications,
      actionLabel: 'Enable',
      run: async () => {
        const next = structuredClone(settings);
        next.notifications = true;
        const saved = await api.updateSettings(next);
        appSettings.set(saved);
      },
    },
    {
      id: 'autostart',
      label: 'Start with Windows (optional)',
      done: settings.autostart,
      actionLabel: 'Turn on',
      run: async () => {
        const next = structuredClone(settings);
        next.autostart = true;
        const saved = await api.updateSettings(next);
        appSettings.set(saved);
      },
    },
  ]);

  const completedRequired = $derived(items.slice(0, 4).every((i) => i.done));
  const doneCount = $derived(items.filter((i) => i.done).length);

  async function trigger(run: () => Promise<void> | void) {
    try {
      await run();
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }
</script>

{#if !$checklistDismissed && !completedRequired}
  <section class="card checklist">
    <SectionHeader title="Getting started" uppercase>
      {#snippet icon()}
        <Check size={15} />
      {/snippet}
      {#snippet actions()}
        <Badge tone="accent">{doneCount} / {items.length}</Badge>
        <button
          type="button"
          class="dismiss-btn"
          aria-label="Dismiss getting started checklist"
          onclick={() => checklistDismissed.set(true)}
        >
          <X size={14} />
        </button>
      {/snippet}
    </SectionHeader>

    <div class="check-grid">
      {#each items as item (item.id)}
        <div class="check-item" class:done={item.done}>
          <span class="check-icon" aria-hidden="true">
            {#if item.done}
              <Check size={14} />
            {:else}
              <span class="circle-dot"></span>
            {/if}
          </span>
          <span class="check-label">{item.label}</span>
          {#if !item.done}
            <Button variant="ghost" size="sm" onclick={() => trigger(item.run)}>
              {item.actionLabel}
            </Button>
          {/if}
        </div>
      {/each}
    </div>
  </section>
{/if}

<style>
  .checklist {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .dismiss-btn {
    border: none;
    background: transparent;
    color: var(--text-muted);
    padding: 4px;
    border-radius: var(--radius-sm);
    display: flex;
  }

  .dismiss-btn:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .check-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
    gap: var(--space-2);
  }

  .check-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    min-height: 36px;
  }

  .check-icon {
    color: var(--text-faint);
    display: flex;
    flex-shrink: 0;
  }

  .check-item.done .check-icon {
    color: var(--success-text);
  }

  .circle-dot {
    width: 12px;
    height: 12px;
    border-radius: 999px;
    border: 1.5px solid var(--text-faint);
  }

  .check-label {
    flex: 1;
    font-size: var(--text-sm);
  }

  .check-item.done .check-label {
    color: var(--text-muted);
    text-decoration: line-through;
  }
</style>
