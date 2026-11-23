<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    tone?: 'neutral' | 'accent' | 'twitch' | 'kick' | 'pink' | 'success' | 'warning' | 'error' | 'info';
    children?: Snippet;
  }

  let { tone = 'neutral', children }: Props = $props();
</script>

<span class="badge tone-{tone} tabular">
  {@render children?.()}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px;
    border-radius: var(--radius-pill);
    font-size: 11px;
    font-weight: 600;
    line-height: 1.35;
    white-space: nowrap;
    border: 1px solid transparent;
  }

  :global(:root[data-design="console"]) .badge {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 1px 5px;
  }

  .tone-neutral {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--text-muted);
  }

  .tone-accent,
  .tone-pink {
    background: var(--accent-tint);
    color: var(--accent-text);
  }

  /* Platform badges: restrained small identifiers only */
  .tone-twitch {
    background: var(--twitch-tint);
    color: var(--twitch-text);
  }

  .tone-kick {
    background: var(--kick-tint);
    color: var(--kick-text);
  }

  .tone-success {
    background: var(--success-tint);
    color: var(--success-text);
  }

  .tone-warning {
    background: var(--warning-tint);
    color: var(--warning-text);
  }

  .tone-error {
    background: var(--error-tint);
    color: var(--error-text);
  }

  .tone-info {
    background: var(--info-tint);
    color: var(--info-text);
  }
</style>
