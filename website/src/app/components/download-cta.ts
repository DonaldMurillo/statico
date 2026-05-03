import { Component, computed, inject, signal } from '@angular/core';
import { isPlatformBrowser } from '@angular/common';
import { PLATFORM_ID } from '@angular/core';

/**
 * Detected target triple matching the release tarball naming
 * (`statico-{os}-{arch}.tar.gz`). `null` means "unknown / SSR" — the
 * CTA falls back to the generic "Download" link to the releases page.
 */
type Target =
  | 'macos-aarch64'
  | 'macos-x86_64'
  | 'linux-x86_64'
  | 'linux-aarch64'
  | 'windows-x86_64'
  | null;

const RELEASE_BASE = 'https://github.com/DonaldMurillo/statico/releases';

interface ResolvedTarget {
  /** Target triple, or null if we can't tell. */
  target: Target;
  /** Human-readable label for the button (e.g. "macOS · Apple Silicon"). */
  label: string;
  /** Direct download URL to the latest tarball, or releases page if unknown. */
  url: string;
  /** Pre-built `curl | tar` install snippet to copy. */
  installCmd: string;
}

@Component({
  selector: 'app-download-cta',
  standalone: true,
  template: `
    <div class="download-cta">
      <a
        class="cmd-btn cmd-btn-primary download-btn"
        [href]="resolved().url"
        rel="noopener"
        [attr.aria-label]="'Download statico for ' + resolved().label"
      >
        <span class="cmd-prompt" aria-hidden="true">↓</span>
        <span class="download-text">Download for <strong>{{ resolved().label }}</strong></span>
      </a>

      @if (resolved().target) {
        <button
          type="button"
          class="install-cmd"
          [attr.aria-label]="copied() ? 'Install command copied' : 'Copy install command'"
          (click)="copyInstall()"
        >
          <code>{{ resolved().installCmd }}</code>
          <span class="copy-icon" aria-hidden="true">{{ copied() ? '✓' : '⧉' }}</span>
        </button>
      } @else {
        <a class="install-cmd-fallback" [href]="otherPlatformsUrl">
          Other platforms →
        </a>
      }
    </div>
  `,
  styles: [
    `
      .download-cta {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: var(--sp-2);
      }

      .download-btn {
        min-width: 18rem;
      }

      .download-text strong {
        font-weight: 600;
      }

      .install-cmd,
      .install-cmd-fallback {
        font-family: var(--font-mono, monospace);
        font-size: 0.78rem;
        color: oklch(0.55 0.02 260);
        background: oklch(0.16 0.01 260);
        border: 1px solid oklch(0.22 0.01 260);
        padding: var(--sp-2) var(--sp-3);
        border-radius: 4px;
        max-width: min(560px, calc(100vw - 2rem));
        display: inline-flex;
        align-items: center;
        gap: var(--sp-2);
        cursor: pointer;
        transition: border-color 120ms ease;
      }

      .install-cmd:hover,
      .install-cmd-fallback:hover {
        border-color: oklch(0.40 0.05 260);
        color: oklch(0.75 0.01 260);
      }

      .install-cmd code {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .copy-icon {
        flex-shrink: 0;
        opacity: 0.7;
      }

      .install-cmd-fallback {
        text-decoration: none;
      }
    `,
  ],
})
export class DownloadCtaComponent {
  /** True after we've detected the platform on the client. SSR initially renders the fallback so the static HTML is platform-neutral. */
  private detected = signal<Target>(null);
  copied = signal<boolean>(false);
  readonly otherPlatformsUrl = `${RELEASE_BASE}/latest`;

  resolved = computed<ResolvedTarget>(() => resolveTarget(this.detected()));

  private readonly platformId = inject(PLATFORM_ID);

  constructor() {
    if (isPlatformBrowser(this.platformId)) {
      // Detect on the client only — SSR has no useful navigator and we
      // want the prerendered HTML to look the same for everyone.
      detectTarget().then((t) => this.detected.set(t));
    }
  }

  async copyInstall(): Promise<void> {
    const cmd = this.resolved().installCmd;
    if (!cmd || typeof navigator === 'undefined' || !navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(cmd);
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 1800);
    } catch {
      // Clipboard API can fail under tracking-prevention rules; keep silent.
    }
  }
}

// ─── Detection + resolution helpers (pure, exported for unit tests) ──────────

interface UserAgentDataLike {
  platform?: string;
  getHighEntropyValues?: (hints: string[]) => Promise<{ architecture?: string; bitness?: string }>;
}

/**
 * Best-effort OS + arch detection. Uses `navigator.userAgentData` when
 * available (Chrome/Edge), falls back to the legacy `userAgent` string.
 *
 * Returns `null` if we can't confidently identify a target — the UI then
 * surfaces the generic "Other platforms" link instead of a wrong tarball.
 */
export async function detectTarget(): Promise<Target> {
  if (typeof navigator === 'undefined') return null;

  const uaData = (navigator as unknown as { userAgentData?: UserAgentDataLike }).userAgentData;
  if (uaData?.platform && uaData.getHighEntropyValues) {
    try {
      const hints = await uaData.getHighEntropyValues(['architecture', 'bitness']);
      const platform = uaData.platform;
      const arch = hints.architecture; // 'arm' | 'x86' | …
      if (platform === 'macOS') return arch === 'arm' ? 'macos-aarch64' : 'macos-x86_64';
      if (platform === 'Linux') return arch === 'arm' ? 'linux-aarch64' : 'linux-x86_64';
      if (platform === 'Windows') return 'windows-x86_64';
    } catch {
      // Fall through to UA sniffing below.
    }
  }

  const ua = navigator.userAgent ?? '';

  if (/Mac OS X|Macintosh/i.test(ua)) {
    // Reliable Apple-Silicon-vs-Intel detection from a vanilla user agent
    // is impossible (Safari intentionally lies). Most macs sold since 2020
    // are Apple Silicon, so default to that.
    return 'macos-aarch64';
  }
  if (/Linux/i.test(ua) || /X11/i.test(ua)) {
    if (/aarch64|arm64/i.test(ua)) return 'linux-aarch64';
    return 'linux-x86_64';
  }
  if (/Windows/i.test(ua)) {
    return 'windows-x86_64';
  }
  return null;
}

/** Map a detected target to button label + URLs. */
export function resolveTarget(target: Target): ResolvedTarget {
  switch (target) {
    case 'macos-aarch64':
      return mac('Apple Silicon', 'macos-aarch64');
    case 'macos-x86_64':
      return mac('Intel', 'macos-x86_64');
    case 'linux-aarch64':
      return linux('aarch64', 'linux-aarch64');
    case 'linux-x86_64':
      return linux('x86_64', 'linux-x86_64');
    case 'windows-x86_64':
      // Windows tarballs may not exist yet — point to releases page for the
      // honest answer. The button will say "Download for Windows" and lead
      // somewhere users can pick. Updated once windows-x86_64 ships.
      return {
        target: 'windows-x86_64',
        label: 'Windows · x86_64',
        url: `${RELEASE_BASE}/latest`,
        installCmd: 'See the releases page for Windows install instructions.',
      };
    default:
      return {
        target: null,
        label: 'your platform',
        url: `${RELEASE_BASE}/latest`,
        installCmd: '',
      };
  }
}

function mac(arch: string, slug: 'macos-aarch64' | 'macos-x86_64'): ResolvedTarget {
  return {
    target: slug,
    label: `macOS · ${arch}`,
    url: `${RELEASE_BASE}/latest/download/statico-${slug}.tar.gz`,
    installCmd: `curl -fsSL ${RELEASE_BASE}/latest/download/statico-${slug}.tar.gz | tar -xz && sudo install -m 0755 statico /usr/local/bin/statico`,
  };
}

function linux(arch: string, slug: 'linux-aarch64' | 'linux-x86_64'): ResolvedTarget {
  return {
    target: slug,
    label: `Linux · ${arch}`,
    url: `${RELEASE_BASE}/latest/download/statico-${slug}.tar.gz`,
    installCmd: `curl -fsSL ${RELEASE_BASE}/latest/download/statico-${slug}.tar.gz | tar -xz && sudo install -m 0755 statico /usr/local/bin/statico`,
  };
}
