import { normalizeApiFormatAlias } from '@/api/endpoints/types/api-format'
import type { ProviderModelMapping } from '@/api/endpoints/types'
import type { TestModelRequest } from '@/api/endpoints/providers'

const DEFAULT_MODEL_TEST_MESSAGE = 'Hello! This is a test message.'
const MODEL_TEST_RESPONSE_PREVIEW_MAX_LENGTH = 160

type ModelTestMappingSource = {
  provider_model_name: string
  provider_model_mappings?: ProviderModelMapping[] | null
}

type ModelTestMappingEndpoint = {
  id: string
  api_format: string
}

type ModelTestEndpointSource = {
  api_format: string
  is_active?: boolean | null
}

type ModelTestKeySource = {
  api_formats?: string[] | null
  is_active?: boolean | null
}

export type ModelTestMappedModelOption = {
  name: string
  priority: number
}

const MODEL_TEST_UNSUPPORTED_API_FORMATS = new Set([
  'openai:video',
  'gemini:video',
  'gemini:files',
])

const MODEL_TEST_DIAGNOSTIC_LABELS: Record<string, string> = {
  pool_account_blocked: '账号已失效，需重新授权',
}

type JsonRecord = Record<string, unknown>

export function isModelTestableApiFormat(apiFormat: string | null | undefined): boolean {
  const normalized = normalizeApiFormatAlias(apiFormat ?? '')
  return Boolean(normalized) && !MODEL_TEST_UNSUPPORTED_API_FORMATS.has(normalized)
}

export function modelTestKeySupportsEndpoint(
  key: ModelTestKeySource,
  endpoint: ModelTestEndpointSource,
): boolean {
  if (key.is_active === false) return false

  const endpointFormat = normalizeApiFormatAlias(endpoint.api_format)
  if (!isModelTestableApiFormat(endpointFormat)) return false

  const keyFormats = normalizeStringList(key.api_formats ?? undefined)
  if (keyFormats.length === 0) return true

  return keyFormats.some(format => normalizeApiFormatAlias(format) === endpointFormat)
}

export function isModelTestableEndpoint(
  endpoint: ModelTestEndpointSource,
  keys: ModelTestKeySource[],
): boolean {
  return endpoint.is_active !== false
    && isModelTestableApiFormat(endpoint.api_format)
    && keys.some(key => modelTestKeySupportsEndpoint(key, endpoint))
}

export function formatModelTestDiagnostic(value: string | null | undefined): string {
  const normalized = value?.trim()
  if (!normalized) return ''
  return MODEL_TEST_DIAGNOSTIC_LABELS[normalized] ?? normalized
}

export function extractModelTestResponsePreview(responseBody: unknown): string | null {
  const text = extractResponseText(responseBody)
  if (text) return text

  const reasoning = extractResponseReasoning(responseBody)
  if (reasoning) return `推理：${reasoning}`

  const summary = extractResponseSummary(responseBody)
  if (summary) return summary

  return null
}

function normalizeStringList(values: string[] | undefined): string[] {
  return (values ?? [])
    .map(value => value.trim())
    .filter(Boolean)
}

function isJsonRecord(value: unknown): value is JsonRecord {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

function compactPreviewText(value: unknown): string | null {
  if (typeof value !== 'string') return null

  const normalized = value.replace(/\s+/g, ' ').trim()
  if (!normalized) return null

  if (normalized.length <= MODEL_TEST_RESPONSE_PREVIEW_MAX_LENGTH) {
    return normalized
  }
  return `${normalized.slice(0, MODEL_TEST_RESPONSE_PREVIEW_MAX_LENGTH - 3)}...`
}

function joinPreviewParts(parts: string[]): string | null {
  return compactPreviewText(parts.filter(Boolean).join(' '))
}

function extractTextFromContentParts(value: unknown, depth = 0): string | null {
  if (depth > 4) return null

  const directText = compactPreviewText(value)
  if (directText) return directText

  if (!Array.isArray(value)) return null

  const parts = value.flatMap((part) => {
    if (typeof part === 'string') return [part]
    if (!isJsonRecord(part)) return []

    const text = compactPreviewText(part.text)
      ?? compactPreviewText(part.content)
      ?? extractTextFromContentParts(part.parts, depth + 1)
    return text ? [text] : []
  })

  return joinPreviewParts(parts)
}

function extractResponseText(responseBody: unknown, depth = 0): string | null {
  if (depth > 4 || !isJsonRecord(responseBody)) return null

  const wrappedText = extractResponseText(responseBody.response, depth + 1)
    ?? extractResponseText(responseBody.body, depth + 1)
  if (wrappedText) return wrappedText

  const outputText = compactPreviewText(responseBody.output_text)
  if (outputText) return outputText

  const topLevelContentText = extractTextFromContentParts(responseBody.content, depth + 1)
  if (topLevelContentText) return topLevelContentText

  const choicesText = extractChoicesText(responseBody.choices, depth + 1)
  if (choicesText) return choicesText

  const outputTextParts = extractOutputText(responseBody.output, depth + 1)
  if (outputTextParts) return outputTextParts

  const candidateText = extractGeminiCandidateText(responseBody.candidates, depth + 1)
  if (candidateText) return candidateText

  return null
}

function extractResponseReasoning(responseBody: unknown, depth = 0): string | null {
  if (depth > 4 || !isJsonRecord(responseBody)) return null

  const wrappedReasoning = extractResponseReasoning(responseBody.response, depth + 1)
    ?? extractResponseReasoning(responseBody.body, depth + 1)
  if (wrappedReasoning) return wrappedReasoning

  const directReasoning = compactPreviewText(responseBody.reasoning_content)
    ?? compactPreviewText(responseBody.thinking)
  if (directReasoning) return directReasoning

  const topLevelReasoning = extractReasoningFromContentParts(responseBody.content, depth + 1)
  if (topLevelReasoning) return topLevelReasoning

  const choicesReasoning = extractChoicesReasoning(responseBody.choices, depth + 1)
  if (choicesReasoning) return choicesReasoning

  const outputReasoning = extractOutputReasoning(responseBody.output, depth + 1)
  if (outputReasoning) return outputReasoning

  return null
}

function extractChoicesText(value: unknown, depth: number): string | null {
  if (!Array.isArray(value)) return null

  for (const choice of value) {
    if (!isJsonRecord(choice)) continue

    const messageText = isJsonRecord(choice.message)
      ? extractTextFromContentParts(choice.message.content, depth + 1)
      : null
    const deltaText = isJsonRecord(choice.delta)
      ? extractTextFromContentParts(choice.delta.content, depth + 1)
      : null
    const text = messageText ?? deltaText ?? extractTextFromContentParts(choice.text, depth + 1)
    if (text) return text
  }

  return null
}

function extractChoicesReasoning(value: unknown, depth: number): string | null {
  if (!Array.isArray(value)) return null

  for (const choice of value) {
    if (!isJsonRecord(choice)) continue

    const messageReasoning = isJsonRecord(choice.message)
      ? extractReasoningFromMessage(choice.message, depth + 1)
      : null
    const deltaReasoning = isJsonRecord(choice.delta)
      ? extractReasoningFromMessage(choice.delta, depth + 1)
      : null
    const reasoning = messageReasoning ?? deltaReasoning
    if (reasoning) return reasoning
  }

  return null
}

function extractReasoningFromMessage(message: JsonRecord, depth: number): string | null {
  return compactPreviewText(message.reasoning_content)
    ?? compactPreviewText(message.thinking)
    ?? extractReasoningFromContentParts(message.content, depth + 1)
}

function extractOutputText(value: unknown, depth: number): string | null {
  if (!Array.isArray(value)) return null

  for (const outputItem of value) {
    if (!isJsonRecord(outputItem)) continue

    const contentText = extractTextFromContentParts(outputItem.content, depth + 1)
      ?? extractResponseText(outputItem.response, depth + 1)
    if (contentText) return contentText
  }

  return null
}

function extractOutputReasoning(value: unknown, depth: number): string | null {
  if (!Array.isArray(value)) return null

  for (const outputItem of value) {
    if (!isJsonRecord(outputItem)) continue

    const reasoning = extractReasoningFromContentParts(outputItem.content, depth + 1)
      ?? compactPreviewText(outputItem.reasoning_content)
      ?? compactPreviewText(outputItem.thinking)
      ?? extractResponseReasoning(outputItem.response, depth + 1)
    if (reasoning) return reasoning
  }

  return null
}

function extractGeminiCandidateText(value: unknown, depth: number): string | null {
  if (!Array.isArray(value)) return null

  for (const candidate of value) {
    if (!isJsonRecord(candidate) || !isJsonRecord(candidate.content)) continue

    const text = extractTextFromContentParts(candidate.content.parts, depth + 1)
    if (text) return text
  }

  return null
}

function extractReasoningFromContentParts(value: unknown, depth = 0): string | null {
  if (depth > 4 || !Array.isArray(value)) return null

  const parts = value.flatMap((part) => {
    if (!isJsonRecord(part)) return []

    const reasoning = compactPreviewText(part.reasoning_content)
      ?? compactPreviewText(part.thinking)
      ?? compactPreviewText(part.reasoning)
      ?? extractReasoningFromContentParts(part.content, depth + 1)
      ?? extractReasoningFromContentParts(part.parts, depth + 1)
    return reasoning ? [reasoning] : []
  })

  return joinPreviewParts(parts)
}

function extractResponseSummary(responseBody: unknown): string | null {
  if (!isJsonRecord(responseBody)) return null

  if (Array.isArray(responseBody.data)) {
    const embeddingDimensions = responseBody.data
      .map(item => isJsonRecord(item) && Array.isArray(item.embedding) ? item.embedding.length : null)
      .find((size): size is number => typeof size === 'number')
    if (embeddingDimensions != null) return `Embedding 维度：${embeddingDimensions}`
    if (responseBody.data.length > 0) return `返回数据：${responseBody.data.length} 条`
  }

  if (Array.isArray(responseBody.results)) return `Rerank 结果：${responseBody.results.length} 条`

  const model = compactPreviewText(responseBody.model)
  if (model) return `返回模型：${model}`

  return null
}

function mappingApiFormatMatches(mapping: ProviderModelMapping, endpoint: ModelTestMappingEndpoint): boolean {
  const apiFormats = normalizeStringList(mapping.api_formats)
  if (apiFormats.length === 0) return true
  const endpointFormat = normalizeApiFormatAlias(endpoint.api_format)
  return apiFormats.some(format => normalizeApiFormatAlias(format) === endpointFormat)
}

function mappingEndpointMatches(mapping: ProviderModelMapping, endpoint: ModelTestMappingEndpoint): boolean {
  const endpointIds = normalizeStringList(mapping.endpoint_ids)
  if (endpointIds.length === 0) return true
  return endpointIds.includes(endpoint.id)
}

export function listModelTestMappedModelOptions(
  model: ModelTestMappingSource | null | undefined,
  endpoint: ModelTestMappingEndpoint | null | undefined,
): ModelTestMappedModelOption[] {
  if (!model || !endpoint || !Array.isArray(model.provider_model_mappings)) return []

  const matchedMappings = model.provider_model_mappings
    .filter(mapping => mapping.name.trim())
    .filter(mapping => mappingApiFormatMatches(mapping, endpoint))
    .filter(mapping => mappingEndpointMatches(mapping, endpoint))
    .sort((left, right) => {
      const leftPriority = Number.isFinite(left.priority) ? left.priority : 1
      const rightPriority = Number.isFinite(right.priority) ? right.priority : 1
      return leftPriority - rightPriority || left.name.localeCompare(right.name)
    })
  const seen = new Set<string>()
  return matchedMappings.flatMap((mapping) => {
    const name = mapping.name.trim()
    const dedupeKey = name.toLowerCase()
    if (seen.has(dedupeKey)) return []
    seen.add(dedupeKey)
    return [{
      name,
      priority: Number.isFinite(mapping.priority) ? mapping.priority : 1,
    }]
  })
}

export function normalizeModelTestMappedModelSelection(
  options: ModelTestMappedModelOption[],
  preferredName: string | null | undefined,
): string | null {
  const preferred = preferredName?.trim()
  if (!preferred) return null
  return options.find(option => option.name === preferred)?.name ?? null
}

export function setModelTestRequestBodyModel(draft: string, modelName: string): string {
  const parsed = parseModelTestRequestBodyDraft(draft)
  if (!parsed.value || parsed.error) return draft

  return JSON.stringify({
    ...parsed.value,
    model: modelName,
  }, null, 2)
}

export function syncModelTestRequestBodyDraft(
  draft: string,
  resetValue: string,
  nextResetValue: string,
  modelName?: string | null,
): { draft: string; resetValue: string } {
  const nextReset = modelName?.trim()
    ? setModelTestRequestBodyModel(nextResetValue, modelName.trim())
    : nextResetValue
  const draftIsUntouched = !draft || draft === resetValue

  if (draftIsUntouched) {
    return {
      draft: nextReset,
      resetValue: nextReset,
    }
  }

  return {
    draft: modelName?.trim()
      ? setModelTestRequestBodyModel(draft, modelName.trim())
      : draft,
    resetValue: nextReset,
  }
}

export function buildExactModelMappingTestRequest(
  providerId: string,
  modelName: string,
  apiFormat: string | null | undefined,
): TestModelRequest {
  return {
    provider_id: providerId,
    model_name: modelName,
    mode: 'direct',
    apply_model_mapping: false,
    api_format: apiFormat || undefined,
  }
}

export function buildDefaultModelTestRequestBody(modelName: string, apiFormat?: string | null): string {
  if (apiFormat?.trim().toLowerCase().endsWith(':embedding')) {
    return JSON.stringify({
      model: modelName,
      input: 'This is a test embedding input.',
    }, null, 2)
  }

  if (apiFormat?.trim().toLowerCase().endsWith(':rerank')) {
    return JSON.stringify({
      model: modelName,
      query: 'Apple',
      documents: [
        'apple',
        'banana',
        'fruit',
        'vegetable',
      ],
      return_documents: true,
      top_n: 4,
    }, null, 2)
  }

  return JSON.stringify({
    model: modelName,
    messages: [
      {
        role: 'user',
        content: DEFAULT_MODEL_TEST_MESSAGE,
      },
    ],
    max_tokens: 30,
    temperature: 0.7,
    stream: true,
  }, null, 2)
}

export function buildDefaultModelTestRequestHeaders(): string {
  return JSON.stringify({}, null, 2)
}

function parseModelTestJsonObjectDraft(
  draft: string,
  options: {
    emptyValue: Record<string, unknown> | null
    emptyError: string | null
    invalidTypeError: string
  },
): { value: Record<string, unknown> | null; error: string | null } {
  const normalized = draft.trim()
  if (!normalized) {
    return {
      value: options.emptyValue,
      error: options.emptyError,
    }
  }

  try {
    const parsed = JSON.parse(normalized)
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return {
        value: null,
        error: options.invalidTypeError,
      }
    }
    return {
      value: parsed as Record<string, unknown>,
      error: null,
    }
  } catch (error) {
    return {
      value: null,
      error: error instanceof Error ? error.message : '无效的 JSON',
    }
  }
}

export function parseModelTestRequestBodyDraft(
  draft: string,
): { value: Record<string, unknown> | null; error: string | null } {
  return parseModelTestJsonObjectDraft(draft, {
    emptyValue: null,
    emptyError: '测试请求体不能为空',
    invalidTypeError: '测试请求体必须是 JSON 对象',
  })
}

export function parseModelTestRequestHeadersDraft(
  draft: string,
): { value: Record<string, unknown> | null; error: string | null } {
  return parseModelTestJsonObjectDraft(draft, {
    emptyValue: {},
    emptyError: null,
    invalidTypeError: '测试请求头必须是 JSON 对象',
  })
}
