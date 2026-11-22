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
    font-weight: 600;
    line-height: 1;
    white-space: nowrap;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out),
      transform var(--duration-fast) var(--ease-out),
      opacity var(--duration-fast) var(--ease-out);
  }

  .btn-sm {
    height: 32px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
  }

  .btn-md {
    height: 36px;
    padding: 0 var(--space-4);
    border-radius: var(--radius-md);
    font-size: var(--text-base);
  }

  .btn-primary {
    background: var(--grad-accent);
    color: var(--accent-on);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.28);
  }
  .btn-primary:hover:not(:disabled) {
    background: var(--grad-accent-hover);
    transform: translateY(-1px);
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
    border-color: rgba(255, 93, 93, 0.35);
    color: var(--error-text);
  }
  .btn-danger:hover:not(:disabled) {
    background: rgba(255, 93, 93, 0.24);
  }

  .btn-twitch {
    background: var(--grad-twitch);
    color: var(--twitch-on);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.22);
  }
  .btn-twitch:hover:not(:disabled) {
    filter: brightness(1.08);
    transform: translateY(-1px);
  }

  .btn-kick {
    background: var(--grad-kick);
    color: var(--kick-on);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.3);
  }
  .btn-kick:hover:not(:disabled) {
    filter: brightness(1.05);
    transform: translateY(-1px);
  }

  .btn:disabled {
    opacity: 0.48;
    cursor: not-allowed;
  }

  @media (prefers-reduced-motion: reduce) {
    .btn {
      transition: none;
    }
    .btn-primary:hover:not(:disabled) {
      transform: none;
    }
  }
</style>
