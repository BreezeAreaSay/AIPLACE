// Thin, optional bridge to Telegram Mini Apps. Nothing here is required for the app to work:
// in a desktop browser `window.Telegram` is undefined and every call is a no-op.

type TgWebApp = {
  ready: () => void
  expand: () => void
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

export function initTelegram() {
  const w = tg()
  if (!w) return
  try {
    w.ready()
    w.expand()
    w.setHeaderColor?.('#0D0E12')
    w.setBackgroundColor?.('#0D0E12')
    w.enableClosingConfirmation?.()
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
