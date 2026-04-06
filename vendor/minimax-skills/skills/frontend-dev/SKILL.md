---
name: frontend-dev
description: |
  Packaged frontend implementation guide for AI Dev Hub.
  Use for UI-heavy subtasks: pages, dashboards, forms, landing sections,
  component composition, responsive layout, interaction polish, and frontend
  integration work aligned with modern design standards.
license: MIT
metadata:
  version: "3.0.0"
  category: frontend
---

# Frontend Dev

This vendored skill is a compact execution guide for frontend subtasks inside
AI Dev Hub. It produces production-quality UI that looks modern and polished
out of the box.

## When To Use

Use this skill when the current subtask is primarily about:

- building or updating a screen, page, modal, form, table, or dashboard
- wiring UI state to existing APIs
- improving responsive layout, empty/loading/error states, and interaction quality
- refining visual hierarchy, typography, spacing, and motion

Do not use it as a reason to redesign unrelated parts of the app.

## Operating Rules

1. Read the local project first.
2. Follow the repo's existing framework, routing, state, and styling conventions.
3. Prefer modifying existing components over introducing parallel abstractions.
4. Keep the write scope bounded to the current subtask.
5. If backend changes are required, coordinate only through the project files and blackboard state.

## Technology Selection (when no existing preference)

When the project has no established frontend or you are starting fresh:

**Preferred stack (unless plan specifies otherwise):**
- **React + Tailwind CSS** — default for web apps
- **shadcn/ui** components if React is used — modern, accessible, customizable
- **Lucide icons** — consistent, lightweight icon set
- If the plan specifies Vue/Svelte/other, follow the plan but still apply the design principles below

**Do NOT reach for:**
- Heavy UI frameworks (Material UI, Ant Design) unless the plan specifically requires them
- CSS-in-JS (styled-components, emotion) when Tailwind is available
- Custom icon systems when Lucide/Heroicons exist

## Design System Principles

### Color

- Use a **neutral base** (slate/zinc/gray) for backgrounds and text
- Use **one primary accent color** for CTAs and interactive elements
- Use **semantic colors** for status: green=success, red=error, amber=warning, blue=info
- Dark text on light backgrounds: `text-gray-900` on `bg-white`, not gray-on-gray
- Ensure contrast ratio ≥ 4.5:1 for text (WCAG AA)

Example Tailwind palette:
```
bg-white / bg-gray-50 / bg-gray-100    — surface layers
text-gray-900 / text-gray-600          — primary / secondary text
bg-blue-600 / hover:bg-blue-700        — primary action
bg-red-50 text-red-700                  — error state
bg-green-50 text-green-700             — success state
```

### Typography

- **System font stack**: `font-sans` (Inter, system-ui) — do not import custom fonts unless required
- **Size scale**: Use Tailwind's scale consistently:
  - Page title: `text-2xl font-bold` or `text-3xl font-bold`
  - Section heading: `text-lg font-semibold`
  - Body: `text-sm` or `text-base`
  - Caption / helper: `text-xs text-gray-500`
- **Line height**: Use Tailwind defaults (`leading-normal`, `leading-relaxed`)
- **Never** use more than 3 font sizes on one screen

### Spacing & Layout

- Use **8px grid** (`p-2 = 8px`, `p-4 = 16px`, `p-6 = 24px`, `gap-4 = 16px`)
- Page padding: `px-4 sm:px-6 lg:px-8`
- Card padding: `p-4` or `p-6`
- Section gap: `space-y-6` or `gap-6`
- Max content width: `max-w-7xl mx-auto` for page content
- **Consistent spacing** — if cards use `p-4`, all cards use `p-4`

### Components

Follow these patterns for common components:

**Card:**
```html
<div class="bg-white rounded-lg border border-gray-200 shadow-sm p-6">
```

**Button (primary):**
```html
<button class="inline-flex items-center px-4 py-2 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 disabled:opacity-50 disabled:cursor-not-allowed">
```

**Input:**
```html
<input class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm placeholder-gray-400 focus:border-blue-500 focus:ring-1 focus:ring-blue-500" />
```

**Badge / Tag:**
```html
<span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-blue-100 text-blue-800">
```

**Empty state:**
```html
<div class="text-center py-12">
  <Icon class="mx-auto h-12 w-12 text-gray-400" />
  <h3 class="mt-2 text-sm font-semibold text-gray-900">No items</h3>
  <p class="mt-1 text-sm text-gray-500">Get started by creating a new item.</p>
  <button class="mt-4 ...">Create item</button>
</div>
```

**Table:**
```html
<table class="min-w-full divide-y divide-gray-200">
  <thead class="bg-gray-50">
    <tr><th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
```

### Animation & Transitions

- Use `transition-colors duration-150` on hover effects
- Use `transition-all duration-200` on expand/collapse
- Loading: use `animate-spin` for spinners, `animate-pulse` for skeletons
- **No** page-level transitions unless the plan requires them
- **No** decorative animations (bouncing, fading in text, parallax)

### Responsive

- **Mobile-first**: write base styles for mobile, add `sm:`, `md:`, `lg:` breakpoints
- Sidebar → bottom nav or hamburger on mobile
- Table → card list on mobile (`hidden md:table-cell` for non-essential columns)
- Form → full-width stack on mobile, 2-column grid on desktop
- Touch targets: minimum `h-10 w-10` (40px) for interactive elements

### Accessibility

- All images: `alt` text (or `alt=""` for decorative)
- Form inputs: associated `<label>` or `aria-label`
- Focus visible: `focus:ring-2 focus:ring-offset-2`
- Color alone never conveys meaning (add icon or text)
- Button vs link: `<button>` for actions, `<a>` for navigation
- Semantic HTML: `<nav>`, `<main>`, `<header>`, `<section>`, `<article>`

## State Handling

Every interactive screen MUST implement all 4 states:

1. **Loading**: Skeleton placeholders or spinner — never blank screen
2. **Empty**: Illustration + message + CTA — never just "No data"
3. **Error**: Red banner with message + retry button — never silent failure
4. **Success**: The actual content with proper layout

```
if (isLoading) return <Skeleton />
if (error) return <ErrorBanner message={error} onRetry={refetch} />
if (data.length === 0) return <EmptyState onCreate={handleCreate} />
return <DataList items={data} />
```

## Data & Integration

- Treat API calls, auth state, and async transitions as failure-prone by default
- Show pending state during async work (disabled button + spinner)
- Surface actionable error messages instead of silent failure
- Validate required fields in the UI before submission
- Optimistic UI updates only when rollback is safe

## Review Bar

The subtask is only ready for review when:

- the requested UI surface exists and is reachable
- it looks visually polished (consistent spacing, proper colors, no broken layout)
- data flow is wired correctly
- all 4 states (loading, empty, error, success) are implemented
- responsive layout works on mobile and desktop
- accessibility basics are met (labels, focus, keyboard)
- the change matches the local project style closely enough to ship

## Delivery

At completion, summarize:

- what user-facing UI was added or changed
- what data/API wiring was implemented
- which files were touched
- screenshots or layout description of the final result
