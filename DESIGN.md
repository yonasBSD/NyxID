# Design System — NyxID

## Product Context

- **What this is:** Auth/SSO platform with credential management, proxy, AI agent isolation, and approval workflows
- **Who it's for:** Developers, DevOps engineers, security engineers managing AI agent access to external services
- **Space/industry:** Identity & access management (peers: Clerk, Auth0, WorkOS, Stytch)
- **Project type:** SaaS dashboard / developer tool

## Brand Alignment

This design system is derived from the **NyxID Brand Guide** (`/Users/aelf/Documents/NyxID/nyxid-brand-guide/DESIGN_TOKENS.md`). All colors, typography, and gradients must match the brand spec. Do not deviate without explicit approval.

## Aesthetic Direction

- **Direction:** Refined dark mode with deep purple identity
- **Decoration level:** Intentional — purple used sparingly on active states, borders, badges, logo, and CTAs
- **Mood:** Night goddess energy. Precise, confident, measured use of color. Purple as identity, not wallpaper. The kind of product a security team trusts because it looks like it takes itself seriously.
- **Key principle:** Color is earned. Semantic colors (green/amber/red) do the heavy lifting for status. Purple marks identity and interaction, nothing else.

---

## Color Tokens

### Primary Palette — NyxID Purple

| Token | Hex | Usage |
|-------|-----|-------|
| `nyx-50` | `#EDE5FE` | Tinted backgrounds, hover states |
| `nyx-100` | `#D9C7FD` | Light fills, disabled states |
| `nyx-200` | `#B894FB` | Borders, secondary accents |
| `nyx-300` | `#9761F8` | Icons, interactive highlights |
| `nyx-400` | `#7845F5` | Hover states, active indicators |
| `nyx-500` | `#5A2AF1` | **Brand Primary** — buttons, links, key UI, focus rings |
| `nyx-600` | `#4A1FD4` | Pressed states, emphasis |
| `nyx-700` | `#3A16B0` | Dark accents, section backgrounds |
| `nyx-800` | `#2B0F8C` | Deep fills, dark mode surfaces |
| `nyx-900` | `#1E0A68` | High-contrast dark accents |
| `nyx-950` | `#120545` | Near-black brand tones |

### Secondary Palette

| Token | Hex | Usage |
|-------|-----|-------|
| `nyx-secondary-50` | `#F3EAFE` | Light tints |
| `nyx-secondary-100` | `#E4D2FD` | Subtle fills |
| `nyx-secondary-200` | `#CDA9FC` | Light borders |
| `nyx-secondary-300` | `#B98DFB` | Decorative accents |
| `nyx-secondary-400` | `#A672FB` | **Brand Secondary** — gradient starts, eyebrow text |
| `nyx-secondary-500` | `#9555F5` | Supporting accents |
| `nyx-secondary-600` | `#7D3EDB` | Pressed secondary states |
| `nyx-secondary-700` | `#642CB7` | Dark secondary accents |
| `nyx-secondary-800` | `#4C1E93` | Deep secondary fills |
| `nyx-secondary-900` | `#36136F` | High-contrast secondary |
| `nyx-secondary-950` | `#220B4B` | Near-black secondary |

### Backgrounds (Dark Mode)

| Token | Hex | Usage |
|-------|-----|-------|
| `background` | `#0D0D0D` | **NyxID Black** — base background, sidebar |
| `card` / `surface` | `#171717` | Card surfaces, elevated content |
| `muted` / `border` | `#262626` | Borders, muted fills, dividers |

### Text Hierarchy

| Token | Hex | Usage |
|-------|-----|-------|
| `foreground` | `#FAFAFA` | Primary (headings, important text) |
| `muted-foreground` | `#A3A3A3` | Secondary (body text, descriptions) |
| `text-tertiary` | `#525252` | Tertiary (timestamps, labels, section headers) |

### Semantic Status

| Token | Hex | Usage |
|-------|-----|-------|
| `success` | `#10B981` | Active, healthy, approved, online |
| `warning` | `#F59E0B` | Expiring, pending, attention, draining |
| `error` / `destructive` | `#EF4444` | Expired, failed, denied, offline |
| `info` | `#3B82F6` | Informational, auth events |

### Accent

| Token | Hex | Usage |
|-------|-----|-------|
| `nyx-orange` | `#F0924C` | Sparingly for emphasis, sunset gradient end |

### Color Usage Rules

- Purple accent ONLY on: active navigation state (inset left border + tint background), CTA buttons, badges, logo, interactive focus rings, featured card borders
- Purple NOT on: every hover state, surface tints on idle cards, decorative backgrounds
- Semantic buttons: Approve = success green, Deny = error red, Primary CTA = `nyx-gradient-vivid`
- Status dots/badges always use semantic colors, never purple

---

## Gradients

### Brand Gradients

| Name | CSS | Usage |
|------|-----|-------|
| **Vivid** | `linear-gradient(to right, #A672FB 0%, #5E00F5 100%)` | CTAs, hero backgrounds, progress bars, score arcs |
| **Sunset** | `linear-gradient(to right, #A672FB 0%, #F0924C 100%)` | Warm accents, featured cards, highlight elements |
| **Fade** | `linear-gradient(to right, #FFFFFF 30%, #A672FB 70%)` | Section dividers, cards on light backgrounds |

### Gradient Utilities (CSS classes)

| Class | Usage |
|-------|-------|
| `.nyx-gradient-vivid` | Background gradient on elements |
| `.nyx-gradient-sunset` | Warm accent backgrounds |
| `.nyx-gradient-text` | Gradient text (vivid, purple-only) |
| `.nyx-gradient-text-sunset` | Gradient text (purple-to-orange) |

---

## Typography

### Font Families

| Token | Family | Usage |
|-------|--------|-------|
| `--font-sans` | Mona Sans | All UI text (headings, body, labels, buttons) |
| `--font-display` | Mona Sans | Display/hero text (same family, weight 700) |
| `--font-mono` | JetBrains Mono | Code, API paths, key prefixes, timestamps |

### Type Scale

| Token | Size | Weight | Line Height | Usage |
|-------|------|--------|-------------|-------|
| `display` | 4.5rem | 700 | 1.1 | Hero sections, landing pages |
| `headline-lg` | 3.5rem | 700 | 1.15 | Page titles (negative letter-spacing) |
| `headline` | 3rem | 700 | 1.2 | Section titles |
| `title-lg` | 2.25rem | 600 | 1.25 | Card headers, modal titles |
| `title` | 1.75rem | 600 | 1.3 | Subsection headings |
| `title-sm` | 1.25rem | 600 | 1.35 | Widget titles, sidebar headings |
| `body-lg` | 1.125rem | 400 | 1.6 | Lead paragraphs |
| `body` | 1rem | 400 | 1.6 | Default body text |
| `body-sm` | 0.875rem | 400 | 1.5 | Secondary text, descriptions |
| `label` | 0.875rem | 500 | 1.4 | Form labels, nav items, buttons |
| `caption` | 0.75rem | 400 | 1.5 | Timestamps, metadata |
| `overline` | 0.6875rem | 600 | 1.5 | Eyebrow text, section labels |

**Rules:**
- Negative letter-spacing (`-0.02em`) for sizes >= 2.25rem
- No separate display font — Mona Sans at weight 700 serves all heading roles
- Monospace (`JetBrains Mono`) for all code, API keys, paths, and technical values

---

## Spacing & Density

| Property | Value |
|----------|-------|
| Base unit | 4px |
| Density | Comfortable |
| Content padding | `px-6 pt-6 md:px-8 lg:px-10` (24px / 32px / 40px horizontal) |
| Content bottom | `max(2rem, env(safe-area-inset-bottom))` |
| Card padding | 20px (`p-5`) for standard cards |
| Card inner padding | `px-5 py-3` for detail rows |
| Nav item padding | 10px vertical, 12px horizontal |
| Section gap | 24px (`gap-6`) between major sections |
| Card gap | 16px (`gap-4`) between cards |
| Inline gap | 8px (`gap-2`) between inline elements (buttons, badges) |

---

## Border Radius

| Element | Radius | Class |
|---------|--------|-------|
| Cards, panels, dialogs | 12px | `rounded-xl` |
| Buttons, inputs, nav items | 8px | `rounded-lg` |
| Badges, pills, avatars | full | `rounded-full` |
| Table containers | 12px | `rounded-xl` |

---

## Layout

### Shell Structure

```
<div className="flex flex-col h-dvh overflow-hidden bg-background">
  <AmbientStatusLine />  {/* 2px health indicator at viewport top */}
  <TopBar />              {/* 72px header with logo, search, user menu */}
  <div className="flex flex-1 min-h-0 overflow-hidden">
    <Sidebar />           {/* 260px, hidden on mobile */}
    <main />              {/* Full-width scrollable content area */}
    {rightPanel}          {/* Optional 300px right panel */}
  </div>
</div>
```

### Key Dimensions

| Element | Value |
|---------|-------|
| Sidebar width | 260px (hidden below `md`) |
| Top bar height | 72px |
| Content max width | **None** — content fills available width within padding |
| Right panel width | 300px (hidden below `lg`, optional) |
| Stat grid | 4 columns on lg, 2 on sm, 1 on mobile |

### Mobile

- Sidebar hidden, accessible via hamburger menu (slide-in drawer with `bg-black/60 backdrop-blur-sm` overlay)
- Safe area insets respected via CSS env vars (`--sat`, `--sab`, `--sal`, `--sar`)
- Inputs forced to 16px on iOS to prevent auto-zoom

---

## Component Patterns

### Page Structure

Every dashboard page follows this pattern:

```tsx
<PageHeader
  title="Page Title"
  description="Short description"
  actions={<AddCtaButton />}  {/* CTAs always in PageHeader actions slot */}
/>
<div className="space-y-6">
  {/* Content sections */}
</div>
```

- **CTAs** (Add, Create, Connect) always go in the `PageHeader` actions slot — never floating or inline in the body
- Use `AddCtaButton` component for primary CTAs (icon + label, gradient border on hover)

### Cards

Standard card container:

```tsx
<div className="rounded-xl border border-border/50 bg-card overflow-hidden">
  {/* Card header (optional) */}
  <div className="flex items-center justify-between border-b border-border/50 px-5 py-3">
    <div>
      <h3 className="text-sm font-semibold text-foreground">Title</h3>
      <p className="text-[11px] text-muted-foreground mt-0.5">Description</p>
    </div>
    {/* Action button (edit icon, etc.) */}
  </div>
  {/* Card body */}
  <div className="p-5">
    {/* Content */}
  </div>
</div>
```

**Rules:**
- All cards use `rounded-xl border border-border/50 bg-card overflow-hidden`
- Card headers: `border-b border-border/50 px-5 py-3`
- Card bodies: `p-5`
- Featured/identity cards: add `border-nyx-500/20 bg-nyx-500/[0.04]` with `hover:border-nyx-500/40`

### Detail Sections (DetailSection / DetailRow)

For info/detail display in detail pages:

```tsx
<DetailSection title="Section Title" description="Optional subtitle">
  <DetailRow label="Field Name" value={fieldValue} />
  <DetailRow label="Status" value={<Badge>Active</Badge>} />
</DetailSection>
```

- `DetailRow` renders as `px-5 py-3 text-[13px]` with label on left, value on right
- Dividers between rows: `border-b border-border/50` (except last row)
- All detail sections wrapped in standard card container

### Empty States

```tsx
<div className="rounded-lg border border-dashed border-border p-4 text-center text-xs text-muted-foreground">
  No items configured.
</div>
```

- Dashed border for empty lists/tables
- Centered text, `text-xs text-muted-foreground`

---

## Tables

### Structure

```tsx
<div className="rounded-xl border border-border/50 bg-card overflow-hidden">
  <Table>
    <TableHeader>
      <TableRow className="border-border/50 hover:bg-transparent">
        <TableHead className="w-[XX%]">Column</TableHead>
        <TableHead className="w-10">Actions</TableHead>  {/* Always visible */}
      </TableRow>
    </TableHeader>
    <TableBody>
      <TableRow className="border-border/30 cursor-pointer hover:bg-white/[0.03]">
        {/* cells */}
      </TableRow>
    </TableBody>
  </Table>
</div>
```

### Table Rules

1. **Column widths:** Always use explicit percentage widths (`w-[XX%]`) on `<TableHead>` to prevent layout shift
2. **Actions column:** Only present when the table has row actions. When present, always labeled "Actions" (never empty or sr-only). Use `w-10` for the actions column. If a table has no actions, omit the actions column entirely.
3. **Cell alignment:** Default `align-top` for cells with multi-line content, `align-middle` otherwise
4. **Row hover:** `hover:bg-white/[0.03]` for clickable rows
5. **Row borders:** `border-border/30`
6. **Monospace values:** Use `font-mono text-xs` or `font-mono text-[11px]` for keys, IDs, URLs

### Table Actions

- **0-1 actions:** Inline button (ghost variant, icon-only)
- **2+ actions:** Three-dot dropdown menu (`MoreHorizontal` icon trigger)

Three-dot dropdown pattern:

```tsx
<DropdownMenu>
  <DropdownMenuTrigger asChild>
    <Button variant="ghost" size="icon" className="h-7 w-7">
      <MoreHorizontal className="h-3.5 w-3.5" />
      <span className="sr-only">Actions for {item.name}</span>
    </Button>
  </DropdownMenuTrigger>
  <DropdownMenuContent align="end">
    <DropdownMenuItem onClick={handleAction}>
      <Icon className="mr-2 h-4 w-4" /> Label
    </DropdownMenuItem>
    <DropdownMenuItem className="text-destructive focus:text-destructive">
      <Trash2 className="mr-2 h-4 w-4 text-destructive" /> Delete
    </DropdownMenuItem>
  </DropdownMenuContent>
</DropdownMenu>
```

---

## Buttons & Actions

### Button Variants

| Variant | Usage |
|---------|-------|
| `primary` | Primary actions (Save, Create, Connect) |
| `outline` | Secondary actions (Cancel, Back, Edit) |
| `ghost` | Inline/subtle actions (table row actions, icon buttons) |
| `destructive` | Dangerous actions (Delete, Revoke) |
| `link` | Text-only navigation actions |

### Button Ordering Rule

**Cancel / secondary always left. Save / primary always right.**

```tsx
<div className="flex justify-end gap-2">
  <Button variant="outline" onClick={onCancel}>Cancel</Button>
  <Button variant="primary" onClick={onSave} disabled={!hasChanges}>Save</Button>
</div>
```

This applies to all forms, dialogs, inline editors, and card footers without exception.

### CTA Button (AddCtaButton)

Primary call-to-action with icon, used in `PageHeader` actions slot:

```tsx
<AddCtaButton onClick={handleAdd} disabled={atLimit}>
  Add service
</AddCtaButton>
```

- Supports `disabled` prop: `disabled:pointer-events-none disabled:opacity-40`
- Uses `Plus` icon by default

### Submit/Save Button State

- **Disabled** when no changes have been made or required fields are empty
- **Loading** state with spinner during mutation
- Never allow submitting an unchanged form

---

## Forms

### Input Controls

- Text inputs: shadcn `<Input>` with `font-mono text-xs` for technical values
- Dropdowns: shadcn `<Select>` — never use native radio buttons; use `<Select>` for single-choice fields
- Checkboxes: shadcn `<Checkbox>` for boolean toggles
- Multi-line: shadcn `<Textarea>`

### Validation

- Client-side: Zod schemas (`schemas/` directory)
- Form handling: React Hook Form with `@hookform/resolvers`
- Inline errors: `text-[11px] text-destructive` below the input
- Server errors: `toast.error()` via Sonner

---

## Glassmorphism & Depth

### Surface Classes

| Class | Usage |
|-------|-------|
| `.glass-surface` | Semi-transparent card with 20px blur |
| `.glass-elevated` | Higher-contrast panel with 40px blur, purple rim, shadow |
| `.hover-glow` | Cards that glow on hover (border + shadow transition) |

### Ambient Glow

| Class | Usage |
|-------|-------|
| `.ambient-glow` | Purple radial glow (default/identity) |
| `.ambient-glow-success` | Green radial glow (healthy state) |
| `.ambient-glow-warning` | Amber radial glow (attention state) |
| `.ambient-glow-critical` | Red radial glow (critical state) |

### Rim Lights

| Class | Usage |
|-------|-------|
| `.rim-light-top` | Subtle purple top border (cards, sections) |
| `.rim-light-left` | 2px purple left border (active nav, focus indicators) |

---

## Animations

| Class | Duration | Usage |
|-------|----------|-------|
| `.animate-fade-up` | 400ms ease-out | Card/section entrance (staggered with `animation-delay`) |
| `.animate-pulse-subtle` | 3s ease-in-out infinite | Attention indicators, critical status line |
| `.animate-score-draw` | 1.5s cubic-bezier | SVG score arc drawing animation |
| `.animate-constellation-pulse` | 2s ease-in-out infinite | Orbital visualization dots |

### Motion Principles

- **Approach:** Minimal-functional — motion serves comprehension, not decoration
- **Easing:** enter = ease-out, exit = ease-in, move = ease-in-out
- **Duration:** micro = 50-100ms, short = 150-200ms, medium = 200-300ms
- Stagger card entrances with incremental `animation-delay` for grouped items

---

## Dashboard-Specific Components

### Security Posture Card

The hero component users see first. Compact horizontal layout:

```
┌──────────────────────────────────────────────────────────┐
│  [ScoreRing 80px]  SECURITY POSTURE                      │
│                    Your setup is almost complete          │
│                    ████████████░░░░  (gradient bar)       │
│                    ✉ Email  🛡 MFA  🔧 Services  🔑 Keys │
└──────────────────────────────────────────────────────────┘
```

- Background: `border-nyx-500/20 bg-nyx-500/[0.04]`, hover: `border-nyx-500/40`
- Score ring: 80px SVG, strokeWidth 5, purple-only gradient (`#5A2AF1` -> `#A672FB` -> `#C4A0FF`)
- Progress bar: continuous `nyx-gradient-vivid` fill on `bg-white/[0.06]` track, `h-1.5 rounded-full`
- Checklist items: icons (Mail, Shield, Server, KeyRound), no bullet points
- Score thresholds: 100 = "fully secured", 75+ = "almost complete", 50+ = "needs attention", <50 = "action required"

### Metric Cards

Glassmorphic stat cards in a 4-column grid:

- Container: `glass-surface` + `hover-glow`, `rounded-2xl`, `min-h-[100px]`
- Overline: `text-[11px] uppercase tracking-[1.5px]` in `text-muted-foreground`
- Value: `text-[22px] font-bold`
- Footer: `text-[11px]` description or delta

---

## Scrollbar

Custom dark-mode scrollbar:

- Width/height: 8px
- Track: transparent
- Thumb: `--color-muted` (#262626), hover: `--color-muted-foreground` (#A3A3A3)
- Hide with `.scrollbar-none` class

---

## Logo Assets

| File | Usage |
|------|-------|
| `/nyxid-coloured-logo.svg` | Gradient icon + white wordmark (sidebar, auth pages) |
| `/nyxid-coloured-icon.svg` | Gradient N-mark (favicons) |
| `/nyxid-logo-white.svg` | White icon + wordmark (dark backgrounds) |
| `/nyxid-icon-white.svg` | White N-mark (small formats) |
| `/nyxid-app-icon.svg` | App stores, splash screens |

**Clear space:** Height of the 'N' on all sides. **Minimum size:** 24px height (digital), 10mm (print).

---

## Voice & Tone

### Principles

- **Direct** — Lead with the point. Short sentences. Active voice.
- **Technical, Not Jargony** — Use real terms, let context explain.
- **Security-Minded** — Frame features through what they protect.
- **Inclusive & Open** — Built for solo devs to enterprises. No gatekeeping.

### Do

- Lead with the benefit: *"Share services, not secrets"*
- Use imperative mood: *"Connect GitHub. Scope the identity. Ship."*
- Ground claims in concrete examples
- Speak to builders — assume technical fluency

### Don't

- Use vague security language: *"enterprise-grade"*, *"military-grade"*
- Bury value behind feature lists
- Talk down or over-explain fundamentals
- Promise absolute security

---

## Decisions Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-04-01 | Initial design system created | User chose refined purple with warm neutrals |
| 2026-04-01 | Purple identity shift | From #8b5cf6 (Tailwind default) to #9775fa (warmer) |
| 2026-05-05 | Brand guide alignment | Colors shifted to brand spec (#5A2AF1/#A672FB), Mona Sans, brand gradients, brand logos |
| 2026-05-08 | Button ordering rule | Cancel/secondary left, Save/primary right — enforced across all forms and dialogs |
| 2026-05-08 | Table action columns | Actions column only when table has row actions; when present, always labeled "Actions" visibly |
| 2026-05-08 | Table action dropdown rule | Tables with 2+ row actions use MoreHorizontal 3-dot dropdown, not inline buttons |
| 2026-05-08 | No native radio buttons | All single-choice fields use shadcn Select dropdown instead of native radio inputs |
| 2026-05-08 | CTA placement rule | Primary CTAs (Add, Create) always in PageHeader actions slot via AddCtaButton |
| 2026-05-08 | Full-width content area | Removed max-w-[960px] constraint — content fills available width within padding |
| 2026-05-08 | Card styling standardized | All cards use `rounded-xl border-border/50 bg-card overflow-hidden` |
| 2026-05-08 | DetailSection pattern | Info/detail display uses DetailSection/DetailRow with consistent `px-5 py-3 text-[13px]` |
| 2026-05-08 | Security Posture card redesign | Compact horizontal layout, purple-only gradient bar, icon checklist, `bg-nyx-500/[0.04]` |
| 2026-05-08 | Submit disabled rule | Save/Submit buttons disabled when no changes exist or required fields are empty |
| 2026-05-11 | Design system comprehensive update | Added component patterns, table rules, glassmorphism tokens, full brand color scales, animation catalog |
