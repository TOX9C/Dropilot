<script lang="ts">
  import LogIn from 'lucide-svelte/icons/log-in';
  import type { Account, PlatformId } from '../lib/types';
  import Button from './Button.svelte';

  interface Props {
    platform: PlatformId;
    account: Account | null;
    onlogin: () => void;
    onlogout: () => void;
  }

  let { platform, account, onlogin, onlogout }: Props = $props();
</script>

{#if account}
  <div class="chip">
    {#if account.avatarUrl}
      <img class="avatar" src={account.avatarUrl} alt="" />
    {:else}
      <div class="avatar placeholder plat-{platform}" aria-hidden="true">
        {account.displayName.slice(0, 1).toUpperCase()}
      </div>
    {/if}
    <span class="name">{account.displayName}</span>
    <Button variant="ghost" size="sm" onclick={onlogout} ariaLabel="Log out">
      <span>Log out</span>
    </Button>
  </div>
{:else}
  <Button variant="primary" size="sm" onclick={onlogin}>
    <LogIn size={14} />
    <span>Log in</span>
  </Button>
{/if}

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    padding: 3px 4px 3px 6px;
  }

  .avatar {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-pill);
    object-fit: cover;
  }

  .placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .plat-twitch {
    background: var(--twitch-tint);
    color: var(--twitch-text);
  }

  .plat-kick {
    background: var(--kick);
    color: var(--kick-on);
  }

  .name {
    font-size: var(--text-sm);
    font-weight: 600;
    padding-right: 4px;
  }
</style>
