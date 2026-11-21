<script lang="ts">
  import X from 'lucide-svelte/icons/x';
  import { dismissToast, uiToasts } from '../lib/stores';
</script>

{#if $uiToasts.length > 0}
  <div class="toast-stack" aria-live="polite">
    {#each $uiToasts as toast (toast.id)}
      <div class="toast toast-{toast.kind}" role="status">
        <span class="toast-msg">{toast.message}</span>
        <button
          type="button"
          class="toast-close"
          aria-label="Dismiss notification"
          onclick={() => dismissToast(toast.id)}
        >
          <X size={13} />
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-stack {
    position: fixed;
    right: var(--space-6);
    bottom: var(--space-6);
    z-index: 150;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 360px;
  }

  .toast {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 600;
    box-shadow: var(--card-shadow);
  }

  .toast-error {
    border-color: rgba(255, 93, 93, 0.45);
    color: var(--error-text);
  }

  .toast-close {
    border: none;
    background: transparent;
    color: var(--text-muted);
    padding: 4px;
    border-radius: var(--radius-sm);
    display: flex;
  }

  .toast-close:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  @media (prefers-reduced-motion: no-preference) {
    .toast {
      animation: toast-in var(--duration-base) var(--ease-out);
    }

    @keyframes toast-in {
      from {
        opacity: 0;
        transform: translateY(6px);
      }
      to {
        opacity: 1;
        transform: translateY(0);
      }
    }
  }
</style>
