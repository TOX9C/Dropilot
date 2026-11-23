<script lang="ts">
  import Check from 'lucide-svelte/icons/check';
  import Plus from 'lucide-svelte/icons/plus';
  import Search from 'lucide-svelte/icons/search';
  import Sparkles from 'lucide-svelte/icons/sparkles';
  import X from 'lucide-svelte/icons/x';
  import type { Campaign } from '../lib/types';
  import Monogram from './Monogram.svelte';

  interface Props {
    campaigns: Campaign[];
    watchedNames?: string[];
    placeholder?: string;
    disabled?: boolean;
    compact?: boolean;
    alignRight?: boolean;
    buttonLabel?: string;
    onSelect: (gameName: string) => void | Promise<void>;
  }

  interface GameOption {
    key: string;
    name: string;
    boxArtUrl: string | null;
    activeCount: number;
    connected: boolean;
    watched: boolean;
  }

  let {
    campaigns,
    watchedNames = [],
    placeholder = 'Search or add any game...',
    disabled = false,
    compact = false,
    alignRight = false,
    buttonLabel = 'Watch',
    onSelect,
  }: Props = $props();

  let query = $state('');
  let open = $state(false);
  let busy = $state(false);
  let activeIndex = $state(0);
  let rootEl = $state<HTMLDivElement | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);
  const listboxId = `gp-list-${Math.random().toString(36).slice(2, 9)}`;

  const watchedSet = $derived(
    new Set(watchedNames.map((n) => n.trim().toLowerCase()).filter(Boolean)),
  );

  const allGames = $derived.by<GameOption[]>(() => {
    const map = new Map<string, GameOption>();
    for (const c of campaigns) {
      const name = c.game.name.trim();
      if (!name) continue;
      const key = name.toLowerCase();
      const existing = map.get(key);
      const isLive = c.status === 'active';
      const isConn = !c.requiresLink || c.linked;
      if (existing) {
        if (isLive) existing.activeCount += 1;
        if (!existing.boxArtUrl && c.game.boxArtUrl) existing.boxArtUrl = c.game.boxArtUrl;
        if (isConn) existing.connected = true;
      } else {
        map.set(key, {
          key,
          name,
          boxArtUrl: c.game.boxArtUrl,
          activeCount: isLive ? 1 : 0,
          connected: isConn,
          watched: watchedSet.has(key),
        });
      }
    }
    return Array.from(map.values()).sort((a, b) => {
      if (a.watched !== b.watched) return a.watched ? 1 : -1;
      if (b.activeCount !== a.activeCount) return b.activeCount - a.activeCount;
      return a.name.localeCompare(b.name);
    });
  });

  const trimmedQuery = $derived(query.trim());

  const filteredGames = $derived.by(() => {
    const q = trimmedQuery.toLowerCase();
    if (!q) return allGames.slice(0, 40);
    return allGames
      .filter((g) => g.key.includes(q))
      .sort((a, b) => {
        const aStarts = a.key.startsWith(q) ? 0 : 1;
        const bStarts = b.key.startsWith(q) ? 0 : 1;
        if (aStarts !== bStarts) return aStarts - bStarts;
        return a.name.localeCompare(b.name);
      })
      .slice(0, 30);
  });

  const exactMatchExists = $derived(
    Boolean(trimmedQuery) &&
      allGames.some((g) => g.key === trimmedQuery.toLowerCase()),
  );

  const showCustomOption = $derived(Boolean(trimmedQuery) && !exactMatchExists);

  const totalOptions = $derived(filteredGames.length + (showCustomOption ? 1 : 0));

  $effect(() => {
    void trimmedQuery;
    activeIndex = 0;
  });

  function handleWindowPointerDown(ev: PointerEvent) {
    if (!open || !rootEl) return;
    if (!rootEl.contains(ev.target as Node)) {
      open = false;
    }
  }

  async function pickGame(name: string) {
    const clean = name.trim();
    if (!clean || busy || disabled) return;
    busy = true;
    try {
      await onSelect(clean);
      query = '';
      open = false;
      inputEl?.blur();
    } finally {
      busy = false;
    }
  }

  function handleKeyDown(ev: KeyboardEvent) {
    if (ev.key === 'Escape') {
      open = false;
      inputEl?.blur();
      return;
    }
    if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      if (!open) {
        open = true;
        activeIndex = 0;
      } else if (totalOptions > 0) {
        activeIndex = (activeIndex + 1) % totalOptions;
      }
      return;
    }
    if (ev.key === 'ArrowUp') {
      ev.preventDefault();
      if (open && totalOptions > 0) {
        activeIndex = (activeIndex - 1 + totalOptions) % totalOptions;
      }
      return;
    }
    if (ev.key === 'Enter') {
      ev.preventDefault();
      if (open && totalOptions > 0) {
        if (activeIndex < filteredGames.length) {
          void pickGame(filteredGames[activeIndex].name);
          return;
        }
        if (showCustomOption) {
          void pickGame(trimmedQuery);
          return;
        }
      }
      if (trimmedQuery) {
        void pickGame(trimmedQuery);
      }
    }
  }
</script>

<svelte:window onpointerdown={handleWindowPointerDown} />

<div class="gp-root" class:gp-compact={compact} bind:this={rootEl}>
  <div class="gp-bar" class:gp-open={open}>
    <span class="gp-search-ico" aria-hidden="true">
      <Search size={13} />
    </span>

    <input
      bind:this={inputEl}
      type="text"
      class="gp-input"
      bind:value={query}
      {placeholder}
      {disabled}
      autocomplete="off"
      spellcheck="false"
      role="combobox"
      aria-controls={listboxId}
      aria-expanded={open}
      aria-autocomplete="list"
      aria-label="Add game to auto-mine"
      onfocus={() => (open = true)}
      onclick={() => (open = true)}
      oninput={() => (open = true)}
      onkeydown={handleKeyDown}
    />

    {#if query}
      <button
        type="button"
        class="gp-clear"
        aria-label="Clear search"
        onclick={() => {
          query = '';
          inputEl?.focus();
        }}
      >
        <X size={12} />
      </button>
    {/if}

    <button
      type="button"
      class="gp-submit"
      disabled={!trimmedQuery || busy || disabled}
      title="Watch this game and auto-mine whenever a drop goes live"
      onclick={() => void pickGame(trimmedQuery)}
    >
      <Plus size={12} />
      <span>{buttonLabel}</span>
    </button>
  </div>

  {#if open}
    <div
      id={listboxId}
      class="gp-popover"
      class:gp-align-right={alignRight}
      role="listbox"
      aria-label="Games with active drops"
    >
      <div class="gp-pop-head">
        <span>
          {trimmedQuery ? `Matches (${filteredGames.length})` : `Active drop games (${allGames.length})`}
        </span>
        <span class="gp-pop-hint">Click to watch</span>
      </div>

      <div class="gp-list">
        {#if filteredGames.length === 0 && !showCustomOption}
          <div class="gp-empty">Type any game name to auto-mine when drops go live.</div>
        {/if}

        {#each filteredGames as game, idx (game.key)}
          <button
            type="button"
            class="gp-row"
            class:gp-row-active={idx === activeIndex}
            class:gp-row-watched={game.watched}
            role="option"
            aria-selected={idx === activeIndex}
            onmouseenter={() => (activeIndex = idx)}
            onclick={() => void pickGame(game.name)}
          >
            <div class="gp-art">
              <Monogram src={game.boxArtUrl} name={game.name} width={24} height={32} />
            </div>

            <div class="gp-meta">
              <span class="gp-name" title={game.name}>{game.name}</span>
              <span class="gp-sub">
                {#if game.activeCount > 0}
                  <span class="gp-live-dot" aria-hidden="true"></span>
                  <span>{game.activeCount} live {game.activeCount === 1 ? 'drop' : 'drops'}</span>
                {:else}
                  <span>Catalog game</span>
                {/if}
                {#if game.connected}
                  <span class="gp-conn-tag">· Linked</span>
                {/if}
              </span>
            </div>

            {#if game.watched}
              <span class="gp-tag gp-tag-watched">
                <Check size={10} />
                <span>Watching</span>
              </span>
            {:else}
              <span class="gp-tag gp-tag-add">
                <Plus size={10} />
                <span>Add</span>
              </span>
            {/if}
          </button>
        {/each}

        {#if showCustomOption}
          <button
            type="button"
            class="gp-row gp-row-custom"
            class:gp-row-active={activeIndex === filteredGames.length}
            role="option"
            aria-selected={activeIndex === filteredGames.length}
            onmouseenter={() => (activeIndex = filteredGames.length)}
            onclick={() => void pickGame(trimmedQuery)}
          >
            <span class="gp-custom-ico" aria-hidden="true">
              <Sparkles size={14} />
            </span>
            <div class="gp-meta">
              <span class="gp-name">Watch "{trimmedQuery}"</span>
              <span class="gp-sub">Auto-mines the moment a drop goes live</span>
            </div>
            <span class="gp-tag gp-tag-add">
              <Plus size={10} />
              <span>Add</span>
            </span>
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .gp-root {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .gp-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 4px 0 9px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--surface-2);
    transition: border-color 140ms ease, box-shadow 140ms ease;
  }

  .gp-compact .gp-bar {
    height: 32px;
    padding: 0 3px 0 8px;
  }

  .gp-bar:hover {
    border-color: var(--border-hover);
  }

  .gp-bar.gp-open {
    border-color: var(--accent);
  }

  .gp-search-ico {
    display: inline-flex;
    align-items: center;
    color: var(--text-faint);
    flex-shrink: 0;
  }

  .gp-bar.gp-open .gp-search-ico {
    color: var(--accent-text);
  }

  .gp-input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text);
    font-size: 12px;
  }

  .gp-input:hover,
  .gp-input:focus {
    background: transparent;
    border: none;
    outline: none;
    box-shadow: none;
  }

  .gp-input::placeholder {
    color: var(--text-faint);
  }

  .gp-clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border: none;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--text-faint);
    flex-shrink: 0;
  }

  .gp-clear:hover {
    background: var(--surface-hover);
    color: var(--text);
  }

  .gp-submit {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 9px;
    border-radius: 7px;
    border: 1px solid var(--border-hover);
    background: var(--accent-tint);
    color: var(--accent-text);
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .gp-submit:hover:not(:disabled) {
    border-color: var(--accent);
    filter: brightness(1.1);
  }

  .gp-submit:disabled {
    opacity: 0.42;
    cursor: not-allowed;
  }

  .gp-popover {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    min-width: 100%;
    width: max(100%, 280px);
    max-width: 360px;
    z-index: 60;
    border-radius: var(--radius-md);
    background: var(--surface);
    border: 1px solid var(--border-hover);
    box-shadow: var(--popover-shadow);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .gp-popover.gp-align-right {
    left: auto;
    right: 0;
  }

  .gp-pop-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 11px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
  }

  .gp-pop-hint {
    color: var(--accent-text);
    font-weight: 600;
    text-transform: none;
    letter-spacing: 0;
  }

  .gp-list {
    max-height: 276px;
    overflow-y: auto;
    padding: 5px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .gp-empty {
    padding: 14px 10px;
    text-align: center;
    font-size: 12px;
    color: var(--text-muted);
  }

  .gp-row {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 6px 8px;
    border-radius: 8px;
    border: 1px solid transparent;
    background: transparent;
    text-align: left;
    color: var(--text);
  }

  .gp-row:hover,
  .gp-row.gp-row-active {
    background: var(--surface-2);
    border-color: var(--border-hover);
  }

  .gp-row.gp-row-watched {
    opacity: 0.78;
  }

  .gp-row-custom {
    border: 1px dashed var(--border-hover);
    background: var(--accent-tint);
  }

  .gp-art {
    flex-shrink: 0;
    border-radius: 5px;
    overflow: hidden;
  }

  .gp-custom-ico {
    width: 24px;
    height: 32px;
    border-radius: 5px;
    background: var(--surface);
    border: 1px solid var(--border-hover);
    color: var(--accent-text);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .gp-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .gp-name {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .gp-sub {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 10.5px;
    color: var(--text-faint);
  }

  .gp-live-dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
    background: var(--success);
    flex-shrink: 0;
  }

  .gp-conn-tag {
    color: var(--success-text);
    font-weight: 600;
  }

  .gp-tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 2px 7px;
    border-radius: var(--radius-pill);
    font-size: 10px;
    font-weight: 700;
    flex-shrink: 0;
  }

  .gp-tag-add {
    background: var(--accent-tint);
    border: 1px solid var(--border-hover);
    color: var(--accent-text);
  }

  .gp-tag-watched {
    background: var(--success-tint);
    border: 1px solid rgba(16, 185, 129, 0.32);
    color: var(--success-text);
  }
</style>
