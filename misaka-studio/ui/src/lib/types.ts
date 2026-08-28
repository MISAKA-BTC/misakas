// The runtime's wire types, mirrored.
//
// Hand-written rather than generated, and kept deliberately narrow: the UI reads a subset of what
// the API returns, and a type that claims every field would have to be regenerated for changes
// the UI does not care about. Everything here corresponds to a Rust type in
// `misaka-studio-core` or `misaka-studio-runtime`.

export type Quantization = {
  label: string
  bits_per_weight: number | null
  family: 'float' | 'legacy' | 'k_quant' | 'i_quant' | 'exotic' | 'unknown'
  tier: 'lossless' | 'recommended' | 'compact' | 'aggressive' | 'unknown'
}

export type ModelSource = {
  repo: string | null
  revision: string | null
  filename: string | null
  base_repo: string | null
  base_revision: string | null
  origin: string | null
}

export type ModelRequirements = {
  weights_bytes: number
  kv_cache_bytes: number
  overhead_bytes: number
  total_bytes: number
  context_tokens: number
}

export type FitVerdict =
  | { verdict: 'fits'; device: string; headroom_bytes: number }
  | { verdict: 'tight'; device: string; headroom_bytes: number }
  | { verdict: 'partial_offload'; device: string; gpu_bytes: number; needed_bytes: number }
  | { verdict: 'does_not_fit'; needed_bytes: number; available_bytes: number }

export type ModelIdentity = {
  h_m: string
  gguf_sha256: string
  gguf_size: number
  filename: string
  base_repo: string
  base_revision: string
}

export type ModelView = {
  id: string
  name: string
  path: string
  size_bytes: number
  quantization: Quantization | null
  architecture: string | null
  parameter_count: number | null
  context_length: number | null
  block_count: number | null
  expert_count: number | null
  kv_cache_bytes_per_token: number | null
  has_chat_template: boolean
  source: ModelSource
  sha256: string | null
  modified_at: number | null
  recommended_context: number
  requirements: ModelRequirements
  fit: FitVerdict
  fit_summary: string
  identity: ModelIdentity | null
}

export type RuntimeDescriptor = {
  backend: string
  engine_commit: string
  engine_patch_sha256: string
  engine_build_number: number
  build_profile: string
  class_tag: string
}

export type RuntimeStatus = {
  backend: string
  backend_available: boolean
  model_id: string | null
  context_size: number | null
  gpu_layers: number | null
  load_ms: number | null
  runtime_hash: string | null
  runtime_class_id: string | null
  model_hash: string | null
  descriptor: RuntimeDescriptor | null
}

export type Availability = { state: 'available'; detail: string } | { state: 'unavailable'; reason: string; remedy: string }

export type BackendInfo = { name: string; selected: boolean; availability: Availability }

export type Accelerator = {
  kind: 'apple_unified' | 'cuda' | 'rocm' | 'vulkan' | 'cpu'
  name: string
  total_memory: number | null
  free_memory: number | null
  usable_memory: number | null
  driver: string | null
  index: number
}

export type HardwareSnapshot = {
  os: string
  arch: string
  cpu_name: string
  physical_cores: number | null
  logical_cores: number
  total_memory: number
  available_memory: number
  accelerators: Accelerator[]
}

export type SystemInfo = {
  hardware: HardwareSnapshot
  data_dir: string
  models_dir: string
  records_path: string
  catalog_endpoint: string
}

export type AcceleratorSample = {
  index: number
  name: string
  utilization_percent: number | null
  memory_used: number | null
  memory_total: number | null
  temperature_c: number | null
}

export type RuntimeSample = {
  hardware: {
    cpu_percent: number
    process_cpu_percent: number
    memory_used: number
    memory_total: number
    process_memory: number
    accelerators: AcceleratorSample[]
  }
  generation: {
    active: number
    last_tokens_per_second: number
    last_time_to_first_token_ms: number
    total_tokens: number
    total_generations: number
  }
}

export type CatalogEntry = {
  id: string
  downloads: number
  likes: number
  tags: string[]
  last_modified: string | null
  gated: boolean
  pipeline_tag: string | null
}

export type CatalogFile = {
  path: string
  size: number | null
  sha256: string | null
  quantization: Quantization | null
}

export type CatalogRepo = {
  id: string
  revision: string | null
  gated: boolean
  files: CatalogFile[]
  base_model: string | null
}

export type DownloadProgress = {
  id: string
  repo: string
  file: string
  model_id: string
  destination: string
  downloaded: number
  total: number | null
  bytes_per_second: number
  status: 'downloading' | 'verifying' | 'completed' | 'failed' | 'cancelled'
  error: string | null
}

export type Settings = {
  models_dir: string
  server: { host: string; port: number; api_key: string | null; cors_origins: string[] }
  backend: {
    kind: 'auto' | 'llama_cpp' | 'mlx' | 'misaka' | 'mock'
    llama_server_path: string | null
    mlx_server_path: string | null
    gpu_layers: { mode: 'auto' } | { mode: 'all' } | { mode: 'none' } | { mode: 'fixed'; layers: number }
    threads: number | null
    flash_attention: boolean
    use_mmap: boolean
    use_mlock: boolean
    extra_args: string[]
    startup_timeout_secs: number
  }
  generation: {
    system_prompt: string
    context_size: number | null
    temperature: number
    top_p: number
    top_k: number
    min_p: number
    repeat_penalty: number
    max_tokens: number
    seed: number | null
  }
  huggingface: { endpoint: string; token: string | null; max_concurrent_downloads: number }
  ui: { theme: 'system' | 'light' | 'dark'; show_provenance: boolean; show_performance: boolean }
  provenance: { record_inferences: boolean; keep_transcripts: boolean; max_records: number }
}

export type InferenceRecord = {
  id: string
  model: ModelIdentity | null
  runtime: { h_r: string; class_id: string; descriptor: RuntimeDescriptor }
  params: {
    temperature: number
    top_p: number
    top_k: number
    min_p: number
    repeat_penalty: number
    max_tokens: number
    seed: number | null
  }
  prompt_commitment: string
  output_commitment: string
  prompt_tokens: number
  completion_tokens: number
  inference_hash: string
  replayability: 'deterministic' | 'seeded_sampling' | 'unrepeatable'
  started_at_unix_ms: number
  duration_ms: number
  time_to_first_token_ms: number | null
  tokens_per_second: number
  prompt?: string
  completion?: string
  model_id?: string
}

/** What the UI records about a completed turn, so a message can show how it was produced. */
export type TurnStats = {
  tokensPerSecond: number
  completionTokens: number
  promptTokens: number
  timeToFirstTokenMs: number | null
  model: string
  finishReason: string
}

export type ChatMessage = {
  id: string
  role: 'system' | 'user' | 'assistant'
  content: string
  /** Set while a response is still streaming. */
  streaming?: boolean
  error?: string
  stats?: TurnStats
}

export type Conversation = {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  modelId: string | null
  messages: ChatMessage[]
}
