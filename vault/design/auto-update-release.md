---
tags: [design, release]
status: active
updated: 2026-10-12
---

# Релиз: подпись и GitHub Releases

См. [[auto-update]].

## Один раз: ключи

```powershell
npm run tauri signer generate -- -w "$env:USERPROFILE\.tauri\mote.key"
```

- Публичный ключ (вывод CLI) → `tauri.conf.json` → `plugins.updater.pubkey` (строка, не путь).
- Приватный: `%USERPROFILE%\.tauri\mote.key` — **не коммитить**.
- Опционально password → env `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

Перед **любым** `tauri build` с `createUpdaterArtifacts: true`:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "$env:USERPROFILE\.tauri\mote.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""   # если без пароля
```

Без ключа release-сборка упадёт на создании `.sig`.

## Endpoint

Репо: https://github.com/alekseichmsk/mote  

`https://github.com/alekseichmsk/mote/releases/latest/download/latest.json`

## Каждый релиз

1. Синхронно bump version: `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`.
2. `npm run tauri build` (с env ключа).
3. Из `target/release/bundle/nsis/` (или sandbox cargo-target):
   - `Mote_<ver>_x64-setup.exe`
   - `Mote_<ver>_x64-setup.exe.sig`
4. Собери `latest.json`:

```powershell
node scripts/make-latest-json.mjs `
  --version 0.1.1 `
  --setup path\to\Mote_0.1.1_x64-setup.exe `
  --owner alekseichmsk --repo mote `
  --out latest.json
```

5. GitHub Release tag `v0.1.1`, assets:
   - setup.exe
   - setup.exe.sig (удобно держать рядом)
   - **`latest.json`** (имя файла именно так — для `…/latest/download/latest.json`)

## Проверка

1. Установи старую версию.
2. Опубликуй новую на Releases.
3. Запусти старую → через ~8 с лог `update available` → трей «Установить обновление» → перезапуск с новой версией.
