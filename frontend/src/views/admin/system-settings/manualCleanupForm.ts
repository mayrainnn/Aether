export const MANUAL_USAGE_CLEANUP_CONFIRM_PHRASE = '确认清理'

export function normalizeConfirmPhraseInput(raw: string): string {
  return raw.replace(/\r?\n/g, '').trim()
}

export function isConfirmPhraseMatched(raw: string): boolean {
  return normalizeConfirmPhraseInput(raw) === MANUAL_USAGE_CLEANUP_CONFIRM_PHRASE
}

export function normalizeOlderThanDaysInput(raw: string | number | null | undefined): number | null {
  if (raw === null || raw === undefined || raw === '') return null
  const parsed = typeof raw === 'number' ? raw : Number(raw)
  if (!Number.isFinite(parsed) || parsed <= 0) return null
  return Math.floor(parsed)
}
