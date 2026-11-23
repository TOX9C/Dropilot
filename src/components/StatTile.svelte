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

  let { label, value, hint, tone: _tone = 'accent', icon, extra }: Props = $props();
</script>

<div class="card stat-tile">
  <div class="stat-top">
    <span class="stat-label">{label}</span>
    {#if icon}
      <span class="stat-icon" aria-hidden="true">{@render icon()}</span>
    {/if}
  </div>
  <div class="stat-mid">
    <span class="stat-val tabular">{value}</span>
    {#if extra}
      {@render extra()}
    {/if}
  </div>
  {#if hint}
    <span class="stat-hint tabular">{hint}</span>
  {/if}
</div>

<style>
  /* Zero tinted backgrounds, zero colored borders, zero rainbow numbers */
  .stat-tile {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 10px;
  }

  .stat-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .stat-label {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .stat-icon {
    color: var(--text-faint);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .stat-mid {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 8px;
  }

  .stat-val {
    font-family: var(--font-mono);
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.02em;
    line-height: 1.1;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stat-hint {
    font-size: 10.5px;
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
