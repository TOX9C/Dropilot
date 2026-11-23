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
    <span class="plat-dot dot-{platform}" aria-hidden="true"></span>
    {#if account.avatarUrl}
      <img class="avatar" src={account.avatarUrl} alt="" />
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
    gap: 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 2px 4px 2px 10px;
  }

  .avatar {
    width: 20px;
    height: 20px;
    border-radius: 999px;
    object-fit: cover;
  }

  .name {
    font-size: var(--text-sm);
    font-weight: 600;
  }
</style>
