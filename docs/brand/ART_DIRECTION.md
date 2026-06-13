# PhoneBridge — Art Direction

**Direction: "Indigo local-first".** Trustworthy, modern, lightly technical. The visual language says
*your data, unified and safe, on your machine* — deep indigo as the anchor, an electric cyan→violet
accent for the "bridge" / flow of data, generous whitespace, tabular numbers for the data-heavy surfaces.

## Principles

1. **Local-first calm** — no alarmist red-by-default, no dark-pattern urgency. Confidence, not pressure.
2. **Data is the hero** — numbers use tabular figures; cards breathe; the gallery/library is edge-to-edge.
3. **One accent, used sparingly** — indigo carries the brand; the cyan→violet gradient appears only on
   the logo mark, primary actions, and "flow" moments (consolidation, dedup).
4. **Honest empty states** — every screen has a designed empty/loading state (privacy-first apps are
   often empty on first run).

## Color tokens

| Token | Hex | Use |
|---|---|---|
| `--ink` | `#1B1F3B` | Primary text (deep indigo-navy) |
| `--ink-soft` | `#525A7A` | Secondary text |
| `--muted` | `#7A8398` | Tertiary / captions |
| `--bg` | `#F6F7FB` | App background (cool near-white) |
| `--surface` | `#FFFFFF` | Cards, panels |
| `--sidebar-1` | `#1E1B4B` | Sidebar gradient start (indigo-950) |
| `--sidebar-2` | `#312E81` | Sidebar gradient end (indigo-900) |
| `--primary` | `#4F46E5` | Primary action, active state (indigo-600) |
| `--primary-700` | `#4338CA` | Primary hover |
| `--accent-cyan` | `#22D3EE` | Gradient / flow accent |
| `--accent-violet` | `#A855F7` | Gradient / flow accent |
| `--success` | `#10B981` | "Safe to purge", consolidated |
| `--warning` | `#F59E0B` | Size mismatch, attention |
| `--danger` | `#EF4444` | Destructive confirm only |
| `--border` | `rgba(27,31,59,0.10)` | Hairlines |

**Brand gradient:** `linear-gradient(135deg, #6366F1 0%, #22D3EE 100%)` — logo mark, primary buttons,
flow visuals. **Sidebar gradient:** `linear-gradient(180deg, #1E1B4B 0%, #312E81 100%)`.

## Typography

- **Family:** Inter (UI), `ui-monospace` for paths/hashes/status codes.
- **Display headings:** weight 800, `letter-spacing: -0.04em`, tight line-height (0.95).
- **Numbers / stats:** `font-variant-numeric: tabular-nums`, weight 700.
- **Body:** 1rem / 1.6, `--ink-soft`.

## Shape & depth

- Radius: cards `20px`, buttons/inputs `14px`, pills `999px`, logo tile `22%`.
- Elevation: one soft shadow only — `0 20px 60px rgba(27,31,59,0.10)`.
- Hairline borders over heavy strokes.

## Logo

Rounded-square tile, brand gradient, with a **bridge arc** linking two nodes (two devices) that converge
into a single funnel (consolidation). Wordmark: "PhoneBridge", weight 800, `-0.03em`.
Source: [`app-icon.svg`](app-icon.svg). Generate the platform set with `npm run tauri icon` (see backlog
`PB-BRAND-ICON-GEN`).

## Iconography & motifs

- **Consolidation motif:** several stacked sources → one library (used in hero + Cleanup view).
- **Dedup motif:** overlapping tiles collapsing into one.
- **Provenance motif:** a single file linked back to the backups it came from.

## Do / Don't

- ✅ Indigo ink on near-white; gradient only for brand/flow moments.
- ✅ Tabular numbers, generous spacing, designed empty states.
- ❌ No stock "AI gradient soup", no neon-on-black everywhere, no red as a default accent.
