---
tags: [design, rive, pet]
status: active
updated: 2026-10-12
---

# Rive pet ([rive.app](https://rive.app/))

## Цель

Персонаж на Rive runtime (`@rive-app/react-canvas`), moods из `PetMood` → state machine / animations. SVG — fallback.

## Сейчас

- Ассет: `src/assets/rive/flying-pet.riv` (community floating pet).
- `PetRive.tsx` + `PetAvatar` (Rive first, SVG on load error).
- Логи: `[mote] rive ready sm=…`, `rive load failed`.

## Свой Mote

1. Собрать розовое облако в [Rive Editor](https://rive.app/) со state machine (idle / talk / focus / music / angry…).
2. Положить `src/assets/rive/mote.riv`.
3. В `PetRive.tsx` сменить `MOTE_RIVE_SRC` на `mote.riv`.

## Файлы

`PetRive.tsx`, `PetAvatar.tsx`, `assets/rive/*`, `package.json` (`@rive-app/react-canvas`)
