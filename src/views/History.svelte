<script lang="ts">
  import { onMount } from 'svelte';
  import Award from 'lucide-svelte/icons/award';
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
  import type { HistoryPlatformFilter, HistoryStats } from '../lib/types';

  let stats = $state<HistoryStats | null>(null);
  let loading = $state(true);
  let exporting = $state(false);
  let clearModalOpen = $state(false);

  let search = $state('');
  let platformFilter = $state<HistoryPlatformFilter>('all');
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

<div class="page history-page hist-console">
  <header class="page-header">
    <div class="page-title-wrap">
      <h1>History &amp; Stats</h1>
    </div>
    <div class="page-actions">
      <Button variant="secondary" size="sm" disabled={loading} onclick={loadHistory}>
        <RefreshCw size={13} />
        <span>Refresh</span>
      </Button>
      <Button variant="secondary" size="sm" disabled={exporting} onclick={() => handleExport('csv')}>
        <Download size={13} />
        <span>CSV</span>
      </Button>
      <Button variant="secondary" size="sm" disabled={exporting} onclick={() => handleExport('json')}>
        <Download size={13} />
        <span>JSON</span>
      </Button>
      {#if stats && (stats.claims.length > 0 || stats.totalWatchMinutes > 0)}
        <Button variant="danger" size="sm" onclick={() => (clearModalOpen = true)}>
          <Trash2 size={13} />
          <span>Clear</span>
        </Button>
      {/if}
    </div>
  </header>

  <section class="stats-grid" aria-label="Lifetime statistics">
    <StatTile
      label="Drops Claimed"
      value={stats?.totalClaims ?? 0}
      hint={stats
        ? stats.epicClaims > 0
          ? `${stats.twitchClaims} Twitch · ${stats.kickClaims} Kick · ${stats.epicClaims} Epic`
          : `${stats.twitchClaims} Twitch · ${stats.kickClaims} Kick`
        : 'Loading...'}
    >
      {#snippet icon()}<Gift size={14} />{/snippet}
    </StatTile>

    <StatTile
      label="Watch Time Saved"
      value={formatMinutes(stats?.totalWatchMinutes ?? 0)}
      hint="Zero-video background farming"
    >
      {#snippet icon()}<Clock size={14} />{/snippet}
    </StatTile>

    <StatTile
      label="Farming Streak"
      value={stats ? `${stats.currentStreakDays}d` : '0d'}
      hint={stats ? `Best streak: ${stats.bestStreakDays} days` : 'Loading...'}
    >
      {#snippet icon()}<Flame size={14} />{/snippet}
    </StatTile>

    <StatTile
      label="Top Farmed Game"
      value={stats?.topGame ?? '—'}
      hint={stats?.topGame ? `${stats.topGameClaims} claimed reward${stats.topGameClaims === 1 ? '' : 's'}` : 'No claimed drops yet'}
    >
      {#snippet icon()}<Award size={14} />{/snippet}
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
            { value: 'epic', label: 'Epic Games', dotTone: 'accent' },
          ] satisfies SelectOption[]}
          value={platformFilter}
          onchange={(v) => (platformFilter = v as HistoryPlatformFilter)}
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
      <div class="designed-empty">Loading history...</div>
    {:else if filteredClaims.length === 0}
      <div class="designed-empty">
        <span class="empty-ico" aria-hidden="true"><Gift size={18} /></span>
        <strong>No claim records match your filter</strong>
        <p>
          {stats && stats.claims.length > 0
            ? 'Try clearing your search or switching platform filters.'
            : 'Completed Twitch & Kick drops will be logged here automatically and saved across restarts.'}
        </p>
      </div>
    {:else}
      <div class="console-table">
        <div class="ct-head tabular">
          <span>TIMESTAMP</span>
          <span>PLAT</span>
          <span>REWARD</span>
          <span>GAME &amp; CAMPAIGN</span>
          <span class="ct-right">STATUS</span>
        </div>
        {#each filteredClaims as item (item.id + item.claimedAt)}
          <div class="ct-row">
            <span class="ct-time tabular">{formatExactDateTime(item.claimedAt)}</span>
            <span class="ct-plat">
              <span class="plat-dot dot-{item.platform}" aria-hidden="true"></span>
              <span>{item.platform.toUpperCase()}</span>
            </span>
            <span class="ct-drop">
              <Monogram
                src={item.rewardImageUrl}
                name={item.dropName || item.gameName}
                width={22}
                height={22}
              />
              <span class="truncate">{item.dropName}</span>
            </span>
            <span class="ct-game truncate">{item.gameName} · <span class="camp-sub">{item.campaignName}</span></span>
            <span class="ct-right">
              {#if item.status === 'manual_required'}
                <Badge tone="warning">Manual</Badge>
              {:else}
                <Badge tone="success">Claimed</Badge>
              {/if}
            </span>
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
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--grid-gap, 14px);
  }

  @media (max-width: 920px) {
    .stats-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .activity-card {
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
    font-size: var(--text-md);
    font-weight: 600;
  }

  .card-sub {
    margin-top: 2px;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .bars-grid {
    display: grid;
    grid-template-columns: repeat(14, minmax(0, 1fr));
    gap: 8px;
    align-items: end;
    height: 112px;
    padding-top: 6px;
  }

  .hist-console .bars-grid {
    height: 82px;
    gap: 4px;
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
    max-width: 44px;
    border-radius: var(--radius-xs);
    background: var(--surface-2);
    border: 1px solid var(--border);
    display: flex;
    align-items: flex-end;
    overflow: hidden;
  }

  .bar-fill {
    width: 100%;
    border-radius: var(--radius-xs);
    background: var(--border-hover);
  }

  .bar-fill.active {
    background: var(--accent);
  }

  .bar-label {
    font-size: 10px;
    color: var(--text-faint);
    white-space: nowrap;
  }

  .table-card {
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
    padding: 0 11px;
    height: var(--btn-sm-h, 32px);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-muted);
  }

  .search-wrap:focus-within {
    border-color: var(--accent);
    color: var(--text);
  }

  .search-wrap input {
    flex: 1;
    height: 100%;
    padding: 0;
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
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
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
    gap: 8px;
    flex-wrap: wrap;
  }

  .drop-name {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text);
  }

  .plat-tag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
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

  .sep { color: var(--text-faint); }
  .camp-sub { color: var(--text-faint); overflow: hidden; text-overflow: ellipsis; }

  .row-time {
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Console tabular claims list */
  .console-table {
    border: 1px solid var(--border);
    border-radius: var(--radius-xs);
    overflow: hidden;
  }

  .ct-head,
  .ct-row {
    display: grid;
    grid-template-columns: 155px 80px minmax(150px, 1fr) minmax(180px, 1.3fr) 88px;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    font-size: 11.5px;
  }

  @media (min-width: 1440px) {
    .ct-head,
    .ct-row {
      grid-template-columns: 180px 94px minmax(200px, 1fr) minmax(240px, 1.4fr) 100px;
      padding: 7px 14px;
    }
  }

  .ct-head {
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    color: var(--text-faint);
  }

  .ct-row {
    border-bottom: 1px solid var(--border);
  }

  .ct-row:last-child {
    border-bottom: none;
  }

  .ct-row:hover {
    background: var(--surface-2);
  }

  .ct-time { color: var(--text-faint); font-size: 11px; }
  .ct-plat { display: inline-flex; align-items: center; gap: 5px; font-family: var(--font-mono); font-size: 10.5px; color: var(--text-muted); }
  .ct-drop {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font-weight: 600;
    color: var(--text);
  }
  .ct-game { color: var(--text-muted); }
  .ct-right { text-align: right; }
</style>
