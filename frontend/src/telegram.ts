// Thin, optional bridge to Telegram Mini Apps. Nothing here is required for the app to work:
// in a desktop browser `window.Telegram` is undefined and every call is a no-op.

type TgWebApp = {
  ready: () => void
  expand: () => void
  initData?: string
  viewportStableHeight?: number
  onEvent?: (ev: string, cb: () => void) => void
  setHeaderColor?: (c: string) => void
  setBackgroundColor?: (c: string) => void
  enableClosingConfirmation?: () => void
  initDataUnsafe?: { user?: { first_name?: string; username?: string } }
  platform?: string
  HapticFeedback?: { impactOccurred: (s: 'light' | 'medium' | 'heavy') => void }
}

declare global {
  interface Window {
    Telegram?: { WebApp?: TgWebApp }
  }
}

export function tg(): TgWebApp | undefined {
  return window.Telegram?.WebApp
}

export function isTelegram(): boolean {
  const w = tg()
  return Boolean(w && w.platform && w.platform !== 'unknown')
}

/** Raw signed initData — sent as `Authorization: tma …` so the backend can pick the user's room. */
export function initData(): string {
  return tg()?.initData ?? ''
}

export function tgUserName(): string | null {
  const u = tg()?.initDataUnsafe?.user
  return u?.first_name ?? u?.username ?? null
}

function applyViewport() {
  const h = tg()?.viewportStableHeight
  document.documentElement.style.setProperty('--app-height', h && h > 200 ? `${h}px` : '100dvh')
}

export function initTelegram() {
  const w = tg()
  if (!w) {
    document.documentElement.style.setProperty('--app-height', '100dvh')
    return
  }
  try {
    w.ready()
    w.expand()
    w.setHeaderColor?.('#0D0E12')
    w.setBackgroundColor?.('#0D0E12')
    w.enableClosingConfirmation?.()
    applyViewport()
    w.onEvent?.('viewportChanged', applyViewport)
  } catch {
    /* older clients */
  }
}

export function haptic(style: 'light' | 'medium' | 'heavy' = 'light') {
  try {
    tg()?.HapticFeedback?.impactOccurred(style)
  } catch {
    /* ignore */
  }
}
