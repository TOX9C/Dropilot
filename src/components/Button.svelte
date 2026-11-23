<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'twitch' | 'kick';
    size?: 'sm' | 'md';
    disabled?: boolean;
    type?: 'button' | 'submit';
    ariaLabel?: string;
    title?: string;
    onclick?: (ev: MouseEvent) => void;
    children?: Snippet;
  }

  let {
    variant = 'secondary',
    size = 'sm',
    disabled = false,
    type = 'button',
    ariaLabel,
    title,
    onclick,
    children,
  }: Props = $props();
</script>

<button
  {type}
  class="btn btn-{variant} btn-{size}"
  {disabled}
  aria-label={ariaLabel}
  {title}
  {onclick}
>
  {@render children?.()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid transparent;
    font-family: var(--font-mono);
    font-weight: 500;
    line-height: 1;
    white-space: nowrap;
  }

  .btn-sm {
    height: var(--btn-sm-h, 32px);
    padding: 0 11px;
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
  }

  .btn-md {
    height: var(--btn-md-h, 36px);
    padding: 0 15px;
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
  }

  .btn-primary {
    background: var(--accent);
    color: var(--accent-on);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .btn-secondary {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--text);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--surface-hover);
    border-color: var(--border-hover);
  }

  .btn-ghost {
    background: transparent;
    color: var(--text-muted);
  }

  .btn-ghost:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }

  .btn-danger {
    background: var(--error-tint);
    border-color: var(--error);
    color: var(--error-text);
  }

  .btn-danger:hover:not(:disabled) {
    background: var(--error);
    color: #ffffff;
  }

  /* Platform login actions stay crisp and restrained — never neon gradient blocks */
  .btn-twitch,
  .btn-kick {
    background: var(--surface-2);
    border-color: var(--border-hover);
    color: var(--text);
  }

  .btn-twitch:hover:not(:disabled),
  .btn-kick:hover:not(:disabled) {
    background: var(--surface-hover);
    border-color: var(--accent);
  }

  .btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  @media (prefers-reduced-motion: no-preference) {
    .btn {
      transition:
        background-color var(--duration-fast) var(--ease-out),
        border-color var(--duration-fast) var(--ease-out),
        color var(--duration-fast) var(--ease-out);
    }
  }
</style>
