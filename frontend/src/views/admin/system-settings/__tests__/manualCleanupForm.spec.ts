import { describe, expect, it } from 'vitest'

import {
  MANUAL_USAGE_CLEANUP_CONFIRM_PHRASE,
  isConfirmPhraseMatched,
  normalizeConfirmPhraseInput,
  normalizeOlderThanDaysInput,
} from '../manualCleanupForm'

describe('manualCleanupForm', () => {
  describe('normalizeConfirmPhraseInput', () => {
    it('trims leading and trailing whitespace', () => {
      expect(normalizeConfirmPhraseInput('  确认清理  ')).toBe(MANUAL_USAGE_CLEANUP_CONFIRM_PHRASE)
    })

    it('strips newlines typed or pasted into the input', () => {
      expect(normalizeConfirmPhraseInput('确认清理\n')).toBe(MANUAL_USAGE_CLEANUP_CONFIRM_PHRASE)
      expect(normalizeConfirmPhraseInput('确认\n清理')).toBe('确认清理')
    })

    it('leaves non-whitespace content untouched', () => {
      expect(normalizeConfirmPhraseInput('取消')).toBe('取消')
    })
  })

  describe('isConfirmPhraseMatched', () => {
    it('matches the exact phrase including pasted whitespace', () => {
      expect(isConfirmPhraseMatched('确认清理')).toBe(true)
      expect(isConfirmPhraseMatched(' 确认清理 ')).toBe(true)
      expect(isConfirmPhraseMatched('确认清理\n')).toBe(true)
    })

    it('rejects partial prefixes and unrelated strings', () => {
      expect(isConfirmPhraseMatched('确认')).toBe(false)
      expect(isConfirmPhraseMatched('确认清理了')).toBe(false)
      expect(isConfirmPhraseMatched('')).toBe(false)
      expect(isConfirmPhraseMatched('取消')).toBe(false)
    })

    it('is case/character sensitive', () => {
      expect(isConfirmPhraseMatched('Confirm')).toBe(false)
    })
  })

  describe('normalizeOlderThanDaysInput', () => {
    it('returns null for empty or non-positive inputs', () => {
      expect(normalizeOlderThanDaysInput('')).toBeNull()
      expect(normalizeOlderThanDaysInput(null)).toBeNull()
      expect(normalizeOlderThanDaysInput(undefined)).toBeNull()
      expect(normalizeOlderThanDaysInput(0)).toBeNull()
      expect(normalizeOlderThanDaysInput(-3)).toBeNull()
    })

    it('returns an integer for positive numeric inputs', () => {
      expect(normalizeOlderThanDaysInput(7)).toBe(7)
      expect(normalizeOlderThanDaysInput('30')).toBe(30)
      expect(normalizeOlderThanDaysInput('7.9')).toBe(7)
    })

    it('returns null for non-numeric strings', () => {
      expect(normalizeOlderThanDaysInput('abc')).toBeNull()
    })
  })
})
