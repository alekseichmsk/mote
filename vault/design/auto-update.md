---
tags: [design, release]
status: active
updated: 2026-10-12
---

# Design: автообновление (GitHub + локальный ключ)

## Цель

Установленный Mote сам находит новую версию на GitHub Releases, проверяет подпись Tauri (minisign) и ставит NSIS setup.

## Стек

- `tauri-plugin-updater` 2
- Статический `latest.json` на релизе
- Подпись: `tauri signer` (pubkey в `tauri.conf.json`, private key только локально / CI secret)

## Поток

1. Старт → через ~8 с фоновый `check` (не блокирует overlay).
2. Есть update → лог `[mote] update available …`, событие `mote://update-available`, в трее активен «Установить обновление».
3. Трей «Проверить обновления…» — ручной check.
4. «Установить обновление» → `download_and_install` (Windows `installMode: passive`) → `app.restart()`.
5. Нет сети / 404 endpoint → `[mote] update err: …` или skip, без краша.

## Конфиг

- Endpoint: `https://github.com/alekseichmsk/mote/releases/latest/download/latest.json`  
  (см. [[auto-update-release]]).

## Файлы

`src-tauri/src/update.rs`, `lib.rs` (tray + plugin), `tauri.conf.json`, `Cargo.toml`, `capabilities/default.json`, vault этот + release howto.

## Логи

`[mote] update check`, `update available v…`, `update downloading`, `update installed`, `update none`, `update err: …`

## Риски

- SmartScreen без EV-сертификата Windows.
- Потеря private key = нельзя подписать следующие обновления для уже установленных клиентов.
- Репо должно быть публичным (иначе latest.json недоступен без токена).
- Автозапуск должен указывать на установленный `Program Files\Mote`, не на `release\` из репо.

## Вне скоупа v1

CI tauri-action, beta-канал, MSI в updater, Authenticode.
