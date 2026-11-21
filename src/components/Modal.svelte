<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    open: boolean;
    title: string;
    blocking?: boolean;
    onclose?: () => void;
    children?: Snippet;
    footer?: Snippet;
  }

  let { open, title, blocking = false, onclose, children, footer }: Props = $props();

  function handleKeydown(ev: KeyboardEvent) {
    if (ev.key === 'Escape' && open && !blocking && onclose) {
      onclose();
    }
  }

  function handleBackdropClick() {
    if (!blocking && onclose) {
      onclose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div class="backdrop" role="presentation" onclick={handleBackdropClick}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="modal-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="dialog-header">
        <h2 id="modal-title">{title}</h2>
      </div>
      <div class="dialog-body">
        {@render children?.()}
      </div>
      {#if footer}
        <div class="dialog-footer">
          {@render footer()}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: var(--backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-6);
  }

  .dialog {
    width: 100%;
    max-width: 500px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--space-6);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  @media (prefers-reduced-motion: no-preference) {
    .backdrop {
      animation: fade-in var(--duration-base) var(--ease-out);
    }

    .dialog {
      animation: modal-in var(--duration-base) var(--ease-out);
    }
  }

  .dialog-header h2 {
    font-size: var(--text-md);
    font-weight: 600;
  }

  .dialog-body {
    font-size: var(--text-base);
    color: var(--text-muted);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .dialog-footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding-top: var(--space-2);
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes modal-in {
    from {
      opacity: 0;
      transform: scale(0.98);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }
</style>
