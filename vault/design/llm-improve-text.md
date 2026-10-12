---
tags: [design, phase-2, llm]
status: active
updated: 2026-10-11
---

# Design: LLM sidecar + «Улучшить текст»

## Цель
Локальный `llama-server` + GGUF полностью из Mote (без Ollama и без ручного env). Словарь остаётся мгновенным; LLM по кнопке исправляет смысл/пунктуацию. Устройство: **GPU (CUDA)** или **CPU**.

## Поток
1. Пользователь: plaque «Улучшить» или selection-меню / Настройки → «Скачать недостающее».
2. `llm_improve_text` → ensure GGUF → ensure runtime (`bin/cuda` или `bin/cpu` по prefs) → ensure sidecar (`-ngl 99` / `-ngl 0`) → `POST /v1/chat/completions`.
3. Нет модели или движка → фоновое скачивание + `DOWNLOADING` → UI ждёт → повтор.
4. Ответ (только текст) → обновить spell offer → Заменить как сейчас.
5. **Warmup:** при старте Mote, если `llmDevice=gpu` и модель+CUDA на месте → фон: sidecar + dummy `improve("ок")` (ответ отбрасывается). CPU / нет файлов → skip, без скачивания.

## Данные
- Модель: `%APPDATA%\Mote\models\Qwen2.5-3B-Instruct-Q4_K_M.gguf`
- Prefs: `%APPDATA%\Mote\prefs.json` → `{ "llmDevice": "gpu" | "cpu" }` (дефолт `gpu`)
- Движок CPU: `%APPDATA%\Mote\bin\cpu\` — zip `llama-b9992-bin-win-cpu-x64.zip`
- Движок GPU: `%APPDATA%\Mote\bin\cuda\` — `llama-b9992-bin-win-cuda-12.4-x64.zip` + `cudart-llama-bin-win-cuda-12.4-x64.zip`
- Миграция: старый плоский `bin\llama-server.exe` → `bin\cpu\`

## Invoke / события
- `llm_status` → `{ ready, modelPath, modelBytes, modelPresent, runtimePresent, runtimePath, sidecarRunning, port, detail, device, gpuRuntimePresent, cpuRuntimePresent }`
- `llm_set_device { device }` → пишет prefs, останавливает sidecar
- `llm_download_model` → скачивает **недостающее** (модель и/или движок активного device) + `mote://model-progress`
- `llm_improve_text { text }` → `{ improved }`
- `mote://llm-status` (опционально) при смене состояния sidecar

## Логи `[mote]`
`llm: ensure`, `llm: download …`, `llm: device set gpu|cpu`, `llm: sidecar start … device= ngl=`, `llm: warmup start|ok|skip|err`, `llm: improve ok chars=`, ошибки без полного текста пользователя.

## Риски
CUDA zip ~600 МБ суммарно; только NVIDIA. Готовность GPU = `ggml-cuda*` **и** `cudart*`/`cublas*` (частичная распаковка не считается «ок»). SHA256 пока optional (один константный слот на три zip — не заполнять одним хэшем). `tar` для распаковки на Windows. Sidecar — скрытый дочерний процесс (`CREATE_NO_WINDOW`). Смена device → stop sidecar → следующий Улучшить поднимает заново.

## Setup (пользователь)
1. Настройки → GPU (CUDA) или CPU → Сохранить.
2. «Скачать недостающее» **или** сразу «Улучшить».
3. Plaque → Улучшить / ПКМ выделения → Улучшить текст.
4. `MOTE_LLAMA_SERVER` — только для разработки.

## Вне среза
game_mode, чат через sidecar, облако/Ollama, авто на паузе печати, Vulkan/HIP.
