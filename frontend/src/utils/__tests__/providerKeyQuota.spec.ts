import { describe, expect, it } from 'vitest'

import { getQuotaDisplayText } from '../providerKeyQuota'

describe('providerKeyQuota', () => {
  it('includes Codex Spark quota windows in display text', () => {
    expect(getQuotaDisplayText({
      status_snapshot: {
        oauth: {
          code: 'valid',
        },
        account: {
          code: 'ok',
          blocked: false,
        },
        quota: {
          provider_type: 'codex',
          code: 'ok',
          exhausted: false,
          windows: [
            {
              code: 'weekly',
              remaining_ratio: 0.9,
            },
            {
              code: '5h',
              remaining_ratio: 0.8,
            },
            {
              code: 'spark_5h',
              remaining_ratio: 0.6,
            },
            {
              code: 'spark_weekly',
              remaining_ratio: 0.95,
            },
          ],
        },
      },
    }, 'codex')).toBe('周剩余 90.0% | 5H剩余 80.0% | Spark5H剩余 60.0% | Spark周剩余 95.0%')
  })
})
