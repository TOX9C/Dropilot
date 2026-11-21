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
    padding: 2px 8px;
    border-radius: var(--radius-pill);
    font-size: 11px;
    font-weight: 600;
    line-height: 1.35;
    white-space: nowrap;
  }

  .tone-neutral {
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .tone-accent {
    background: var(--accent-tint);
    color: var(--accent-text);
  }
  .tone-twitch {
    background: var(--twitch-tint);
    color: var(--twitch-text);
  }
  .tone-kick {
    background: var(--kick-tint);
    color: var(--kick-text);
  }
  .tone-pink {
    background: var(--pink-tint);
    color: var(--pink-text);
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
