<script lang="ts">
  import ArrowUpDown from 'lucide-svelte/icons/arrow-up-down';
  import Gamepad2 from 'lucide-svelte/icons/gamepad-2';
  import LayoutGrid from 'lucide-svelte/icons/layout-grid';
  import List from 'lucide-svelte/icons/list';
  import Search from 'lucide-svelte/icons/search';
  import X from 'lucide-svelte/icons/x';
  import type { CampaignFilter, CampaignSort, DropsViewMode, PlatformFilter } from '../lib/types';
  import CustomSelect, { type SelectOption } from './CustomSelect.svelte';

  interface Props {
    filter: CampaignFilter;
    sort: CampaignSort;
    search: string;
    counts: Record<CampaignFilter, number>;
    platformFilter?: PlatformFilter;
    platformCounts?: Record<PlatformFilter, number>;
    games?: Array<{ name: string; count: number }>;
    selectedGame?: string;
    viewMode?: DropsViewMode;
    onfilter: (next: CampaignFilter) => void;
    onsort: (next: CampaignSort) => void;
    onsearch: (next: string) => void;
    onplatform?: (next: PlatformFilter) => void;
    ongame?: (next: string) => void;
    onviewmode?: (next: DropsViewMode) => void;
  }

  let {
    filter,
    sort,
    search,
    counts,
    platformFilter = 'all',
    platformCounts,
    games = [],
    selectedGame = 'all',
    viewMode = 'grid',
    onfilter,
    onsort,
    onsearch,
    onplatform,
    ongame,
    onviewmode,
  }: Props = $props();

  const platformTabs: { id: PlatformFilter; label: string }[] = [
    { id: 'all', label: 'All platforms' },
    { id: 'twitch', label: 'Twitch' },
    { id: 'kick', label: 'Kick' },
  ];

  const tabs: { id: CampaignFilter; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'active', label: 'Active' },
    { id: 'connected', label: 'Connected' },
    { id: 'unlinked', label: 'Not linked' },
    { id: 'farming', label: 'Farming' },
    { id: 'claimable', label: 'Claimable' },
    { id: 'upcoming', label: 'Upcoming' },
    { id: 'completed', label: 'Completed' },
  ];

  const hasActiveFilters = $derived(
    platformFilter !== 'all' || filter !== 'all' || selectedGame !== 'all' || search.trim() !== '',
  );

  const gameOptions = $derived.by((): SelectOption[] => [
    {
      value: 'all',
      label: 'All games',
      meta: String(games.length),
      dotTone: 'accent',
    },
    ...games.map((g) => ({
      value: g.name,
      label: g.name,
      meta: String(g.count),
    })),
  ]);

  const sortOptions: SelectOption[] = [
    {
      value: 'ending_soon',
      label: 'Ending soonest',
      subtitle: 'Prioritize drops closing soon',
      dotTone: 'warn',
    },
    {
      value: 'progress',
      label: 'Most progress',
      subtitle: 'Closest to unlocking first',
      dotTone: 'accent',
    },
    {
      value: 'shortest',
      label: 'Shortest watch time',
      subtitle: 'Fastest drops to complete',
      dotTone: 'live',
    },
    {
      value: 'rewards',
      label: 'Most rewards',
      subtitle: 'Campaigns with the most skins',
      dotTone: 'twitch',
    },
    {
      value: 'name',
      label: 'Name (A–Z)',
      subtitle: 'Alphabetical by game',
      dotTone: 'muted',
    },
  ];

  function resetAll() {
    onplatform?.('all');
    onfilter('all');
    ongame?.('all');
    onsearch('');
  }
</script>

<div class="filter-bar card">
  <div class="top-controls">
    {#if onplatform}
      <div class="seg-group" role="tablist" aria-label="Platform filter">
        {#each platformTabs as ptab (ptab.id)}
          <button
            type="button"
            role="tab"
            aria-selected={platformFilter === ptab.id}
            class="seg-btn seg-{ptab.id}"
            class:active={platformFilter === ptab.id}
            onclick={() => onplatform(ptab.id)}
          >
            {#if ptab.id !== 'all'}
              <span class="plat-dot dot-{ptab.id}" aria-hidden="true"></span>
            {/if}
            <span>{ptab.label}</span>
            {#if platformCounts}
              <span class="count tabular">{platformCounts[ptab.id]}</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    <div class="search-wrap">
      <Search size={14} />
      <input
        type="search"
        class="search-input"
        value={search}
        oninput={(e) => onsearch((e.currentTarget as HTMLInputElement).value)}
        placeholder="Search game, campaign, or reward..."
        aria-label="Search campaigns by game, name, or reward"
      />
    </div>

    <div class="selects">
      {#if ongame && games.length > 0}
        <CustomSelect
          options={gameOptions}
          value={selectedGame}
          onchange={(val) => ongame(val)}
          ariaLabel="Filter by game"
          searchable={games.length > 6}
          searchPlaceholder="Search games..."
          size="sm"
          minMenuWidth={240}
          maxWidth="185px"
        >
          {#snippet icon()}
            <Gamepad2 size={13} />
          {/snippet}
        </CustomSelect>
      {/if}

      <CustomSelect
        options={sortOptions}
        value={sort}
        onchange={(val) => onsort(val as CampaignSort)}
        ariaLabel="Sort campaigns"
        size="sm"
        minMenuWidth={228}
        maxWidth="175px"
        align="right"
      >
        {#snippet icon()}
          <ArrowUpDown size={13} />
        {/snippet}
      </CustomSelect>

      {#if hasActiveFilters}
        <button type="button" class="reset-btn" onclick={resetAll} title="Clear all filters">
          <X size={13} />
          <span>Reset</span>
        </button>
      {/if}
    </div>
  </div>

  <div class="pills-row">
    <div class="pills" role="tablist" aria-label="Campaign status filter">
      {#each tabs as tab (tab.id)}
        {#if tab.id === 'all' || counts[tab.id] > 0 || filter === tab.id}
          <button
            type="button"
            role="tab"
            aria-selected={filter === tab.id}
            class="tab-pill pill-{tab.id}"
            class:active={filter === tab.id}
            onclick={() => onfilter(tab.id)}
          >
            <span>{tab.label}</span>
            <span class="count tabular">{counts[tab.id]}</span>
          </button>
        {/if}
      {/each}
    </div>

    {#if onviewmode}
      <div class="view-switch" role="group" aria-label="Layout density">
        <button
          type="button"
          class="view-btn"
          class:active={viewMode === 'grid'}
          aria-pressed={viewMode === 'grid'}
          title="Grid view"
          onclick={() => onviewmode('grid')}
        >
          <LayoutGrid size={13} />
          <span>Grid</span>
        </button>
        <button
          type="button"
          class="view-btn"
          class:active={viewMode === 'compact'}
          aria-pressed={viewMode === 'compact'}
          title="Compact list view"
          onclick={() => onviewmode('compact')}
        >
          <List size={13} />
          <span>Compact</span>
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .filter-bar {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
  }

  .top-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .top-controls :global(input),
  .top-controls :global(select) {
    height: 30px;
    font-size: var(--text-xs);
  }

  .seg-group {
    display: inline-flex;
    align-items: center;
    padding: 2px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    gap: 2px;
  }

  .seg-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 9px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .seg-btn:hover {
    color: var(--text);
  }

  .seg-btn.active {
    background: var(--surface-hover);
    color: var(--text);
  }

  .seg-twitch.active {
    background: var(--twitch-tint);
    color: var(--twitch-text);
  }

  .seg-kick.active {
    background: var(--kick-tint);
    color: var(--kick-text);
  }

  .search-wrap {
    flex: 1;
    min-width: 170px;
    position: relative;
    display: flex;
    align-items: center;
    color: var(--text-faint);
  }

  .search-wrap :global(svg) {
    position: absolute;
    left: 9px;
    pointer-events: none;
  }

  .search-input {
    width: 100%;
    padding-left: 28px;
  }

  .selects {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .reset-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 30px;
    padding: 0 9px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .reset-btn:hover {
    background: var(--surface-hover);
    color: var(--text);
  }

  .pills-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .pills {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
  }

  .tab-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .tab-pill:hover {
    background: var(--surface-hover);
    color: var(--text);
  }

  .tab-pill.active {
    background: var(--accent-tint);
    border-color: var(--accent);
    color: var(--accent-text);
  }

  .tab-pill.pill-connected.active,
  .tab-pill.pill-farming.active {
    background: var(--success-tint);
    border-color: var(--success);
    color: var(--success-text);
  }

  .tab-pill.pill-unlinked.active {
    background: var(--warning-tint);
    border-color: var(--warning);
    color: var(--warning-text);
  }

  .view-switch {
    display: inline-flex;
    align-items: center;
    padding: 2px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    gap: 2px;
    margin-left: auto;
  }

  .view-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
  }

  .view-btn:hover {
    color: var(--text);
  }

  .view-btn.active {
    background: var(--accent-tint);
    color: var(--accent-text);
  }

  .count {
    font-size: 11px;
    opacity: 0.85;
  }
</style>
