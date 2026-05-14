<template>
  <Dialog
    :open="open"
    size="lg"
    title="立即清理请求记录"
    description="按现有分级保留策略主动清理请求记录，可选指定清理更早时间的数据。操作不可逆。"
    :persistent="submitting"
    @update:open="handleOpenChange"
  >
    <div class="px-4 sm:px-6 py-4 space-y-4">
      <div>
        <Label
          for="manual-cleanup-older-than-days"
          class="block text-sm font-medium"
        >
          清理 N 天前的记录（可选）
        </Label>
        <Input
          id="manual-cleanup-older-than-days"
          :model-value="olderThanDays ?? ''"
          type="number"
          min="1"
          placeholder="留空代表按当前保留策略"
          class="mt-1"
          :disabled="submitting"
          @update:model-value="handleDaysChange"
        />
        <p class="mt-1 text-xs text-muted-foreground">
          留空代表按当前保留策略清理。填入数字代表清理 N 天前的记录；该值只能比策略更宽松，不会删除更新的数据。
        </p>
      </div>

      <div class="rounded-md border border-border bg-muted/30 px-4 py-3">
        <div class="flex items-center justify-between">
          <h4 class="text-sm font-medium">
            预计影响
          </h4>
          <button
            v-if="!previewLoading"
            type="button"
            class="text-xs text-muted-foreground hover:text-foreground"
            :disabled="submitting"
            @click="loadPreview"
          >
            刷新预估
          </button>
          <span
            v-else
            class="text-xs text-muted-foreground"
          >
            正在计算…
          </span>
        </div>
        <div
          v-if="previewError"
          class="mt-2 text-xs text-destructive"
        >
          {{ previewError }}
        </div>
        <div
          v-else-if="preview"
          class="mt-2 grid grid-cols-2 gap-y-1 gap-x-4 text-xs text-muted-foreground"
        >
          <div>详细记录待压缩</div>
          <div class="text-right text-foreground">
            {{ formatCount(preview.counts.detail) }}
          </div>
          <div>压缩记录待清体</div>
          <div class="text-right text-foreground">
            {{ formatCount(preview.counts.compressed) }}
          </div>
          <div>请求头待清空</div>
          <div class="text-right text-foreground">
            {{ formatCount(preview.counts.header) }}
          </div>
          <div>整条记录待删除</div>
          <div class="text-right text-destructive font-medium">
            {{ formatCount(preview.counts.log) }}
          </div>
        </div>
        <div
          v-else-if="!previewLoading"
          class="mt-2 text-xs text-muted-foreground"
        >
          尚未计算预估数据
        </div>
      </div>

      <div>
        <Label
          for="manual-cleanup-confirm-phrase"
          class="block text-sm font-medium"
        >
          输入「{{ confirmPhrase }}」以确认清理
        </Label>
        <Input
          id="manual-cleanup-confirm-phrase"
          :model-value="typedPhrase"
          class="mt-1"
          autocomplete="off"
          :placeholder="confirmPhrase"
          :disabled="submitting"
          @update:model-value="typedPhrase = String($event)"
          @keydown.enter.prevent="maybeSubmitOnEnter"
        />
        <p class="mt-1 text-xs text-muted-foreground">
          确认操作后会立刻执行清理，且不可撤销。
        </p>
      </div>
    </div>

    <template #footer>
      <Button
        variant="destructive"
        :disabled="!canSubmit"
        @click="handleConfirm"
      >
        {{ submitting ? '清理中…' : '确认清理' }}
      </Button>
      <Button
        variant="outline"
        :disabled="submitting"
        @click="handleCancel"
      >
        取消
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Dialog } from '@/components/ui'
import Button from '@/components/ui/button.vue'
import Input from '@/components/ui/input.vue'
import Label from '@/components/ui/label.vue'
import { adminApi, type ManualUsageCleanupPreview } from '@/api/admin'
import { parseApiError } from '@/utils/errorParser'
import {
  MANUAL_USAGE_CLEANUP_CONFIRM_PHRASE,
  isConfirmPhraseMatched,
  normalizeOlderThanDaysInput,
} from './manualCleanupForm'

const props = defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  confirm: [olderThanDays: number | undefined]
}>()

const confirmPhrase = MANUAL_USAGE_CLEANUP_CONFIRM_PHRASE

const olderThanDays = ref<number | null>(null)
const typedPhrase = ref('')
const preview = ref<ManualUsageCleanupPreview | null>(null)
const previewLoading = ref(false)
const previewError = ref<string | null>(null)
const submitting = ref(false)

let previewDebounceTimer: ReturnType<typeof setTimeout> | null = null
let previewSeq = 0

const normalizedPhrase = computed(() => typedPhrase.value)

const canSubmit = computed(
  () =>
    !submitting.value &&
    !previewLoading.value &&
    isConfirmPhraseMatched(normalizedPhrase.value),
)

watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      resetForm()
      void loadPreview()
    } else if (previewDebounceTimer) {
      clearTimeout(previewDebounceTimer)
      previewDebounceTimer = null
    }
  },
)

function resetForm() {
  olderThanDays.value = null
  typedPhrase.value = ''
  preview.value = null
  previewError.value = null
  previewLoading.value = false
  submitting.value = false
}

function handleDaysChange(value: string | number) {
  olderThanDays.value = normalizeOlderThanDaysInput(value)
  schedulePreview()
}

function schedulePreview() {
  if (previewDebounceTimer) {
    clearTimeout(previewDebounceTimer)
  }
  previewDebounceTimer = setTimeout(() => {
    previewDebounceTimer = null
    void loadPreview()
  }, 300)
}

async function loadPreview() {
  const seq = ++previewSeq
  previewLoading.value = true
  previewError.value = null
  try {
    const params: { older_than_days?: number } = {}
    if (olderThanDays.value !== null) {
      params.older_than_days = olderThanDays.value
    }
    const result = await adminApi.previewManualUsageCleanup(params)
    if (seq === previewSeq) {
      preview.value = result
    }
  } catch (error) {
    if (seq === previewSeq) {
      preview.value = null
      previewError.value = parseApiError(error).message
    }
  } finally {
    if (seq === previewSeq) {
      previewLoading.value = false
    }
  }
}

function handleOpenChange(value: boolean) {
  if (!value && submitting.value) {
    return
  }
  emit('update:open', value)
}

function handleCancel() {
  if (submitting.value) return
  emit('update:open', false)
}

function maybeSubmitOnEnter() {
  if (canSubmit.value) {
    void handleConfirm()
  }
}

async function handleConfirm() {
  if (!canSubmit.value) return
  submitting.value = true
  try {
    emit('confirm', olderThanDays.value ?? undefined)
  } finally {
    submitting.value = false
  }
}

function formatCount(value: number): string {
  return value.toLocaleString()
}
</script>
