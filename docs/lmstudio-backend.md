# LM Studio Backend Support

**Status:** Planned
**Priority:** Medium
**Estimated Scope:** ~300-400 lines across 4 files

## Overview

Add LM Studio as an alternative backend to Ollama for running local LLMs. LM Studio provides an OpenAI-compatible API and a CLI (`lms`) that mirrors Ollama's functionality.

## Motivation

- LM Studio is popular among users who prefer a GUI for model management
- Some users may already have LM Studio installed and configured
- LM Studio's CLI provides equivalent automation capabilities to Ollama
- Offers choice without requiring users to install multiple tools

## Design

### New Backend Enum

```rust
// src/types.rs
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Ollama,
    LmStudio,
}
```

### Configuration

| Env Variable | Description | Default |
|--------------|-------------|---------|
| `BACKEND` | Which runtime to use | `ollama` |
| `LMSTUDIO_URL` | LM Studio API endpoint | `http://localhost:1234` |

`MODEL_SOURCE` remains relevant only for Ollama backend (to choose between Ollama library and HuggingFace). LM Studio uses HuggingFace format directly.

### CLI Command Mapping

| Operation | Ollama | LM Studio |
|-----------|--------|-----------|
| Check installed | `which ollama` | `which lms` |
| Start server | `ollama serve` | `lms server start` |
| Check running | `GET /api/tags` | `lms server status` |
| List downloaded | `GET /api/tags` | `lms ls` |
| Pull model | `ollama pull <model>` | `lms get <model>` |
| Load model | `POST /api/generate` (empty) | `lms load <model>` |
| Check loaded | `GET /api/ps` | `lms ps` |

### API Format Differences

**Ollama:**
```json
{
  "model": "model-name",
  "prompt": "...",
  "stream": false,
  "options": { "temperature": 0.1, "num_predict": 100 }
}
```

**LM Studio (OpenAI-compatible):**
```json
{
  "model": "model-name",
  "messages": [{ "role": "user", "content": "..." }],
  "stream": false,
  "temperature": 0.1,
  "max_tokens": 100
}
```

### Model Name Formats

| Backend | Source | Format | Example |
|---------|--------|--------|---------|
| Ollama | Ollama library | `model:tag` | `llama3.2:3b` |
| Ollama | HuggingFace | `hf.co/user/repo` | `hf.co/unsloth/SmolLM3-3B-128K-GGUF` |
| LM Studio | HuggingFace | `user/repo` | `unsloth/SmolLM3-3B-128K-GGUF` |
| LM Studio | HuggingFace | `user/repo@quant` | `qwen/qwen2.5-coder-32b-instruct-gguf@Q4_K_M` |

## Implementation Plan

### Phase 1: Types and Configuration

**File: `src/types.rs`**

1. Add `Backend` enum
2. Add `lmstudio_url` field to `Config`
3. Update `Config::default()` to read `BACKEND` and `LMSTUDIO_URL` env vars
4. Add `Config::backend_url()` method that returns appropriate URL

### Phase 2: Model Manager Abstraction

**File: `src/model_manager.rs`**

1. Add backend-aware methods:
   - `check_backend_installed()` - calls `which ollama` or `which lms`
   - `check_backend_running()` - checks appropriate API or CLI
   - `start_backend()` - runs `ollama serve` or `lms server start`
   - `check_model_exists()` - parses `lms ls` for LM Studio
   - `pull_model_with_progress()` - uses `lms get` for LM Studio
   - `load_model()` - uses `lms load` for LM Studio
   - `check_model_loaded()` - parses `lms ps` for LM Studio

2. Add OpenAI-compatible request/response types:
   ```rust
   #[derive(Serialize)]
   struct OpenAIRequest {
       model: String,
       messages: Vec<ChatMessage>,
       stream: bool,
       temperature: f32,
       max_tokens: u32,
   }

   #[derive(Serialize)]
   struct ChatMessage {
       role: String,
       content: String,
   }
   ```

3. Update `generate()` to use appropriate API format based on backend

### Phase 3: Handler Updates

**File: `src/lib.rs`**

1. Update `handle_init_llm_tool()` to use backend-agnostic methods
2. Update error messages to reference correct backend name
3. Update `handle_get_instructions_tool()` error messages

### Phase 4: Tests

**File: `tests/integration_test.rs`**

1. Add mock tests for LM Studio backend
2. Test backend selection logic

## API Endpoints

### LM Studio

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v1/models` | GET | List available models |
| `/v1/chat/completions` | POST | Generate completions |

### Ollama (current)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/tags` | GET | List models |
| `/api/ps` | GET | List loaded models |
| `/api/generate` | POST | Generate completions |

## Error Messages

Backend-specific friendly error messages:

| Scenario | Ollama | LM Studio |
|----------|--------|-----------|
| Not installed | "Ollama is not installed. Please install from https://ollama.com" | "LM Studio CLI not found. Install from https://lmstudio.ai and enable CLI in settings." |
| Not running | "Ollama is not running. Run init_llm to start it." | "LM Studio server not running. Run init_llm to start it." |
| Model not found | "Model 'X' not found. Check MODEL_NAME..." | "Model 'X' not found. Download it in LM Studio or check MODEL_NAME." |

## Usage Examples

### Ollama (default)
```json
{
  "env": {
    "BACKEND": "ollama",
    "MODEL_SOURCE": "huggingface",
    "MODEL_NAME": "unsloth/SmolLM3-3B-128K-GGUF"
  }
}
```

### LM Studio
```json
{
  "env": {
    "BACKEND": "lmstudio",
    "LMSTUDIO_URL": "http://localhost:1234",
    "MODEL_NAME": "unsloth/SmolLM3-3B-128K-GGUF"
  }
}
```

## Testing Strategy

1. **Unit tests**: Mock CLI outputs for `lms ls`, `lms ps`, `lms server status`
2. **Integration tests**: Test with actual LM Studio installation (manual/CI optional)
3. **API format tests**: Verify OpenAI request/response serialization

## References

- [LM Studio CLI Documentation](https://lmstudio.ai/docs/cli)
- [LM Studio `lms get` command](https://lmstudio.ai/docs/cli/get)
- [Use Models from the Hugging Face Hub in LM Studio](https://huggingface.co/blog/yagilb/lms-hf)
- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat)

## Open Questions

1. Should we support `lms get` progress parsing? (LM Studio may output progress differently than Ollama)
2. How does `lms server start` signal readiness? (Need to test stderr output)
3. Should we support quantization selection via `MODEL_NAME@Q4_K_M` syntax?
