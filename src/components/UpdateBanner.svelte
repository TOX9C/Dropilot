<script lang="ts">
  import ArrowDownToLine from 'lucide-svelte/icons/arrow-down-to-line';
  import Download from 'lucide-svelte/icons/download';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import X from 'lucide-svelte/icons/x';
  import { api } from '../lib/api';
  import {
    installAppUpdate,
    installingUpdate,
    updateBannerDismissed,
    updateInfo,
    updateProgress,
  } from '../lib/stores';
  import Badge from './Badge.svelte';
  import Button from './Button.svelte';

  function formatMb(bytes: number): string {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  const visible = $derived(
    Boolean($updateInfo?.available && (!$updateBannerDismissed || $installingUpdate)),
  );
</script>

{#if visible && $updateInfo}
  <aside class="update-toast card" aria-live="polite" aria-label="Application update available">
    <div class="ut-head">
      <div class="ut-title-row">
        <span class="ut-ico" aria-hidden="true"><ArrowDownToLine size={15} /></span>
        <div>
          <div class="ut-title">
            <strong>{$installingUpdate ? 'Installing update' : 'Update available'}</strong>
            <Badge tone="accent">v{$updateInfo.latestVersion}</Badge>
          </div>
          <span class="ut-sub tabular">Current: v{$updateInfo.currentVersion}</span>
        </div>
      </div>

      {#if !$installingUpdate}
        <button
          type="button"
          class="ut-close"
          onclick={() => updateBannerDismissed.set(true)}
          aria-label="Dismiss update notification"
        >
          <X size={14} />
        </button>
      {/if}
    </div>

    {#if $updateInfo.notes && !$installingUpdate}
      <p class="ut-notes">{$updateInfo.notes}</p>
    {/if}

    {#if $installingUpdate && $updateProgress}
      <div class="ut-progress">
        <div class="ut-prog-row tabular">
          <span>
            {#if $updateProgress.stage === 'downloading'}
              Downloading signed update…
            {:else if $updateProgress.stage === 'installing'}
              Verifying signature &amp; installing…
            {:else if $updateProgress.stage === 'restarting'}
              Restarting Dropilot…
            {:else}
              Preparing update…
            {/if}
          </span>
          {#if $updateProgress.percent !== null}
            <span>{$updateProgress.percent}%</span>
          {:else if $updateProgress.downloadedBytes > 0}
            <span>{formatMb($updateProgress.downloadedBytes)}</span>
          {/if}
        </div>
        <div class="ut-bar-track">
          <div
            class="ut-bar-fill"
            style={$updateProgress.percent !== null ? `width: ${$updateProgress.percent}%` : 'width: 40%'}
          ></div>
        </div>
      </div>
    {:else}
      <div class="ut-actions">
        {#if $updateInfo.signedUpdateReady}
          <Button variant="primary" size="sm" onclick={() => void installAppUpdate()}>
            <RefreshCw size={13} />
            <span>Update &amp; Restart</span>
          </Button>
        {:else}
          <Button variant="primary" size="sm" onclick={() => void api.openUrl($updateInfo.releaseUrl)}>
            <Download size={13} />
            <span>Download v{$updateInfo.latestVersion}</span>
          </Button>
        {/if}
        <Button variant="ghost" size="sm" onclick={() => updateBannerDismissed.set(true)}>
          Later
        </Button>
      </div>
    {/if}
  </aside>
{/if}

<style>
  .update-toast {
    position: fixed;
    right: 20px;
    bottom: 64px;
    width: 332px;
    max-width: calc(100vw - 32px);
    z-index: 95;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-color: var(--border-hover);
    box-shadow: var(--popover-shadow);
  }

  .ut-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }

  .ut-title-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .ut-ico {
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm);
    background: var(--accent-tint);
    color: var(--accent-text);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .ut-title {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
  }

  .ut-sub {
    font-size: 11px;
    color: var(--text-muted);
  }

  .ut-close {
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    padding: 4px;
    border-radius: var(--radius-xs);
    display: inline-flex;
  }

  .ut-close:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .ut-notes {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.45;
    max-height: 76px;
    overflow-y: auto;
    white-space: pre-line;
  }

  .ut-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .ut-progress {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .ut-prog-row {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--text-muted);
  }

  .ut-bar-track {
    height: 6px;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    overflow: hidden;
  }

  .ut-bar-fill {
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--accent);
    transition: width 160ms linear;
  }
</style>
