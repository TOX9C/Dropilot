<script lang="ts">
  import Badge from '../components/Badge.svelte';
  import LogView from '../components/LogView.svelte';
  import { api } from '../lib/api';
  import { clearLogsView, notifyUi } from '../lib/stores';
  import type { LogEntry } from '../lib/types';

  interface Props {
    logs: LogEntry[];
  }

  let { logs }: Props = $props();

  const counts = $derived.by(() => {
    let info = 0;
    let warn = 0;
    let error = 0;
    for (const l of logs) {
      if (l.level === 'info') info++;
      else if (l.level === 'warn') warn++;
      else if (l.level === 'error') error++;
    }
    return { info, warn, error };
  });

  async function handleCopy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      notifyUi('Copied visible logs to clipboard');
    } catch {
      notifyUi('Could not access clipboard', 'error');
    }
  }

  async function handleOpenFolder() {
    try {
      await api.openLogFolder();
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }
</script>

<div class="page page-logs">
  <header class="page-header">
    <div class="page-title-wrap">
      <h1>Logs</h1>
    </div>

    <div class="page-actions">
      <Badge tone="info">Info {counts.info}</Badge>
      <Badge tone="warning">Warn {counts.warn}</Badge>
      <Badge tone="error">Error {counts.error}</Badge>
      <Badge>{logs.length} / 1000</Badge>
    </div>
  </header>

  <LogView
    {logs}
    onCopy={handleCopy}
    onOpenFolder={handleOpenFolder}
    onClearView={clearLogsView}
  />
</div>

<style>
  .page-logs {
    min-height: calc(100vh - 64px);
  }
</style>
