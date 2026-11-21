<script lang="ts">
  import AlertTriangle from 'lucide-svelte/icons/alert-triangle';
  import Button from './Button.svelte';
  import Modal from './Modal.svelte';

  interface Props {
    open: boolean;
    onAccept: () => void;
  }

  let { open, onAccept }: Props = $props();
</script>

<Modal {open} title="Before you start using DropPilot" blocking>
  <div class="warn-head">
    <AlertTriangle size={20} />
    <span>Platform Terms &amp; Account Responsibility</span>
  </div>
  <p>
    DropPilot automates drop progress tracking and reward claiming for your own logged-in Twitch
    and Kick accounts using standard web sessions.
  </p>
  <p>
    Automated stream watching or claiming may conflict with each platform's Terms of Service. You
    are solely responsible for your accounts and how you choose to use DropPilot. DropPilot is not
    affiliated with or endorsed by Twitch Interactive, Inc. or Kick Streaming Pty Ltd.
  </p>
  <p class="note">
    Mining stays paused until you acknowledge this notice. Your tokens remain stored only in
    Windows Credential Manager.
  </p>

  {#snippet footer()}
    <Button variant="primary" size="md" onclick={onAccept}>
      I understand, continue
    </Button>
  {/snippet}
</Modal>

<style>
  .warn-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--warning-text);
    font-weight: 600;
  }

  p {
    line-height: 1.5;
  }

  .note {
    font-size: var(--text-sm);
    color: var(--text-faint);
  }
</style>
