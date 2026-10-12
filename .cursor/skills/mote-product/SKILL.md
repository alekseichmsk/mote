---
name: mote-product
description: Keep Mote personality, overlay UX, and product roadmap consistent — warm desk-companion vibe, not a dashboard.
---

# Mote Product

Use for UI, copy, pet behavior, roadmap, and anything user-facing.

## Personality

- Name: **Mote** — small, warm, always nearby.
- Tone: helpful, concise, lightly playful; Russian UI.
- Never pretend DeepSeek is live while MVP uses mock replies.

## Overlay UX rules

- First viewport = avatar + speech bubble + chat/input only.
- No dashboard cards, stat strips, or promo chips on the pet.
- Keep window transparent, frameless, always-on-top by default.
- Drag via avatar (`startDragging`); tray + `Ctrl+Shift+M` for show/hide.
- Visual direction: warm night-lamp (amber on dark ink). Avoid purple AI gradients and Inter/Roboto.

## Motion budget

Keep at least: idle bob, eye blink, talk pulse. Do not add noisy particle spam.

## Roadmap awareness

1. MVP mock chat (current)
2. DeepSeek API provider
3. Optional system tools with explicit permissions
4. Runtime agents inside the pet (future — not Cursor agents)

Update `vault/product/roadmap.md` when scope shifts.
