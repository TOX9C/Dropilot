<script lang="ts">
  import X from 'lucide-svelte/icons/x';

  interface Props {
    open: boolean;
    src: string | null;
    title: string;
    subtitle?: string;
    onclose: () => void;
  }

  let { open, src, title, subtitle, onclose }: Props = $props();

  let imgFailed = $state(false);
  let closeBtnEl = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    void src;
    imgFailed = false;
  });

  $effect(() => {
    if (open) {
      queueMicrotask(() => closeBtnEl?.focus());
    }
  });

  function handleKeydown(ev: KeyboardEvent) {
    if (ev.key === 'Escape' && open) {
      ev.preventDefault();
      onclose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open && src}
  <div class="lightbox-scrim" role="presentation" onclick={onclose}>
    <div
      class="lightbox-frame"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="lightbox-stage">
        {#if !imgFailed}
          <img
            class="lightbox-img"
            {src}
            alt={title}
            decoding="async"
            onerror={() => (imgFailed = true)}
          />
        {:else}
          <div class="lightbox-fallback">
            <span>Unable to load artwork preview.</span>
          </div>
        {/if}

        <button
          bind:this={closeBtnEl}
          type="button"
          class="lightbox-close"
          aria-label="Close image preview"
          title="Close (Esc)"
          onclick={onclose}
        >
          <X size={15} />
        </button>
      </div>

      <div class="lightbox-caption">
        <div class="caption-text">
          <strong class="caption-title">{title}</strong>
          {#if subtitle}
            <span class="caption-sub">{subtitle}</span>
          {/if}
        </div>
        <span class="caption-hint tabular">ESC</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .lightbox-scrim {
    position: fixed;
    inset: 0;
    z-index: 160;
    background: rgba(5, 8, 12, 0.86);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-4);
  }

  .lightbox-frame {
    max-width: calc(90vw + 26px);
    max-height: 94vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .lightbox-stage {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--sidebar-bg);
    padding: var(--space-3);
    min-width: 240px;
    min-height: 180px;
  }

  .lightbox-img {
    display: block;
    max-width: 90vw;
    max-height: 82vh;
    width: auto;
    height: auto;
    object-fit: contain;
    border-radius: var(--radius-xs);
  }

  .lightbox-fallback {
    padding: 36px 48px;
    color: var(--text-muted);
    font-size: var(--text-sm);
    text-align: center;
  }

  .lightbox-close {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .lightbox-close:hover {
    border-color: var(--accent);
    color: var(--accent-text);
    background: var(--surface-2);
  }

  .lightbox-caption {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: 9px 12px;
    background: var(--surface);
    border-top: 1px solid var(--border);
  }

  .caption-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .caption-title {
    font-family: var(--font-display);
    font-size: var(--text-md);
    font-weight: 700;
    color: var(--text);
    line-height: 1.3;
  }

  .caption-sub {
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.35;
  }

  .caption-hint {
    flex-shrink: 0;
    padding: 2px 6px;
    border-radius: var(--radius-xs);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-faint);
    font-size: 10px;
    font-weight: 600;
  }

  @media (prefers-reduced-motion: no-preference) {
    .lightbox-scrim {
      animation: lb-fade-in var(--duration-fast) var(--ease-out);
    }

    .lightbox-frame {
      animation: lb-pop-in var(--duration-fast) var(--ease-out);
    }
  }

  @keyframes lb-fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes lb-pop-in {
    from {
      opacity: 0;
      transform: scale(0.985);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }
</style>
