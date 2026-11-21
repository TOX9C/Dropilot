<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    value: string | number;
    hint?: string;
    tone?: 'accent' | 'twitch' | 'kick' | 'success';
    icon?: Snippet;
    extra?: Snippet;
  }

  let { label, value, hint, tone = 'accent', icon, extra }: Props = $props();
</script>

<div class="card stat-tile tile-{tone}">
  <div class="stat-top">
    <span class="section-label">{label}</span>
    {#if icon}
      <span class="stat-icon tone-{tone}" aria-hidden="true">{@render icon()}</span>
    {/if}
  </div>
  <div class="stat-mid">
    <span class="stat-val tabular val-{tone}">{value}</span>
    {#if extra}
      {@render extra()}
    {/if}
  </div>
  {#if hint}
    <span class="stat-hint tabular">{hint}</span>
  {/if}
</div>

<style>
  .stat-tile {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: var(--space-4);
    border-top: 2px solid transparent;
  }

  .tile-twitch {
    background: var(--tile-twitch-bg);
    border-top-color: var(--twitch);
  }
  .tile-twitch:hover {
    border-color: rgba(168, 85, 247, 0.42);
  }

  .tile-accent {
    background: var(--tile-accent-bg);
    border-top-color: var(--cyan);
  }
  .tile-accent:hover {
    border-color: rgba(56, 189, 248, 0.42);
  }

  .tile-success {
    background: var(--tile-success-bg);
    border-top-color: var(--success);
  }
  .tile-success:hover {
    border-color: rgba(52, 211, 153, 0.42);
  }

  .tile-kick {
    background: var(--tile-kick-bg);
    border-top-color: var(--kick);
  }
  .tile-kick:hover {
    border-color: rgba(83, 252, 24, 0.4);
  }

  .stat-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .stat-icon {
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border: 1px solid var(--border);
  }

  .tone-accent {
    background: var(--cyan-tint);
    border-color: rgba(56, 189, 248, 0.34);
    color: var(--cyan-text);
  }
  .tone-twitch {
    background: var(--twitch-tint);
    border-color: rgba(168, 85, 247, 0.36);
    color: var(--twitch-text);
  }
  .tone-kick {
    background: var(--kick-tint);
    border-color: rgba(83, 252, 24, 0.34);
    color: var(--kick-text);
  }
  .tone-success {
    background: var(--success-tint);
    border-color: rgba(52, 211, 153, 0.34);
    color: var(--success-text);
  }

  .stat-mid {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .stat-val {
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.08;
  }

  .val-twitch { color: var(--twitch-text); }
  .val-accent { color: var(--cyan-text); }
  .val-success { color: var(--success-text); }
  .val-kick { color: var(--kick-text); }

  .stat-hint {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
</style>
