---
tags: [bug, layout]
status: patch
updated: 2026-10-12
---

# layout_auto ломает GitHub URL

## Симптом

`https://github.com/alekseichmsk/mote.git` → куски path становились кириллицей (`ьщеуюпше` и т.п.). `github.com` скипался, остальное — нет.

## Причина

`/` режет URL на токены; force-mixed конвертировал `alekseichmsk` / `mote.git`. Целое поле с `://` не считалось URL.

## Фикс

- `looks_like_url_or_code`: одно «слово» с `://` / `git@` / `.git` → skip всего поля
- `url_spans` + skip токенов внутри span
- `.git` в url-token / tld

## Проверка

Авто-раскладка on → вставить ссылку → без замены; лог `layout_auto skip` / отсутствие `ok «alekseichmsk»`.
