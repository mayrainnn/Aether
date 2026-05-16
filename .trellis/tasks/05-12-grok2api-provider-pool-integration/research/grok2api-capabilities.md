# grok2api 能力取证

范围：`/Volumes/mayrain/workspace/private/grok2api`

## 结论先行

这个项目不是“单纯把请求转发到一个上游 API”，而是一个按 **模型能力 + 账号池 + 代理 + 上游协议** 共同决定行为的 Grok 网关。

最关键的集成事实：

1. OpenAI 兼容层的可见模型，不是静态枚举出来就算可用，而是要同时满足 `ModelSpec.enabled`、当前可用账号池、以及 `supports_mode(pool, mode)`。
2. 上游 Grok 访问依赖每个账号的 SSO / Cloudflare 材料和代理 lease，不是统一服务级 token。
3. Chat / Responses / Image / Video 走的是不同的上游路径：HTTP SSE、WebSocket、任务化视频 job、文件上传与本地缓存，不能用一条“provider pool”抽象硬压平。

## 公开接口面

- `app/main.py:398-405` 只挂载了 `web_router`、`openai_router`、`anthropic_router`，所以产品面实际分为 `/v1/*`、`/v1/messages`、`/admin/*`、`/webui/*`。
- `README.md:17-24` 和 `README.md:269-285` 列出了主要接口：
  - OpenAI 兼容：`/v1/models`、`/v1/chat/completions`、`/v1/responses`、`/v1/images/generations`、`/v1/images/edits`、`/v1/videos`、`/v1/videos/{video_id}`、`/v1/videos/{video_id}/content`
  - Anthropic 兼容：`/v1/messages`
  - 本地文件：`/v1/files/image`、`/v1/files/video`
- `app/products/openai/router.py:213-345` 是 `chat/completions` 和 `responses` 的统一入口包装层，`app/products/openai/router.py:433-608` 负责独立 image/video/files 路由。

## OpenAI 兼容层行为

- `app/products/openai/schemas.py:29-42` 的 `ChatCompletionRequest` 接受 `reasoning_effort`、`image_config`、`video_config`、`tools`、`tool_choice`、`parallel_tool_calls`、`max_tokens`，但实际分发时只使用其中一部分。
- `app/products/openai/schemas.py:62-88` 的 `ResponsesCreateRequest` 明确写了“只处理 model/input/instructions/stream/reasoning/temperature/top_p，其余字段静默丢弃”。
- `app/products/openai/router.py:236-312` 根据模型能力分发：
  - `is_image_edit()` -> `images.edit`
  - `is_image()` -> `images.generate`
  - `is_video()` -> `video.completions`
  - 否则走 `chat_completions`
- `app/products/openai/router.py:314-344` 和 `app/products/openai/router.py:352-425` 分别把异常包装成 OpenAI / Responses 风格 SSE 错误事件。
- `app/products/openai/_format.py:97-134` 说明普通 chat response 会把 `reasoning_content`、`annotations`、`search_sources` 挂到响应体里。

## 流式实现

- `app/products/openai/chat.py:380-447` 直接消费上游 Grok app-chat 的 SSE 行。
- `app/products/openai/chat.py:503-723` 的 streaming 路径会：
  - 保留 reasoning delta
  - 解析 tool call XML
  - 把图像 URL 转成 markdown 或本地代理 URL
  - 把 search sources 放进 `search_sources`
  - 在最终事件后输出 `data: [DONE]`
- `app/products/openai/chat.py:724-883` 的非流式路径会汇总全文、图片、引用、思考内容，再拼成 OpenAI chat completion 形状。
- `app/products/openai/responses.py:260-749` 会把 Responses API 的事件序列重建成标准 `response.created` / `response.output_item.*` / `response.completed` SSE 流。
- `app/products/openai/video.py:1089-1135` 的视频 chat-completions 也支持 SSE 进度事件，最后返回 chat completion 形状。

## 模型与账号池

- `app/control/model/registry.py:12-56` 是模型真值表，涵盖：
  - Chat：`grok-4.20-*`、`grok-4.3-beta`
  - Image：`grok-imagine-image-lite`、`grok-imagine-image`、`grok-imagine-image-pro`
  - Image edit：`grok-imagine-image-edit`
  - Video：`grok-imagine-video`
- `app/control/model/spec.py:50-91` 定义了池优先级：
  - 默认：basic -> super -> heavy，或 super -> heavy
  - `prefer_best=True` 时会反向优先试更高等级池
- `app/products/_account_selection.py:15-85` 定义了账号重试与模式回退：
  - random 策略固定最多 5 次重试
  - quota 策略读 `retry.max_retries`
  - chat 的 `AUTO` 可按 `features.auto_chat_mode_fallback` 回退到 FAST / EXPERT
- `app/products/openai/router.py:39-56` 的 `/v1/models` 不是静态输出，而是根据当前可管理账号池动态筛选。

## 账号、认证、配置

- `app/platform/auth/middleware.py:59-118` 说明：
  - `/v1/*` 使用 `app.api_key`，支持 `Authorization: Bearer ...` 和 `X-API-Key`
  - `/admin/*` 使用 `app.app_key`
  - `/webui/*` 使用 `app.webui_key`，为空且 `webui_enabled=true` 时放行
- `README.md:144-150` 与 `app/platform/auth/middleware.py:59-118` 一致。
- `app/control/account/backends/factory.py:14-36` 和 `.env.example:27-49` 说明账号存储可选：
  - `local` -> SQLite
  - `redis`
  - `mysql`
  - `postgresql`
- `app/control/account/backends/factory.py:81-136` 说明本地库默认落到 `${DATA_DIR}/accounts.db`，SQL 后端需要相应 DSN。
- `app/platform/config/snapshot.py:25-152` 说明配置优先级是：
  1. `config.defaults.toml`
  2. 后端用户覆盖
  3. `GROK_*` 环境变量，永远最高优先级
- `README.md:158-192` 和 `config.defaults.toml` 都确认了运行时配置、环境变量、以及 `GROK_APP_API_KEY` 这类覆盖方式。

## 上游 Grok / X 访问方式

- `app/dataplane/reverse/runtime/endpoint_table.py:11-43` 是上游 URL 真值表：
  - `https://grok.com/rest/app-chat/conversations/new`
  - `https://grok.com/rest/app-chat/upload-file`
  - `https://grok.com/rest/rate-limits`
  - `https://grok.com/rest/media/post/create`
  - `https://grok.com/rest/media/post/create-link`
  - `wss://grok.com/ws/imagine/listen`
  - `wss://livekit.grok.com`
  - `https://accounts.x.ai/auth_mgmt.AuthManagement/SetTosAcceptedVersion`
- `app/dataplane/reverse/transport/http.py:16-225` 说明所有 HTTP 上游调用都经由 `curl_cffi` 的 `ResettableSession`，并带：
  - `build_http_headers(...)`
  - `build_session_kwargs(...)`
  - 非 200 / 非 201 / 非 204 时转成 `UpstreamError`
- `app/dataplane/proxy/adapters/headers.py:172-260` 说明 cookie 不是裸 `sso`：
  - 会拼 `sso` / `sso-rw`
  - 可附带 `cf_clearance`
  - 会注入浏览器指纹、客户端 hints、origin / referer
- `app/dataplane/proxy/adapters/session.py:36-68` 说明 proxy lease 也会影响 session kwargs，支持 SOCKS / HTTP 代理和 SSL verify 关闭。
- `app/dataplane/reverse/protocol/xai_chat.py:16-83` 说明 app-chat payload 直接使用 `modeId`、`temporary`、`disableMemory`、`toolOverrides`、`responseMetadata` 等 Grok Web 字段，不是 OpenAI 字段。
- `app/dataplane/reverse/transport/imagine_ws.py:278-381` 说明图像生成主要走 `wss://grok.com/ws/imagine/listen`，并可在一个 WS 连接上多轮复用。

## 失败与重试

- `app/platform/errors.py:14-87` 定义了统一错误类型：
  - `ValidationError` -> 400
  - `AuthError` -> 401
  - `RateLimitError` -> 429
  - `UpstreamError` -> 502+，并携带上游 body excerpt
- `app/dataplane/reverse/classifier.py:13-52` 会把上游状态映射为 `SUCCESS` / `RATE_LIMITED` / `AUTH_FAILURE` / `FORBIDDEN` / `UPSTREAM_5XX` 等类别。
- `app/dataplane/reverse/protocol/xai_usage.py:201-219` 专门识别 invalid/blocked credential 文本标记。
- `app/products/openai/chat.py:160-188` 和 `app/products/openai/chat.py:672-716` 会按 `retry.on_codes` 决定是否换账号重试，并在成功 / 失败后异步做 quota / failure 同步。
- `app/products/openai/video.py:846-899` 说明视频任务也会回写账号反馈，但视频本身是任务化流程，不像 chat 那样直接流式返回最终内容。

## 图像 / 视频的实际约束

- `app/products/openai/router.py:436-455` 与 `app/products/openai/images.py:272-481` 说明图像生成有两条上游路径：
  - `grok-imagine-image-lite` 走 chat endpoint
  - 其他 image 模型走 imagine WebSocket
- `app/products/openai/router.py:529-569` 和 `app/products/openai/images.py:620-758` 说明 image edit 需要 multipart 图片输入，`mask` 当前不支持，`size` 基本锁死在 `1024x1024`。
- `app/products/openai/router.py:463-497` 与 `app/products/openai/video.py:916-1040` 说明视频创建是异步 job：
  - `POST /v1/videos` 创建 job
  - `GET /v1/videos/{video_id}` 查询状态
  - `GET /v1/videos/{video_id}/content` 下载结果
  - 输入参考图最多保留前 7 张
- `README.md:388-396` 和 `README.md:510-551` / `README.md:588-597` 已把这些约束写进示例。

## 对 provider pool 集成最有用的判断

1. 不能只在 OpenAI 路由层加“provider pool”开关，必须进入 `ModelSpec.pool_candidates()` / `reserve_account()` / `list_models()` 这条链路。
2. 需要区分 chat、responses、image WS、video job 四条上游路径，它们对账号反馈、重试和成功判定并不一致。
3. 上游 Grok 访问依赖 SSO + Cloudflare + proxy lease，池化时要把这些材料当成“账号级状态”，不能只看 token 字符串。
4. 配置层的 `GROK_*` 覆盖总是最高优先级，部署文档和运行时调参必须按这个优先级理解。
