---
tags: [decision]
status: active
updated: 2026-10-07
---

# ADR 0001 — Tauri 2 + React/TS

## Decision

Собрать Mote на **Tauri 2 + React + TypeScript**.

## Why

- Легче Electron
- Нативный Windows shell (tray, hotkeys, autostart plugins)
- Удобный путь к будущему API-провайдеру и Rust-командам

## Consequences

Нужны Rust toolchain + MSVC на машине разработчика. Cursor-агенты живут отдельно в `.cursor/`, не в рантайме питомца.
