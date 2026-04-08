---
name: ui-design-system
description: |
  Design-focused skill for UI polish, visual consistency, and design system
  enforcement. Use when the UI exists but needs to look significantly better:
  layout refinement, color harmony, spacing consistency, micro-interactions,
  and visual hierarchy improvements.
license: MIT
metadata:
  version: "1.0.0"
  category: design
---

# UI Design System

This skill focuses on making existing UI look production-quality. It is about
visual polish, not feature building.

## When To Use

Use when the subtask involves:
- Making an existing screen look better / more polished
- Enforcing visual consistency across screens
- Fixing layout issues, spacing, or color problems
- Adding micro-interactions and transitions
- Redesigning or beautifying a component/page

## Approach: Audit → Fix → Verify

### Step 1: Visual Audit

Before changing anything, read the existing code and catalog issues:

```
VISUAL_AUDIT:
- [ ] Inconsistent spacing (e.g., some cards use p-4, others p-6)
- [ ] Color palette violations (random hex values instead of theme tokens)
- [ ] Typography inconsistency (mixed font sizes without hierarchy)
- [ ] Missing states (loading, empty, error)
- [ ] Broken responsive layout at specific breakpoints
- [ ] Accessibility gaps (missing labels, low contrast)
- [ ] Alignment issues (text/icons not vertically centered)
- [ ] Clutter (too many visual elements competing for attention)
```

### Step 2: Design Hierarchy

Apply the **visual hierarchy pyramid** to every screen:

```
Level 1 (most prominent):  Primary action / key metric / hero content
Level 2 (secondary):       Navigation / section headers / supporting data
Level 3 (tertiary):        Metadata / timestamps / helper text
Level 4 (background):      Borders / dividers / surface colors
```

Rules:
- Only 1 primary action per viewport
- Reduce visual weight as hierarchy level increases
- Use size, weight, and color to create hierarchy (not decoration)

### Step 3: The 5 Polish Passes

Execute these passes in order:

**Pass 1: Spacing & Alignment**
- Enforce 8px grid (4px for tight UI)
- Consistent padding within component types
- Align text baselines and icon centers
- Equal gap between sibling elements

**Pass 2: Color & Contrast**
- Max 5 colors on any screen (neutral + primary + 2-3 semantic)
- Background layers: `white → gray-50 → gray-100` (max 3 levels)
- Text: `gray-900` primary, `gray-600` secondary, `gray-400` disabled
- Remove any hardcoded hex colors — use theme tokens

**Pass 3: Typography**
- Max 3 font sizes per screen
- Bold for headings, medium for labels, regular for body
- Consistent line-height (Tailwind defaults are fine)
- Truncate long text with `truncate` or `line-clamp-2` — never let text break layout

**Pass 4: Interactive Feedback**
- Every clickable element: cursor-pointer + hover state + focus ring
- Buttons: `hover:bg-*-700` + `active:scale-[0.98]` + `disabled:opacity-50`
- Links: `hover:underline` or `hover:text-*-600`
- Form inputs: `focus:ring-2 focus:ring-blue-500 focus:border-blue-500`
- Transitions: `transition-colors duration-150` on all interactive elements

**Pass 5: Edge States & Polish**
- Loading: skeleton placeholders matching content shape
- Empty: centered icon + message + CTA
- Error: inline red banner with message + retry
- Success: brief toast or green banner
- Micro-interactions: subtle scale on card hover `hover:shadow-md transition-shadow`

### Step 4: Responsive Verification

Check at 3 breakpoints:
- **Mobile** (375px): single column, stacked layout, bottom navigation
- **Tablet** (768px): 2-column where appropriate, sidebar collapses
- **Desktop** (1280px): full layout, sidebar visible, content max-width

### Step 5: Dark Mode (only if project already supports it)

If the project has dark mode infrastructure:
- `dark:bg-gray-900 dark:text-gray-100`
- Borders: `dark:border-gray-700`
- Cards: `dark:bg-gray-800`
- Primary accent stays the same hue, adjust lightness

## Common Patterns to Enforce

**Page layout:**
```html
<div class="min-h-screen bg-gray-50">
  <header class="bg-white border-b border-gray-200">...</header>
  <main class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
    <div class="space-y-6">
      <!-- sections -->
    </div>
  </main>
</div>
```

**Dashboard grid:**
```html
<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
  <div class="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
    <p class="text-sm text-gray-500">Metric Label</p>
    <p class="mt-1 text-2xl font-semibold text-gray-900">1,234</p>
  </div>
</div>
```

**Action bar:**
```html
<div class="flex items-center justify-between">
  <h1 class="text-2xl font-bold text-gray-900">Page Title</h1>
  <button class="inline-flex items-center gap-2 px-4 py-2 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700">
    <PlusIcon class="h-4 w-4" /> Create
  </button>
</div>
```

**Data table:**
```html
<div class="overflow-hidden rounded-lg border border-gray-200">
  <table class="min-w-full divide-y divide-gray-200">
    <thead class="bg-gray-50">
      <tr>
        <th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Name</th>
      </tr>
    </thead>
    <tbody class="divide-y divide-gray-200 bg-white">
      <tr class="hover:bg-gray-50 transition-colors">
        <td class="px-4 py-3 text-sm text-gray-900">...</td>
      </tr>
    </tbody>
  </table>
</div>
```

## Anti-Patterns (DO NOT)

- Gradients on backgrounds (unless brand requires)
- Drop shadows heavier than `shadow-md`
- Rounded corners larger than `rounded-xl`
- More than 2 border styles on a page
- Animated page entrances
- Custom scrollbars
- Overly decorative empty states (simple icon + text is better)
- Using `!important` in CSS

## Review Bar

Ready for review when:
- All 5 polish passes are applied
- Spacing is consistent (8px grid)
- Color palette uses ≤ 5 colors
- Typography uses ≤ 3 sizes
- Every interactive element has hover + focus states
- Responsive at 375px / 768px / 1280px
- No hardcoded hex colors
