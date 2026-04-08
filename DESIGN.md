# Design System — NyxID

## Product Context
- **What this is:** Auth/SSO platform with credential management, proxy, AI agent isolation, and approval workflows
- **Who it's for:** Developers, DevOps engineers, security engineers managing AI agent access to external services
- **Space/industry:** Identity & access management (peers: Clerk, Auth0, WorkOS, Stytch)
- **Project type:** SaaS dashboard / developer tool

## Aesthetic Direction
- **Direction:** Refined dark mode with warm purple identity
- **Decoration level:** Intentional — purple used sparingly on active states, borders, badges, logo. Not on every surface or hover.
- **Mood:** Night goddess energy. Precise, confident, measured use of color. Purple as identity, not wallpaper. The kind of product a security team trusts because it looks like it takes itself seriously.
- **Key principle:** Color is earned. Semantic colors (green/amber/red) do the heavy lifting for status. Purple marks identity and interaction, nothing else.

## Color

### Primary Accent
- **Primary:** `#9775fa` — warm violet (NOT #8b5cf6 which is the AI-default Tailwind violet-500)
- **Primary light:** `#c4b5fd` — for logo wordmark, light text on accent backgrounds
- **Primary deep:** `#7c5ce0` — for hover states, pressed states

### Backgrounds (3-layer depth)
- **Sidebar:** `#06060b` — darkest layer
- **Base:** `#07060e` — main background
- **Surface/Card:** `#0c0b14` — elevated content

### Text Hierarchy (4 levels)
- **Primary:** `#e8e4f0` — headings, important text (warm off-white, NOT pure white)
- **Secondary:** `#9e96b0` — body text, descriptions
- **Muted:** `#7a7490` — supporting text, metadata
- **Tertiary:** `#4a4460` — timestamps, disabled text, section labels

### Borders
- **Default:** `#1c1828` — card borders, dividers

### Semantic Status
- **Success/Online:** `#34d399` — active services, healthy nodes, approved grants
- **Warning/Expiring:** `#f59e0b` — expiring tokens, pending approvals, attention items
- **Error/Expired:** `#f87171` — expired tokens, failed requests, denied approvals
- **Info/Auth:** `#60a5fa` — informational badges, auth events

### Usage Rules
- Purple accent ONLY on: active navigation state (inset left border + tint background), pending approval card left border, approval count badges, logo, interactive focus rings
- Purple NOT on: primary action buttons (use semantic colors), every hover state, surface tints on idle cards, decorative backgrounds
- Buttons: Approve = success green background tint, Deny = error red background tint, Secondary = ghost with border, Primary CTA = `#9775fa` solid (rare, only for main page CTAs)

## Typography
- **Display/Hero:** Space Grotesk 500 — page titles, stat values, card headings. Techy and sharp.
- **Body:** Manrope 400 — all body text, descriptions, row content. Geometric but warm.
- **UI/Labels:** Manrope 500 — nav items, button labels, form labels
- **Data/Tables:** JetBrains Mono 400 — timestamps, log entries, API paths, code snippets. Supports tabular-nums.
- **Logo:** Playfair Display 400 — NyxID wordmark only. Letterspace: 1px. Color: `#c4b5fd`.
- **Loading:** Google Fonts CDN
- **Scale:**
  - 10px — section labels (uppercase, tracking 1.2px)
  - 11px — badges, timestamps, stat descriptions, tertiary text
  - 12px — button text, small body
  - 13px — nav items, row content, body text
  - 14px — descriptions, secondary body
  - 15px — card titles (Space Grotesk)
  - 18px — logo wordmark
  - 20px — page title in header
  - 28px — stat values (Space Grotesk)

## Spacing
- **Base unit:** 4px
- **Density:** Comfortable
- **Scale:** 2xs(2px) xs(4px) sm(8px) md(16px) lg(24px) xl(32px) 2xl(48px)
- **Content padding:** 32px horizontal, 28px vertical (desktop)
- **Card padding:** 22px (standard), 18px (stats)
- **Nav item padding:** 10px vertical, 14px horizontal
- **Gap between sections:** 24px
- **Gap between cards:** 16px
- **Gap within cards:** 14px (title to content), 10px (between rows)

## Layout
- **Approach:** Grid-disciplined with sidebar + header shell
- **Sidebar:** 228px width, collapsible admin section, 3 nav groups (Monitor/Manage/Configure)
- **Header:** 56px height, page title left, avatar right
- **Content max width:** None (full width within padding)
- **Border radius:** 10px (cards, panels), 8px (buttons, inputs, nav items), 100px (badges, pills)
- **Stat grid:** 4 columns responsive (4 on lg, 2 on sm, 1 on mobile)

## Navigation Structure
Sidebar organized into functional zones:

**Monitor** — what's happening
- Dashboard (home)
- Activity (event log)

**Manage** — your resources
- AI Services (unified credentials)
- Nodes (proxy nodes)
- Approvals (pending + history + grants, unified)

**Configure** — setup and settings
- Settings (profile, security, sessions, MCP)
- Developer (apps, AI setup, integration guide)
- Guide (documentation)

**Admin** (collapsible, admin-only)
- Users, Audit Log, Service Accounts, Roles, Groups, Nodes, Services, Providers

## Motion
- **Approach:** Minimal-functional
- **Easing:** enter(ease-out) exit(ease-in) move(ease-in-out)
- **Duration:** micro(50-100ms) short(150-200ms) medium(200-300ms)
- **Where used:** nav hover/active transitions, dropdown open/close, sidebar mobile drawer, skeleton loading

## Dashboard Content Strategy
The dashboard shows what's HAPPENING, not what you HAVE:
1. **Stats row** — counts with meaningful descriptions ("All healthy", "3 days remaining"), not just numbers
2. **Activity feed** — recent proxy requests, grants, node events, errors with color-coded dots
3. **Pending approvals** — inline approve/deny with countdown timers, purple left-border accent
4. **Attention items** — expiring tokens, failed requests, with action buttons (Renew, Reconnect)

## Decisions Log
| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-04-01 | Initial design system created | Created by /design-consultation + /design-shotgun. User reviewed 7 directions (4 IA layouts, 3 alternate color palettes, 1 refined purple) and chose refined purple with warm neutrals. |
| 2026-04-01 | Keep purple, shift accent | User explicitly chose to keep purple identity but shift from #8b5cf6 (AI default) to #9775fa (warmer, less saturated). Purple used sparingly, not on every element. |
| 2026-04-01 | Sidebar reorganization | Collapsed from 20 items to 3 zones (Monitor/Manage/Configure) + collapsible Admin. Approvals unified into single nav item. |
| 2026-04-01 | Activity-first dashboard | Replaced static count cards with activity timeline, pending approvals, and attention items. Stats still present but with meaningful descriptions. |