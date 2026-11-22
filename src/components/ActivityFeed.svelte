<script lang="ts">
  import Clock from 'lucide-svelte/icons/clock';
  import { slowNowMs } from '../lib/clock';
  import { formatRelativeTime } from '../lib/format';
  import { activeRoute } from '../lib/stores';
  import type { LogEntry } from '../lib/types';
  import Button from './Button.svelte';
  import SectionHeader from './SectionHeader.svelte';

  interface Props {
    logs: LogEntry[];
  }

  let { logs }: Props = $props();

  const recent = $derived(logs.slice(0, 6));
</script>

<section class="card feed-card">
  <SectionHeader title="Activity feed" uppercase>
    {#snippet icon()}
      <Clock size={15} />
    {/snippet}
    {#snippet actions()}
      <Button variant="ghost" size="sm" onclick={() => activeRoute.set('logs')}>
        <span>View logs →</span>
      </Button>
    {/snippet}
  </SectionHeader>

  {#if recent.length === 0}
    <p class="empty">Waiting for miner activity...</p>
  {:else}
    <ul class="feed-list">
      {#each recent as entry (entry.id)}
        <li class="feed-row">
          <span class="dot dot-{entry.level}" aria-hidden="true"></span>
          <span class="msg">{entry.message}</span>
          <span class="ts tabular">{formatRelativeTime(entry.timestamp, $slowNowMs)}</span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .feed-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .empty {
    color: var(--text-muted);
    font-size: var(--text-sm);
    padding: var(--space-2) 0;
  }

  .feed-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .feed-row {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
    flex-shrink: 0;
    transform: translateY(-1px);
  }

  .dot-info {
    background: var(--info);
  }
  .dot-warn {
    background: var(--warning);
  }
  .dot-error {
    background: var(--error);
  }
  .dot-debug {
    background: var(--text-faint);
  }

  .msg {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ts {
    font-size: var(--text-xs);
    color: var(--text-faint);
    white-space: nowrap;
  }
</style>
