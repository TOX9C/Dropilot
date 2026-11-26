<script lang="ts">
  import X from 'lucide-svelte/icons/x';
  import { slowNowMs } from '../lib/clock';
  import { formatRelativeTime, formatTimestamp } from '../lib/format';
  import { editionInfo } from '../lib/stores';
  import type { LogEntry, LogLevel, PlatformId } from '../lib/types';
  import Badge from './Badge.svelte';
  import Button from './Button.svelte';
  import CustomSelect, { type SelectOption } from './CustomSelect.svelte';
  import Toggle from './Toggle.svelte';

  interface Props {
    logs: LogEntry[];
    onCopy: (text: string) => void;
    onOpenFolder: () => void;
    onClearView: () => void;
  }

  interface CollapsedRow {
    entry: LogEntry;
    repeat: number;
  }

  let { logs, onCopy, onOpenFolder, onClearView }: Props = $props();

  let levelFilter = $state<'all' | LogLevel>('all');
  let platformFilter = $state<'all' | PlatformId>('all');
  let search = $state('');
  let followLogs = $state(true);
  let logBoxEl = $state<HTMLDivElement | null>(null);

  const collapsedRows = $derived.by<CollapsedRow[]>(() => {
    const q = search.trim().toLowerCase();
    const filtered = logs.filter((entry) => {
      if (levelFilter !== 'all' && entry.level !== levelFilter) return false;
      if (platformFilter !== 'all' && entry.platform !== platformFilter) return false;
      if (q && !entry.message.toLowerCase().includes(q)) return false;
      return true;
    });

    const grouped: CollapsedRow[] = [];
    for (const entry of filtered) {
      const last = grouped[grouped.length - 1];
      if (
        last &&
        last.entry.level === entry.level &&
        last.entry.platform === entry.platform &&
        last.entry.message === entry.message
      ) {
        last.repeat += 1;
      } else {
        grouped.push({ entry, repeat: 1 });
      }
    }
    return grouped;
  });

  $effect(() => {
    if (followLogs && logBoxEl && collapsedRows.length > 0) {
      // Logs are newest-first, so following keeps the top in view cleanly
      logBoxEl.scrollTop = 0;
    }
  });

  const levelOptions: SelectOption[] = [
    { value: 'all', label: 'All levels', dotTone: 'accent' },
    { value: 'info', label: 'Info', dotTone: 'info' },
    { value: 'warn', label: 'Warn', dotTone: 'warn' },
    { value: 'error', label: 'Error', dotTone: 'error' },
    { value: 'debug', label: 'Debug', dotTone: 'muted' },
  ];

  const platformOptions: SelectOption[] = [
    { value: 'all', label: 'All platforms', dotTone: 'accent' },
    { value: 'twitch', label: 'Twitch', dotTone: 'twitch' },
    { value: 'kick', label: 'Kick', dotTone: 'kick' },
  ];

  function copyVisible() {
    const text = collapsedRows
      .map(
        ({ entry, repeat }) =>
          `[${formatTimestamp(entry.timestamp)}] [${entry.level.toUpperCase()}]${entry.platform ? ` [${entry.platform}]` : ''} ${entry.message}${repeat > 1 ? ` (x${repeat})` : ''}`,
      )
      .join('\n');
    onCopy(text);
  }
</script>

<div class="log-view">
  <div class="toolbar">
    <div class="filters">
      <input
        type="search"
        class="search-input"
        bind:value={search}
        placeholder="Filter messages..."
        aria-label="Search log messages"
      />

      <CustomSelect
        options={levelOptions}
        value={levelFilter}
        onchange={(v) => (levelFilter = v as typeof levelFilter)}
        ariaLabel="Filter logs by level"
        size="sm"
        minMenuWidth={160}
      />

      {#if $editionInfo.kickEnabled}
        <CustomSelect
          options={platformOptions}
          value={platformFilter}
          onchange={(v) => (platformFilter = v as typeof platformFilter)}
          ariaLabel="Filter logs by platform"
          size="sm"
          minMenuWidth={170}
        />
      {/if}

      <label class="follow-toggle">
        <Toggle checked={followLogs} ariaLabel="Auto-follow newest logs" onchange={(v) => (followLogs = v)} />
        <span>Follow</span>
      </label>
    </div>

    <div class="actions">
      <Button variant="ghost" size="sm" onclick={onClearView}>
        <X size={14} />
        <span>Clear view</span>
      </Button>
      <Button variant="secondary" size="sm" onclick={copyVisible}>
        <span>Copy</span>
      </Button>
      <Button variant="secondary" size="sm" onclick={onOpenFolder}>
        <span>Open log folder ↗</span>
      </Button>
    </div>
  </div>

  {#if collapsedRows.length === 0}
    <div class="card empty-logs">
      <p>No log lines match the current filters.</p>
    </div>
  {:else}
    <div class="card log-box" role="log" aria-live="polite" bind:this={logBoxEl}>
      {#each collapsedRows as row (row.entry.id)}
        <div
          class="log-line cv-row mono border-{row.entry.level}"
          title={formatRelativeTime(row.entry.timestamp, $slowNowMs)}
        >
          <span class="ts tabular">{formatTimestamp(row.entry.timestamp)}</span>
          <span class="lvl lvl-{row.entry.level}">{row.entry.level.toUpperCase()}</span>
          {#if row.entry.platform}
            <Badge tone={row.entry.platform}>{row.entry.platform}</Badge>
          {/if}
          <span class="msg">{row.entry.message}</span>
          {#if row.repeat > 1}
            <Badge tone="neutral">×{row.repeat}</Badge>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .log-view {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-3);
    padding: 10px 12px;
    border-radius: var(--radius-md);
    background: var(--surface);
    border: 1px solid var(--border);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    flex: 1;
    min-width: 260px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .search-input {
    flex: 1;
    min-width: 180px;
    max-width: 340px;
  }

  .follow-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-xs);
    color: var(--text-muted);
    cursor: pointer;
  }

  .log-box {
    flex: 1;
    min-height: 360px;
    overflow-y: auto;
    background: var(--bg);
    padding: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .empty-logs {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-8) var(--space-4);
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  .log-line {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    padding: 4px 8px;
    border-left: 3px solid transparent;
    border-radius: 4px;
    line-height: 1.45;
  }

  .log-line:hover {
    background: var(--surface-2);
  }

  .border-info { border-left-color: var(--info); }
  .border-warn { border-left-color: var(--warning); }
  .border-error { border-left-color: var(--error); }
  .border-debug { border-left-color: var(--border-hover); }

  .ts {
    color: var(--text-faint);
    flex-shrink: 0;
  }

  .lvl {
    font-weight: 600;
    width: 44px;
    flex-shrink: 0;
  }

  .lvl-info { color: var(--info-text); }
  .lvl-warn { color: var(--warning-text); }
  .lvl-error { color: var(--error-text); }
  .lvl-debug { color: var(--text-faint); }

  .msg {
    color: var(--text);
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    padding-left: 4px;
  }
</style>
