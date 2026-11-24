export function progressPercent(current: number, required: number): number {
  if (required <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((current / required) * 100)));
}

export function formatProgressPercent(current: number, required: number): string {
  if (required <= 0 || current <= 0) return '0%';
  const raw = Math.max(0, Math.min(100, (current / required) * 100));
  if (raw < 1) return `${raw.toFixed(1)}%`;
  return `${Math.round(raw)}%`;
}

export function formatLivePercent(currentFloat: number, required: number): string {
  if (required <= 0 || currentFloat <= 0) return '0%';
  const raw = Math.max(0, Math.min(100, (currentFloat / required) * 100));
  if (raw >= 100) return '100%';
  if (raw < 10) return `${raw.toFixed(1)}%`;
  return `${raw.toFixed(1)}%`;
}

export function formatLiveDuration(minutesFloat: number): string {
  const totalSecs = Math.max(0, Math.floor(minutesFloat * 60));
  const hrs = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  if (hrs > 0) return `${hrs}h ${pad(mins)}m`;
  if (mins > 0) return `${mins}m ${pad(secs)}s`;
  return `${secs}s`;
}

export function formatLiveRemaining(remainingSeconds: number): string {
  const secsTotal = Math.max(0, Math.ceil(remainingSeconds));
  if (secsTotal === 0) return 'Ready to claim';
  const hrs = Math.floor(secsTotal / 3600);
  const mins = Math.floor((secsTotal % 3600) / 60);
  const secs = secsTotal % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  if (hrs > 0) return `${hrs}h ${pad(mins)}m left`;
  if (mins > 0) return `${mins}m ${pad(secs)}s left`;
  return `${secs}s left`;
}

export function formatEtaClock(remainingSeconds: number, now = Date.now()): string {
  if (remainingSeconds <= 0) return 'Now';
  const target = new Date(now + remainingSeconds * 1000);
  return `~${target.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  })}`;
}

export function formatMinutes(mins: number): string {
  if (mins < 60) return `${mins}m`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m === 0 ? `${h}h` : `${h}h ${m}m`;
}

export function formatEta(currentMinutes: number, requiredMinutes: number): string {
  const remaining = Math.max(0, requiredMinutes - currentMinutes);
  if (remaining === 0) return 'Ready to claim';
  return `~${remaining} min left`;
}

export function formatTimeRemaining(isoDate: string, now = Date.now()): string {
  const target = new Date(isoDate).getTime();
  if (Number.isNaN(target)) return '';
  const diffMs = target - now;
  if (diffMs <= 0) return 'Ended';
  const totalSecs = Math.floor(diffMs / 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  if (totalSecs < 86_400) {
    const hrs = Math.floor(totalSecs / 3600);
    const mins = Math.floor((totalSecs % 3600) / 60);
    const secs = totalSecs % 60;
    return `${pad(hrs)}:${pad(mins)}:${pad(secs)}`;
  }
  const days = Math.floor(totalSecs / 86_400);
  const hours = Math.floor((totalSecs % 86_400) / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  return `${days}d ${hours}h ${mins}m`;
}

export function formatEndCountdown(
  isoDate: string,
  now = Date.now(),
): { text: string; tone: 'error' | 'warning' | 'neutral'; ended: boolean } {
  const target = new Date(isoDate).getTime();
  if (Number.isNaN(target)) {
    return { text: '—', tone: 'neutral', ended: false };
  }
  const diffMs = target - now;
  if (diffMs <= 0) {
    return { text: 'Ended', tone: 'error', ended: true };
  }
  const text = formatTimeRemaining(isoDate, now);
  if (diffMs < 6 * 3_600_000) {
    return { text, tone: 'error', ended: false };
  }
  if (diffMs < 24 * 3_600_000) {
    return { text, tone: 'warning', ended: false };
  }
  return { text, tone: 'neutral', ended: false };
}

export function formatExactDateTime(isoDate: string): string {
  const d = new Date(isoDate);
  if (Number.isNaN(d.getTime())) return isoDate;
  return d.toLocaleString([], {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}

export function formatStartsIn(isoDate: string, now = Date.now()): string {
  const target = new Date(isoDate).getTime();
  if (Number.isNaN(target)) return '';
  const diffMs = target - now;
  if (diffMs <= 0) return 'Starting soon';
  const totalMins = Math.max(1, Math.ceil(diffMs / 60_000));
  const days = Math.floor(totalMins / (60 * 24));
  const hours = Math.floor((totalMins % (60 * 24)) / 60);
  const mins = totalMins % 60;
  if (days > 0) return `Starts in ${days}d ${hours}h`;
  if (hours > 0) return `Starts in ${hours}h ${mins}m`;
  return `Starts in ${mins}m`;
}

export function formatCountdown(isoDate: string | null | undefined, now: number): string {
  if (!isoDate) return '—';
  const target = new Date(isoDate).getTime();
  if (Number.isNaN(target)) return '—';
  const totalSecs = Math.max(0, Math.floor((target - now) / 1000));
  const hrs = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  if (hrs > 0) return `${hrs}:${pad(mins)}:${pad(secs)}`;
  return `${mins}:${pad(secs)}`;
}

export function formatViewerCount(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}K`;
  return String(count);
}

export function formatTimestamp(isoDate: string): string {
  const d = new Date(isoDate);
  if (Number.isNaN(d.getTime())) return isoDate;
  return d.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}

export function formatRelativeTime(isoDate: string | number, now = Date.now()): string {
  const ts = typeof isoDate === 'number' ? isoDate : new Date(isoDate).getTime();
  if (Number.isNaN(ts)) return '';
  const diffSec = Math.max(0, Math.floor((now - ts) / 1000));
  if (diffSec < 10) return 'just now';
  if (diffSec < 60) return `${diffSec}s ago`;
  const mins = Math.floor(diffSec / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

export function upgradeArtworkUrl(url: string, width?: number, height?: number): string;
export function upgradeArtworkUrl(
  url: string | null | undefined,
  width?: number,
  height?: number,
): string | null;
export function upgradeArtworkUrl(
  url: string | null | undefined,
  width = 420,
  height = 560,
): string | null {
  if (!url) return null;
  const trimmed = url.trim();
  if (!trimmed) return null;

  let next = trimmed
    .replace(/\{width\}|%7Bwidth%7D/gi, String(width))
    .replace(/\{height\}|%7Bheight%7D/gi, String(height));

  if (/jtvnw\.net|ttv-boxart/i.test(next)) {
    next = next.replace(
      /-\d+x\d+(\.(?:jpg|jpeg|png|webp))(?=$|[?#])/i,
      `-${width}x${height}$1`,
    );
  } else if (/kick\.com/i.test(next)) {
    next = next.replace(
      /-\d+x\d+(\.(?:jpg|jpeg|png|webp))(?=$|[?#])/i,
      `-${width}x${height}$1`,
    );
    try {
      const parsed = new URL(next);
      if (parsed.searchParams.has('w') || parsed.searchParams.has('width')) {
        if (parsed.searchParams.has('w')) parsed.searchParams.set('w', String(width));
        if (parsed.searchParams.has('width')) parsed.searchParams.set('width', String(width));
        if (parsed.searchParams.has('h')) parsed.searchParams.set('h', String(height));
        if (parsed.searchParams.has('height')) parsed.searchParams.set('height', String(height));
        next = parsed.toString();
      }
    } catch {
      // keep non-URL strings untouched
    }
  }

  return next;
}
