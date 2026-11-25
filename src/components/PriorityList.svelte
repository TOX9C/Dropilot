<script lang="ts">
  import ChevronRight from 'lucide-svelte/icons/chevron-right';
  import Clock from 'lucide-svelte/icons/clock';
  import X from 'lucide-svelte/icons/x';
  import { slowNowMs } from '../lib/clock';
  import { formatEndCountdown } from '../lib/format';
  import { priorityItemMatchesGame } from '../lib/selectors';
  import type { Campaign, PlatformId, PriorityItem } from '../lib/types';
  import Badge from './Badge.svelte';
  import GamePicker from './GamePicker.svelte';

  interface Props {
    items: PriorityItem[];
    campaigns?: Campaign[];
    platform?: PlatformId;
    prioritizeEndingSoon?: boolean;
    onReorder: (next: PriorityItem[]) => void;
    onAddGame?: (gameName: string) => void | Promise<void>;
    onToggleEndingSoon?: (enabled: boolean) => void | Promise<void>;
  }

  let {
    items,
    campaigns = [],
    platform: _platform,
    prioritizeEndingSoon = false,
    onReorder,
    onAddGame,
    onToggleEndingSoon,
  }: Props = $props();

  let collapsed = $state(false);
  let dragIndex = $state<number | null>(null);
  let dropTargetIndex = $state<number | null>(null);
  let listEl = $state<HTMLUListElement | null>(null);

  const DRAG_THRESHOLD_PX = 4;
  let pendingDrag: {
    index: number;
    pointerId: number;
    startY: number;
    rowEl: HTMLLIElement;
  } | null = null;

  const isDragging = $derived(dragIndex !== null);

  const ENDING_SOON_MS = 24 * 60 * 60 * 1000;

  const watchedGameNames = $derived(
    items.filter((i): i is { kind: 'game'; gameId: string; gameName: string } => i.kind === 'game').map((i) => i.gameName),
  );

  function matchingActiveCampaigns(item: PriorityItem): Campaign[] {
    if (item.kind === 'campaign') {
      return campaigns.filter((c) => c.status === 'active' && c.id === item.campaignId);
    }
    return campaigns.filter(
      (c) => c.status === 'active' && priorityItemMatchesGame(item, c.game.id, c.game.name),
    );
  }

  function hasActiveDropFor(item: PriorityItem): boolean {
    return matchingActiveCampaigns(item).length > 0;
  }

  function urgentEndLabel(item: PriorityItem, now: number): string | null {
    const active = matchingActiveCampaigns(item);
    if (active.length === 0) return null;
    let earliestMs = Infinity;
    let earliestIso: string | null = null;
    for (const c of active) {
      const endMs = Date.parse(c.endsAt);
      if (Number.isFinite(endMs) && endMs > now && endMs < earliestMs) {
        earliestMs = endMs;
        earliestIso = c.endsAt;
      }
    }
    if (!earliestIso || earliestMs - now > ENDING_SOON_MS) return null;
    const cd = formatEndCountdown(earliestIso, now);
    return cd.ended ? null : cd.text;
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

  function resetPointerDrag() {
    if (pendingDrag) {
      try {
        if (pendingDrag.rowEl.hasPointerCapture(pendingDrag.pointerId)) {
          pendingDrag.rowEl.releasePointerCapture(pendingDrag.pointerId);
        }
      } catch {
        // Ignore if pointer is already released
      }
    }
    pendingDrag = null;
    dragIndex = null;
    dropTargetIndex = null;
  }

  function resolveDropTarget(clientY: number): number | null {
    if (!listEl) return null;
    const rows = Array.from(listEl.querySelectorAll<HTMLLIElement>(':scope > li.item'));
    if (rows.length === 0) return null;
    for (let i = 0; i < rows.length; i += 1) {
      const rect = rows[i].getBoundingClientRect();
      if (clientY <= rect.bottom) {
        return i;
      }
    }
    return rows.length - 1;
  }

  function handleRowPointerDown(ev: PointerEvent, idx: number) {
    if (ev.button !== 0 || items.length < 2) return;
    const target = ev.target as HTMLElement | null;
    if (target?.closest('.item-actions, button:not(.grip)')) return;

    const rowEl = ev.currentTarget as HTMLLIElement;
    pendingDrag = {
      index: idx,
      pointerId: ev.pointerId,
      startY: ev.clientY,
      rowEl,
    };
    try {
      rowEl.setPointerCapture(ev.pointerId);
    } catch {
      // Ignore if capture is unsupported
    }
  }

  function handleRowPointerMove(ev: PointerEvent) {
    if (!pendingDrag || ev.pointerId !== pendingDrag.pointerId) return;
    if (dragIndex === null) {
      if (Math.abs(ev.clientY - pendingDrag.startY) < DRAG_THRESHOLD_PX) return;
      dragIndex = pendingDrag.index;
      dropTargetIndex = pendingDrag.index;
    }
    ev.preventDefault();
    const nextTarget = resolveDropTarget(ev.clientY);
    if (nextTarget !== null) {
      dropTargetIndex = nextTarget;
    }
  }

  function handleRowPointerUp(ev: PointerEvent) {
    if (!pendingDrag || ev.pointerId !== pendingDrag.pointerId) return;
    if (dragIndex !== null && dropTargetIndex !== null && dragIndex !== dropTargetIndex) {
      moveItem(dragIndex, dropTargetIndex);
    }
    resetPointerDrag();
  }

  function handleRowPointerCancel(ev: PointerEvent) {
    if (!pendingDrag || ev.pointerId !== pendingDrag.pointerId) return;
    resetPointerDrag();
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
    <span class="hint">{prioritizeEndingSoon ? 'Ending soon boosted' : 'Top farms first'}</span>
  </button>

  {#if !collapsed}
    {#if onAddGame}
      <GamePicker
        {campaigns}
        watchedNames={watchedGameNames}
        placeholder="Add any game..."
        compact
        alignRight
        onSelect={onAddGame}
      />
    {/if}

    {#if onToggleEndingSoon}
      <div class="ending-soon-opt" class:active={prioritizeEndingSoon}>
        <div class="opt-text">
          <span class="opt-title">
            <Clock size={12} />
            <span>Ending soon first</span>
          </span>
          <span class="opt-desc">
            Farm lower priority drops ending within 24h while top ones still have time
          </span>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={prioritizeEndingSoon}
          aria-label="Prioritize drops ending soon"
          class="mini-switch"
          class:on={prioritizeEndingSoon}
          onclick={() => onToggleEndingSoon(!prioritizeEndingSoon)}
        >
          <span class="mini-thumb"></span>
        </button>
      </div>
    {/if}

    {#if items.length === 0}
      <div class="empty-illus">
        <p>Add any game above (even without drops yet) or click <strong>Farm</strong> on a campaign card.</p>
      </div>
    {:else}
      <ul
        bind:this={listEl}
        class="list"
        class:is-dragging={isDragging}
        role="listbox"
        aria-label="Sortable priority items"
      >
        {#each items as item, idx}
          {@const activeDrop = hasActiveDropFor(item)}
          {@const urgentEnd = urgentEndLabel(item, $slowNowMs)}
          <li
            class="item"
            class:dragging={dragIndex === idx}
            class:drop-over={dropTargetIndex === idx && dragIndex !== null && dragIndex !== idx}
            class:drop-before={dropTargetIndex === idx && dragIndex !== null && dropTargetIndex < dragIndex}
            class:drop-after={dropTargetIndex === idx && dragIndex !== null && dropTargetIndex > dragIndex}
            class:item-urgent={prioritizeEndingSoon && Boolean(urgentEnd)}
            role="option"
            aria-selected="false"
            onpointerdown={(e) => handleRowPointerDown(e, idx)}
            onpointermove={handleRowPointerMove}
            onpointerup={handleRowPointerUp}
            onpointercancel={handleRowPointerCancel}
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
                    {activeDrop ? 'Drop active' : 'Waiting for drop'}
                  </span>
                {:else}
                  <span>Campaign</span>
                {/if}
                {#if urgentEnd && prioritizeEndingSoon}
                  <span class="urgent-chip tabular" title="Ending soon — boosted while higher-ranked drops still have time">
                    Ends {urgentEnd}
                  </span>
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

  .list.is-dragging,
  .list.is-dragging * {
    user-select: none;
    cursor: grabbing !important;
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    cursor: grab;
    touch-action: none;
    transition: opacity 120ms ease, border-color 120ms ease, background 120ms ease;
  }

  .item.dragging {
    opacity: 0.5;
    cursor: grabbing;
  }

  .item.drop-over {
    border-color: var(--accent);
    background: var(--accent-tint, var(--surface-hover));
  }

  .item.drop-before {
    box-shadow: inset 0 2px 0 var(--accent);
  }

  .item.drop-after {
    box-shadow: inset 0 -2px 0 var(--accent);
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
  }

  .watch-live-text {
    color: var(--success-text);
    font-weight: 600;
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

  .ending-soon-opt {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    transition: border-color 140ms ease, background 140ms ease;
  }

  .ending-soon-opt.active {
    border-color: rgba(245, 158, 11, 0.36);
    background: rgba(245, 158, 11, 0.08);
  }

  .opt-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .opt-title {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 700;
    color: var(--text);
  }

  .ending-soon-opt.active .opt-title {
    color: var(--warning-text);
  }

  .opt-desc {
    font-size: 11px;
    line-height: 1.3;
    color: var(--text-faint);
  }

  .mini-switch {
    position: relative;
    width: 34px;
    height: 19px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border-hover);
    background: var(--surface-hover);
    padding: 0;
    flex-shrink: 0;
    transition: background 150ms ease, border-color 150ms ease;
  }

  .mini-switch.on {
    background: var(--warning);
    border-color: var(--warning);
  }

  .mini-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: #fff;
    transition: transform 150ms ease;
  }

  .mini-switch.on .mini-thumb {
    transform: translateX(15px);
  }

  .item.item-urgent {
    border-color: rgba(245, 158, 11, 0.35);
  }

  .urgent-chip {
    display: inline-flex;
    align-items: center;
    padding: 1px 5px;
    border-radius: var(--radius-pill);
    background: var(--warning-tint);
    color: var(--warning-text);
    font-size: 10px;
    font-weight: 700;
    white-space: nowrap;
  }
</style>
