<script lang="ts">
  import Check from 'lucide-svelte/icons/check';
  import ChevronDown from 'lucide-svelte/icons/chevron-down';
  import Search from 'lucide-svelte/icons/search';
  import X from 'lucide-svelte/icons/x';
  import type { Snippet } from 'svelte';
  import Monogram from './Monogram.svelte';

  export type SelectDotTone =
    | 'live'
    | 'offline'
    | 'twitch'
    | 'kick'
    | 'accent'
    | 'info'
    | 'warn'
    | 'error'
    | 'muted';

  export interface SelectOption {
    value: string;
    label: string;
    meta?: string;
    subtitle?: string;
    avatarUrl?: string | null;
    dotTone?: SelectDotTone;
  }

  interface Props {
    options: SelectOption[];
    value: string;
    onchange: (value: string) => void;
    ariaLabel: string;
    title?: string;
    placeholder?: string;
    searchable?: boolean;
    searchPlaceholder?: string;
    size?: 'sm' | 'md';
    minMenuWidth?: number;
    maxWidth?: string;
    align?: 'left' | 'right';
    icon?: Snippet;
  }

  let {
    options,
    value,
    onchange,
    ariaLabel,
    title = '',
    placeholder = 'Select...',
    searchable = false,
    searchPlaceholder = 'Search...',
    size = 'sm',
    minMenuWidth = 220,
    maxWidth = 'none',
    align = 'left',
    icon,
  }: Props = $props();

  let open = $state(false);
  let query = $state('');
  let activeIndex = $state(0);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let menuStyle = $state('');
  let openUpward = $state(false);

  const selectedOption = $derived(options.find((o) => o.value === value) ?? null);

  const filteredOptions = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return options;
    return options.filter(
      (o) =>
        o.label.toLowerCase().includes(q) ||
        (o.subtitle && o.subtitle.toLowerCase().includes(q)) ||
        (o.meta && o.meta.toLowerCase().includes(q)),
    );
  });

  function updatePosition() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const width = Math.min(Math.max(rect.width, minMenuWidth), vw - 24);
    const estHeight = Math.min(filteredOptions.length * 40 + (searchable ? 54 : 14), 320);

    const spaceBelow = vh - rect.bottom - 12;
    const spaceAbove = rect.top - 12;
    const flipUp = spaceBelow < Math.min(estHeight, 200) && spaceAbove > spaceBelow;
    openUpward = flipUp;

    let left = align === 'right' ? rect.right - width : rect.left;
    if (left + width > vw - 12) left = vw - width - 12;
    if (left < 12) left = 12;

    if (flipUp) {
      const bottom = vh - rect.top + 6;
      const maxH = Math.max(120, Math.min(320, spaceAbove - 8));
      menuStyle = `left:${Math.round(left)}px;bottom:${Math.round(bottom)}px;width:${Math.round(width)}px;--menu-max-h:${Math.round(maxH)}px;`;
    } else {
      const top = rect.bottom + 6;
      const maxH = Math.max(120, Math.min(320, spaceBelow - 8));
      menuStyle = `left:${Math.round(left)}px;top:${Math.round(top)}px;width:${Math.round(width)}px;--menu-max-h:${Math.round(maxH)}px;`;
    }
  }

  function openMenu() {
    open = true;
    query = '';
    const idx = options.findIndex((o) => o.value === value);
    activeIndex = idx >= 0 ? idx : 0;
    updatePosition();
    queueMicrotask(() => {
      updatePosition();
      if (searchable && searchInputEl) {
        searchInputEl.focus();
      }
      const activeRow = menuEl?.querySelector<HTMLElement>('[data-active="true"]');
      activeRow?.scrollIntoView({ block: 'nearest' });
    });
  }

  function closeMenu(restoreFocus = false) {
    open = false;
    query = '';
    if (restoreFocus) {
      triggerEl?.focus();
    }
  }

  function toggleMenu() {
    if (open) {
      closeMenu();
    } else {
      openMenu();
    }
  }

  function pick(opt: SelectOption) {
    onchange(opt.value);
    closeMenu(true);
  }

  function onKeydown(e: KeyboardEvent) {
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        openMenu();
      }
      return;
    }

    if (e.key === 'Escape') {
      e.preventDefault();
      closeMenu(true);
      return;
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (filteredOptions.length > 0) {
        activeIndex = (activeIndex + 1) % filteredOptions.length;
        scrollHighlightedIntoView();
      }
      return;
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (filteredOptions.length > 0) {
        activeIndex = (activeIndex - 1 + filteredOptions.length) % filteredOptions.length;
        scrollHighlightedIntoView();
      }
      return;
    }

    if (e.key === 'Enter') {
      e.preventDefault();
      const chosen = filteredOptions[activeIndex];
      if (chosen) pick(chosen);
    }
  }

  function scrollHighlightedIntoView() {
    queueMicrotask(() => {
      const el = menuEl?.querySelector<HTMLElement>(`[data-idx="${activeIndex}"]`);
      el?.scrollIntoView({ block: 'nearest' });
    });
  }

  $effect(() => {
    if (!open) return;

    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as Node | null;
      if (!target) return;
      if (triggerEl?.contains(target) || menuEl?.contains(target)) return;
      closeMenu();
    };

    const handleScrollOrResize = (e: Event) => {
      if (e.type === 'scroll' && menuEl && e.target instanceof Node && menuEl.contains(e.target)) {
        return;
      }
      updatePosition();
    };

    window.addEventListener('pointerdown', handlePointerDown, true);
    window.addEventListener('resize', handleScrollOrResize);
    window.addEventListener('scroll', handleScrollOrResize, true);
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown, true);
      window.removeEventListener('resize', handleScrollOrResize);
      window.removeEventListener('scroll', handleScrollOrResize, true);
    };
  });

  $effect(() => {
    if (query !== undefined && activeIndex >= filteredOptions.length) {
      activeIndex = 0;
    }
  });
</script>

<div class="cs-root" style:max-width={maxWidth}>
  <button
    type="button"
    class="cs-trigger cs-{size}"
    class:open
    bind:this={triggerEl}
    aria-label={ariaLabel}
    aria-haspopup="listbox"
    aria-expanded={open}
    {title}
    onclick={toggleMenu}
    onkeydown={onKeydown}
  >
    <span class="cs-trigger-main">
      {#if icon}
        <span class="cs-icon" aria-hidden="true">{@render icon()}</span>
      {:else if selectedOption?.avatarUrl !== undefined}
        <Monogram
          src={selectedOption.avatarUrl}
          name={selectedOption.label}
          width={18}
          height={18}
          rounded
        />
      {:else if selectedOption?.dotTone}
        <span class="cs-dot tone-{selectedOption.dotTone}" aria-hidden="true"></span>
      {/if}
      <span class="cs-label truncate">{selectedOption ? selectedOption.label : placeholder}</span>
      {#if selectedOption?.meta}
        <span class="cs-trigger-meta tabular">{selectedOption.meta}</span>
      {/if}
    </span>
    <span class="cs-chevron" class:rotated={open} aria-hidden="true">
      <ChevronDown size={14} />
    </span>
  </button>

  {#if open}
    <div
      class="cs-menu"
      class:upward={openUpward}
      style={menuStyle}
      role="listbox"
      aria-label={ariaLabel}
      bind:this={menuEl}
      onkeydown={onKeydown}
      tabindex="-1"
    >
      {#if searchable}
        <div class="cs-search-wrap">
          <Search size={13} />
          <input
            type="text"
            class="cs-search-input"
            bind:this={searchInputEl}
            bind:value={query}
            placeholder={searchPlaceholder}
            aria-label={searchPlaceholder}
          />
          {#if query}
            <button
              type="button"
              class="cs-search-clear"
              aria-label="Clear search"
              onclick={() => {
                query = '';
                searchInputEl?.focus();
              }}
            >
              <X size={12} />
            </button>
          {/if}
        </div>
      {/if}

      <div class="cs-list">
        {#if filteredOptions.length === 0}
          <div class="cs-empty">No matches for "{query}"</div>
        {:else}
          {#each filteredOptions as opt, idx (opt.value)}
            {@const isSelected = opt.value === value}
            {@const isHighlighted = idx === activeIndex}
            <button
              type="button"
              role="option"
              aria-selected={isSelected}
              data-idx={idx}
              data-active={isSelected ? 'true' : undefined}
              class="cs-option"
              class:selected={isSelected}
              class:highlighted={isHighlighted}
              onmouseenter={() => (activeIndex = idx)}
              onclick={() => pick(opt)}
            >
              <span class="cs-opt-left">
                {#if opt.avatarUrl !== undefined}
                  <span class="cs-avatar-wrap">
                    <Monogram src={opt.avatarUrl} name={opt.label} width={24} height={24} rounded />
                    {#if opt.dotTone}
                      <span class="cs-avatar-dot tone-{opt.dotTone}" aria-hidden="true"></span>
                    {/if}
                  </span>
                {:else if opt.dotTone}
                  <span class="cs-dot tone-{opt.dotTone}" aria-hidden="true"></span>
                {/if}
                <span class="cs-opt-text">
                  <span class="cs-opt-label truncate">{opt.label}</span>
                  {#if opt.subtitle}
                    <span class="cs-opt-sub truncate">{opt.subtitle}</span>
                  {/if}
                </span>
              </span>

              <span class="cs-opt-right">
                {#if opt.meta}
                  <span class="cs-opt-meta tabular">{opt.meta}</span>
                {/if}
                {#if isSelected}
                  <span class="cs-check" aria-hidden="true"><Check size={14} /></span>
                {/if}
              </span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .cs-root {
    position: relative;
    display: inline-flex;
    min-width: 0;
  }

  .cs-trigger {
    width: 100%;
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    font-weight: 600;
    text-align: left;
    cursor: pointer;
    transition:
      border-color var(--duration-fast) var(--ease-out),
      background var(--duration-fast) var(--ease-out),
      box-shadow var(--duration-fast) var(--ease-out);
  }

  .cs-sm {
    height: 30px;
    font-size: var(--text-xs);
  }

  .cs-md {
    height: 34px;
    font-size: var(--text-xs);
  }

  .cs-trigger:hover {
    border-color: var(--border-hover);
    background: var(--surface-hover);
  }

  .cs-trigger.open {
    border-color: var(--accent);
    background: var(--surface-hover);
    box-shadow: 0 0 0 2px var(--accent-tint);
  }

  .cs-trigger-main {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    flex: 1;
  }

  .cs-icon {
    display: inline-flex;
    align-items: center;
    color: var(--accent-text);
    flex-shrink: 0;
  }

  .cs-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cs-trigger-meta {
    flex-shrink: 0;
    padding: 1px 6px;
    border-radius: var(--radius-pill);
    background: var(--neutral-tint);
    color: var(--text-muted);
    font-size: 10.5px;
    font-weight: 700;
  }

  .cs-chevron {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
    transition: transform var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
  }

  .cs-chevron.rotated {
    transform: rotate(180deg);
    color: var(--accent-text);
  }

  .cs-menu {
    position: fixed;
    z-index: 950;
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-hover);
    background: var(--surface);
    box-shadow:
      0 18px 42px rgba(2, 6, 16, 0.72),
      0 0 0 1px rgba(255, 255, 255, 0.04) inset;
    overflow: hidden;
    animation: cs-pop 120ms var(--ease-out);
  }

  .cs-menu.upward {
    animation-name: cs-pop-up;
  }

  @keyframes cs-pop {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.985);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @keyframes cs-pop-up {
    from {
      opacity: 0;
      transform: translateY(4px) scale(0.985);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .cs-search-wrap {
    position: relative;
    display: flex;
    align-items: center;
    padding: 8px 8px 6px;
    border-bottom: 1px solid var(--border);
    color: var(--text-faint);
  }

  .cs-search-wrap :global(svg:first-child) {
    position: absolute;
    left: 17px;
    pointer-events: none;
  }

  .cs-search-input {
    width: 100%;
    height: 30px;
    padding: 0 26px 0 28px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    font-size: var(--text-xs);
    outline: none;
  }

  .cs-search-input:focus {
    border-color: var(--accent);
  }

  .cs-search-clear {
    position: absolute;
    right: 13px;
    width: 20px;
    height: 20px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .cs-search-clear:hover {
    background: var(--surface-hover);
    color: var(--text);
  }

  .cs-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
    max-height: var(--menu-max-h, 290px);
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  .cs-empty {
    padding: 18px 12px;
    text-align: center;
    color: var(--text-faint);
    font-size: var(--text-xs);
  }

  .cs-option {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 7px 9px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
    font-size: var(--text-xs);
    text-align: left;
    cursor: pointer;
    transition:
      background var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out);
  }

  .cs-option.highlighted {
    background: var(--surface-2);
  }

  .cs-option.selected {
    background: var(--accent-tint);
    border-color: rgba(34, 211, 238, 0.25);
  }

  .cs-option.selected .cs-opt-label {
    color: var(--accent-text);
    font-weight: 700;
  }

  .cs-opt-left {
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
    flex: 1;
  }

  .cs-avatar-wrap {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
  }

  .cs-avatar-dot {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1.5px solid var(--surface);
  }

  .cs-opt-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .cs-opt-label {
    font-weight: 600;
    line-height: 1.25;
  }

  .cs-opt-sub {
    font-size: 11px;
    color: var(--text-faint);
    line-height: 1.2;
  }

  .cs-opt-right {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .cs-opt-meta {
    padding: 2px 7px;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
  }

  .cs-option.selected .cs-opt-meta {
    background: rgba(6, 182, 212, 0.14);
    border-color: rgba(34, 211, 238, 0.3);
    color: var(--accent-text);
  }

  .cs-check {
    display: inline-flex;
    align-items: center;
    color: var(--accent-text);
  }

  .cs-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .tone-live {
    background: var(--success);
    box-shadow: 0 0 6px rgba(16, 185, 129, 0.55);
  }

  .tone-offline {
    background: var(--neutral-dot);
  }

  .tone-twitch {
    background: var(--twitch);
  }

  .tone-kick {
    background: var(--kick);
  }

  .tone-accent {
    background: var(--accent-hover);
    box-shadow: 0 0 6px rgba(34, 211, 238, 0.45);
  }

  .tone-info {
    background: var(--info);
  }

  .tone-warn {
    background: var(--warning);
  }

  .tone-error {
    background: var(--error);
  }

  .tone-muted {
    background: var(--neutral-dot);
  }
</style>
