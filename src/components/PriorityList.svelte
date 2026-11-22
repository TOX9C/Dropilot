<script lang="ts">
  import ChevronRight from 'lucide-svelte/icons/chevron-right';
  import Plus from 'lucide-svelte/icons/plus';
  import X from 'lucide-svelte/icons/x';
  import { priorityItemMatchesGame } from '../lib/selectors';
  import type { Campaign, PlatformId, PriorityItem } from '../lib/types';
  import Badge from './Badge.svelte';

  interface Props {
    items: PriorityItem[];
    campaigns?: Campaign[];
    platform?: PlatformId;
    onReorder: (next: PriorityItem[]) => void;
    onAddGame?: (gameName: string) => void | Promise<void>;
  }

  let { items, campaigns = [], platform, onReorder, onAddGame }: Props = $props();

  let collapsed = $state(false);
  let dragIndex = $state<number | null>(null);
  let dropTargetIndex = $state<number | null>(null);
  let gameInput = $state('');
  let adding = $state(false);

  const datalistId = $derived(`priority-games-${platform ?? 'all'}`);

  const suggestedGames = $derived.by(() => {
    const names = new Set<string>();
    for (const c of campaigns) {
      const n = c.game.name.trim();
      if (n) names.add(n);
    }
    return Array.from(names).sort((a, b) => a.localeCompare(b));
  });

  function hasActiveDropFor(item: PriorityItem): boolean {
    if (item.kind !== 'game') return false;
    return campaigns.some(
      (c) => c.status === 'active' && priorityItemMatchesGame(item, c.game.id, c.game.name),
    );
  }

  async function handleSubmitGame(ev: SubmitEvent) {
    ev.preventDefault();
    const clean = gameInput.trim();
    if (!clean || !onAddGame || adding) return;
    adding = true;
    try {
      await onAddGame(clean);
      gameInput = '';
    } finally {
      adding = false;
    }
  }

  function itemLabel(item: PriorityItem): string {
    return item.kind === 'game' ? item.gameName : item.campaignName;
  }

  function moveItem(from: number, to: number) {
    if (to < 0 || to >= items.length || from === to) return;
    const copy = [...items];
    const [moved] = copy.splice(from, 1);
    copy.splice(to, 0, moved);
    onReorder(copy);
  }

  function removeItem(idx: number) {
    onReorder(items.filter((_, i) => i !== idx));
  }

  function handleKeyDown(ev: KeyboardEvent, idx: number) {
    if (ev.altKey && ev.key === 'ArrowUp') {
      ev.preventDefault();
      moveItem(idx, idx - 1);
    } else if (ev.altKey && ev.key === 'ArrowDown') {
      ev.preventDefault();
      moveItem(idx, idx + 1);
    }
  }
</script>

<aside class="card priority-panel" aria-label="Priority queue">
  <button
    type="button"
    class="panel-header"
    aria-expanded={!collapsed}
    onclick={() => (collapsed = !collapsed)}
  >
    <div class="header-left">
      <span style:transform={collapsed ? 'none' : 'rotate(90deg)'} style="display:inline-flex"><ChevronRight size={16} /></span>
      <span class="section-label">Priority</span>
      <Badge>{items.length}</Badge>
    </div>
    <span class="hint">Top farms first</span>
  </button>

  {#if !collapsed}
    {#if onAddGame}
      <form class="add-game-form" onsubmit={handleSubmitGame}>
        <input
          class="add-game-input"
          type="text"
          list={datalistId}
          bind:value={gameInput}
          placeholder="Add any game to auto-mine..."
          aria-label="Game name to auto-mine"
        />
        <datalist id={datalistId}>
          {#each suggestedGames as gName (gName)}
            <option value={gName}></option>
          {/each}
        </datalist>
        <button
          type="submit"
          class="add-game-btn"
          disabled={!gameInput.trim() || adding}
          title="Watch this game and auto-mine whenever a drop appears"
        >
          <Plus size={12} />
          <span>Watch</span>
        </button>
      </form>
    {/if}

    {#if items.length === 0}
      <div class="empty-illus">
        <p>Add any game above (even without drops yet) or click <strong>Farm</strong> on a campaign card.</p>
      </div>
    {:else}
      <ul class="list" role="listbox" aria-label="Sortable priority items">
        {#each items as item, idx}
          {@const activeDrop = hasActiveDropFor(item)}
          <li
            class="item"
            class:dragging={dragIndex === idx}
            class:drop-over={dropTargetIndex === idx && dragIndex !== idx}
            role="option"
            aria-selected="false"
            draggable="true"
            ondragstart={() => (dragIndex = idx)}
            ondragover={(e) => {
              e.preventDefault();
              dropTargetIndex = idx;
            }}
            ondrop={(e) => {
              e.preventDefault();
              if (dragIndex !== null) moveItem(dragIndex, idx);
              dragIndex = null;
              dropTargetIndex = null;
            }}
            ondragend={() => {
              dragIndex = null;
              dropTargetIndex = null;
            }}
          >
            <button
              type="button"
              class="grip"
              aria-label={`Reorder ${itemLabel(item)}. Use Alt plus Up or Down arrow`}
              onkeydown={(e) => handleKeyDown(e, idx)}
            >
              ⋮⋮
            </button>
            <span class="rank tabular">{idx + 1}</span>
            <div class="item-meta">
              <span class="item-name">{itemLabel(item)}</span>
              <span class="item-kind">
                {#if item.kind === 'game'}
                  <span class="watch-dot" class:watch-live={activeDrop} aria-hidden="true"></span>
                  <span class:watch-live-text={activeDrop}>
                    {activeDrop ? 'Game · Drop active' : 'Game · Waiting for drop'}
                  </span>
                {:else}
                  Campaign
                {/if}
              </span>
            </div>
            <div class="item-actions">
              <button
                type="button"
                class="icon-btn"
                aria-label={`Move ${itemLabel(item)} up`}
                disabled={idx === 0}
                onclick={() => moveItem(idx, idx - 1)}
              >
                ↑
              </button>
              <button
                type="button"
                class="icon-btn"
                aria-label={`Move ${itemLabel(item)} down`}
                disabled={idx === items.length - 1}
                onclick={() => moveItem(idx, idx + 1)}
              >
                ↓
              </button>
              <button
                type="button"
                class="icon-btn"
                aria-label={`Remove ${itemLabel(item)} from priority`}
                onclick={() => removeItem(idx)}
              >
                <X size={13} />
              </button>
            </div>
          </li>
        {/each}
      </ul>
      <p class="kbd-tip">Drag rows or focus handle and press Alt+Up / Alt+Down.</p>
    {/if}
  {/if}
</aside>

<style>
  .priority-panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    background: transparent;
    border: none;
    padding: 0;
    width: 100%;
    text-align: left;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .hint {
    font-size: 11px;
    color: var(--text-faint);
  }

  .empty-illus {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--surface-2);
    border: 1px dashed var(--border);
    color: var(--text-muted);
    font-size: var(--text-xs);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .item.dragging {
    opacity: 0.5;
  }

  .item.drop-over {
    border-color: var(--accent);
  }

  .grip {
    color: var(--text-faint);
    background: transparent;
    border: none;
    padding: 2px;
    border-radius: var(--radius-sm);
    cursor: grab;
    display: flex;
  }

  .rank {
    font-size: var(--text-xs);
    color: var(--text-faint);
    width: 16px;
    text-align: center;
  }

  .item-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .item-name {
    font-size: var(--text-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-kind {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-faint);
  }

  .watch-dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
    background: var(--text-faint);
    flex-shrink: 0;
  }

  .watch-dot.watch-live {
    background: var(--success);
    box-shadow: 0 0 6px var(--success);
  }

  .watch-live-text {
    color: var(--success-text);
    font-weight: 600;
  }

  .add-game-form {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .add-game-input {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0 9px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    font-size: 12px;
  }

  .add-game-input::placeholder {
    color: var(--text-faint);
  }

  .add-game-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .add-game-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 30px;
    padding: 0 9px;
    border-radius: var(--radius-sm);
    border: 1px solid rgba(129, 140, 248, 0.35);
    background: var(--accent-tint);
    color: var(--accent-text);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }

  .add-game-btn:hover:not(:disabled) {
    border-color: var(--accent);
    filter: brightness(1.1);
  }

  .add-game-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .item-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .icon-btn {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text);
  }

  .icon-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .kbd-tip {
    font-size: 11px;
    color: var(--text-faint);
  }
</style>
