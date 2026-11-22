<script lang="ts">
  import { onMount } from 'svelte';
  import Award from 'lucide-svelte/icons/award';
  import BarChart3 from 'lucide-svelte/icons/bar-chart-3';
  import Clock from 'lucide-svelte/icons/clock';
  import Download from 'lucide-svelte/icons/download';
  import Flame from 'lucide-svelte/icons/flame';
  import Gift from 'lucide-svelte/icons/gift';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Search from 'lucide-svelte/icons/search';
  import Trash2 from 'lucide-svelte/icons/trash-2';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import CustomSelect, { type SelectOption } from '../components/CustomSelect.svelte';
  import Modal from '../components/Modal.svelte';
  import Monogram from '../components/Monogram.svelte';
  import StatTile from '../components/StatTile.svelte';
  import { api } from '../lib/api';
  import { formatExactDateTime, formatMinutes } from '../lib/format';
  import { notifyUi } from '../lib/stores';
  import type { HistoryStats, PlatformFilter } from '../lib/types';

  let stats = $state<HistoryStats | null>(null);
  let loading = $state(true);
  let exporting = $state(false);
  let clearModalOpen = $state(false);

  let search = $state('');
  let platformFilter = $state<PlatformFilter>('all');
  let statusFilter = $state<'all' | 'claimed' | 'manual_required'>('all');

  async function loadHistory() {
    loading = true;
    try {
      stats = await api.getHistoryStats();
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadHistory();
  });

  const filteredClaims = $derived.by(() => {
    const list = stats?.claims ?? [];
    const q = search.trim().toLowerCase();
    return list.filter((item) => {
      if (platformFilter !== 'all' && item.platform !== platformFilter) return false;
      if (statusFilter !== 'all' && item.status !== statusFilter) return false;
      if (!q) return true;
      return (
        item.gameName.toLowerCase().includes(q) ||
        item.dropName.toLowerCase().includes(q) ||
        item.campaignName.toLowerCase().includes(q)
      );
    });
  });

  const maxDailyMinutes = $derived.by(() => {
    const pts = stats?.dailyActivity ?? [];
    return Math.max(1, ...pts.map((p) => p.minutes));
  });

  function shortDayLabel(isoDay: string): string {
    const parts = isoDay.split('-');
    if (parts.length !== 3) return isoDay;
    return `${parts[1]}/${parts[2]}`;
  }

  async function handleExport(format: 'csv' | 'json') {
    exporting = true;
    try {
      const res = await api.exportHistory(format);
      // Also trigger a quick browser download in addition to saving in %APPDATA%\Dropilot\exports.
      try {
        const blob = new Blob([res.content], {
          type: format === 'csv' ? 'text/csv;charset=utf-8' : 'application/json;charset=utf-8',
        });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = res.filename;
        a.click();
        URL.revokeObjectURL(url);
      } catch {
        // Disk save succeeded regardless
      }
      notifyUi(`Exported ${res.filename} to Dropilot/exports`);
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      exporting = false;
    }
  }

  async function handleClearHistory() {
    try {
      await api.clearHistory();
      clearModalOpen = false;
      await loadHistory();
      notifyUi('Claim history cleared');
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }
</script>

<div class="page history-page">
  <header class="page-head">
    <div class="head-left">
      <div class="head-badge">
        <BarChart3 size={20} />
      </div>
      <div>
        <h1>History &amp; Stats</h1>
        <p class="subtitle">Persistent lifetime claimed drops, watch time saved, and activity streaks.</p>
      </div>
    </div>
    <div class="head-actions">
      <Button variant="secondary" size="sm" disabled={loading} onclick={loadHistory}>
        <RefreshCw size={14} />
        <span>Refresh</span>
      </Button>
      <Button variant="secondary" size="sm" disabled={exporting} onclick={() => handleExport('csv')}>
        <Download size={14} />
        <span>CSV</span>
      </Button>
      <Button variant="secondary" size="sm" disabled={exporting} onclick={() => handleExport('json')}>
        <Download size={14} />
        <span>JSON</span>
      </Button>
      {#if stats && (stats.claims.length > 0 || stats.totalWatchMinutes > 0)}
        <Button variant="danger" size="sm" onclick={() => (clearModalOpen = true)}>
          <Trash2 size={14} />
          <span>Clear</span>
        </Button>
      {/if}
    </div>
  </header>

  <section class="stats-grid" aria-label="Lifetime statistics">
    <StatTile
      label="Drops Claimed"
      value={stats?.totalClaims ?? 0}
      hint={stats ? `${stats.twitchClaims} Twitch · ${stats.kickClaims} Kick` : 'Loading...'}
      tone="success"
    >
      {#snippet icon()}<Gift size={16} />{/snippet}
    </StatTile>

    <StatTile
      label="Watch Time Saved"
      value={formatMinutes(stats?.totalWatchMinutes ?? 0)}
      hint="Zero-video background farming"
      tone="accent"
    >
      {#snippet icon()}<Clock size={16} />{/snippet}
    </StatTile>

    <StatTile
      label="Farming Streak"
      value={stats ? `${stats.currentStreakDays}d` : '0d'}
      hint={stats ? `Best streak: ${stats.bestStreakDays} days` : 'Loading...'}
      tone="twitch"
    >
      {#snippet icon()}<Flame size={16} />{/snippet}
    </StatTile>

    <StatTile
      label="Top Farmed Game"
      value={stats?.topGame ?? '—'}
      hint={stats?.topGame ? `${stats.topGameClaims} claimed reward${stats.topGameClaims === 1 ? '' : 's'}` : 'No claimed drops yet'}
      tone="kick"
    >
      {#snippet icon()}<Award size={16} />{/snippet}
    </StatTile>
  </section>

  {#if stats}
    <section class="card activity-card">
      <div class="card-head">
        <div>
          <h2>14-Day Farming Activity</h2>
          <p class="card-sub">Daily watch minutes credited across Twitch &amp; Kick</p>
        </div>
        <Badge tone="accent">{formatMinutes(stats.dailyActivity.reduce((a, b) => a + b.minutes, 0))} last 14d</Badge>
      </div>

      <div class="bars-grid" role="img" aria-label="14-day watch activity bar chart">
        {#each stats.dailyActivity as point (point.date)}
          {@const pct = point.minutes > 0 ? Math.max(8, Math.round((point.minutes / maxDailyMinutes) * 100)) : 3}
          <div
            class="bar-col"
            title={`${point.date}: ${formatMinutes(point.minutes)} watched · ${point.claims} claimed`}
          >
            <div class="bar-track">
              <div
                class="bar-fill"
                class:active={point.minutes > 0}
                style="height: {pct}%"
              ></div>
            </div>
            <span class="bar-label tabular">{shortDayLabel(point.date)}</span>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  <section class="card table-card">
    <div class="filter-row">
      <div class="search-wrap">
        <Search size={14} />
        <input
          type="text"
          bind:value={search}
          placeholder="Search game, reward, or campaign..."
          aria-label="Search claim history"
        />
      </div>

      <div class="filter-controls">
        <CustomSelect
          options={[
            { value: 'all', label: 'All platforms', dotTone: 'accent' },
            { value: 'twitch', label: 'Twitch', dotTone: 'twitch' },
            { value: 'kick', label: 'Kick', dotTone: 'kick' },
          ] satisfies SelectOption[]}
          value={platformFilter}
          onchange={(v) => (platformFilter = v as PlatformFilter)}
          ariaLabel="Filter by platform"
          size="sm"
        />

        <CustomSelect
          options={[
            { value: 'all', label: 'All statuses', dotTone: 'muted' },
            { value: 'claimed', label: 'Claimed', dotTone: 'live' },
            { value: 'manual_required', label: 'Manual required', dotTone: 'warn' },
          ] satisfies SelectOption[]}
          value={statusFilter}
          onchange={(v) => (statusFilter = v as 'all' | 'claimed' | 'manual_required')}
          ariaLabel="Filter by claim status"
          size="sm"
        />
      </div>
    </div>

    {#if loading && !stats}
      <div class="empty-state">Loading history...</div>
    {:else if filteredClaims.length === 0}
      <div class="empty-state">
        <Gift size={28} />
        <h3>No claim records match your filter</h3>
        <p>
          {stats && stats.claims.length > 0
            ? 'Try clearing your search or switching platform filters.'
            : 'Completed Twitch & Kick drops will be logged here automatically and saved across restarts.'}
        </p>
      </div>
    {:else}
      <div class="history-list">
        {#each filteredClaims as item (item.id + item.claimedAt)}
          <div class="history-row">
            <div class="row-main">
              <Monogram src={item.rewardImageUrl} name={item.gameName} width={36} height={36} />
              <div class="row-text">
                <div class="row-title">
                  <span class="drop-name">{item.dropName}</span>
                  <Badge tone={item.platform === 'twitch' ? 'twitch' : 'kick'}>
                    {item.platform === 'twitch' ? 'Twitch' : 'Kick'}
                  </Badge>
                  {#if item.status === 'manual_required'}
                    <Badge tone="warning">Manual claim</Badge>
                  {:else}
                    <Badge tone="success">Claimed</Badge>
                  {/if}
                </div>
                <div class="row-sub">
                  <span>{item.gameName}</span>
                  <span class="sep">·</span>
                  <span class="camp-sub">{item.campaignName}</span>
                </div>
              </div>
            </div>
            <div class="row-time tabular">{formatExactDateTime(item.claimedAt)}</div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>

<Modal
  open={clearModalOpen}
  title="Clear persistent claim history?"
  onclose={() => (clearModalOpen = false)}
>
  <p>
    This permanently deletes your saved claim log, watch-time totals, and farming streak counters from
    <code>%APPDATA%\Dropilot\history.json</code>.
  </p>
  {#snippet footer()}
    <Button variant="secondary" size="sm" onclick={() => (clearModalOpen = false)}>Cancel</Button>
    <Button variant="danger" size="sm" onclick={handleClearHistory}>Clear history</Button>
  {/snippet}
</Modal>

<style>
  .history-page {
    gap: 16px;
  }

  .page-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
  }

  .head-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .head-badge {
    width: 40px;
    height: 40px;
    border-radius: 12px;
    background: var(--accent-tint);
    border: 1px solid var(--border-hover);
    color: var(--accent-text);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .head-left h1 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 700;
  }

  .subtitle {
    margin: 2px 0 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
  }

  @media (max-width: 980px) {
    .stats-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .activity-card {
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .card-head h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }

  .card-sub {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--text-muted);
  }

  .bars-grid {
    display: grid;
    grid-template-columns: repeat(14, minmax(0, 1fr));
    gap: 8px;
    align-items: end;
    height: 112px;
    padding-top: 8px;
  }

  .bar-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    height: 100%;
  }

  .bar-track {
    flex: 1;
    width: 100%;
    max-width: 28px;
    border-radius: 6px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    display: flex;
    align-items: flex-end;
    overflow: hidden;
  }

  .bar-fill {
    width: 100%;
    border-radius: 5px;
    background: rgba(148, 163, 184, 0.22);
    transition: height 240ms var(--ease-out);
  }

  .bar-fill.active {
    background: var(--grad-accent);
  }

  .bar-label {
    font-size: 10px;
    color: var(--text-faint);
    white-space: nowrap;
  }

  .table-card {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .filter-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
  }

  .search-wrap {
    flex: 1;
    min-width: 220px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    height: 36px;
    border-radius: var(--radius-md);
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-muted);
  }

  .search-wrap:focus-within {
    border-color: var(--border-hover);
    color: var(--text);
  }

  .search-wrap input {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--text);
    font-size: var(--text-sm);
    outline: none;
  }

  .filter-controls {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .history-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .history-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    background: var(--surface-2);
    border: 1px solid var(--border);
    transition: border-color var(--duration-fast) var(--ease-out);
  }

  .history-row:hover {
    border-color: var(--border-hover);
  }

  .row-main {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .row-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .row-title {
    display: flex;
    align-items: center;
    gap: 7px;
    flex-wrap: wrap;
  }

  .drop-name {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text);
  }

  .row-sub {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sep {
    color: var(--text-faint);
  }

  .camp-sub {
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row-time {
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 8px;
    padding: 40px 20px;
    color: var(--text-muted);
  }

  .empty-state h3 {
    margin: 0;
    font-size: 15px;
    color: var(--text);
  }

  .empty-state p {
    margin: 0;
    max-width: 420px;
    font-size: 13px;
  }
</style>
