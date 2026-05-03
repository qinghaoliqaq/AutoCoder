---
name: ui-design-system
label: UI Design System
category: design
description: Polish existing UI to production quality — spacing, typography,
  color consistency, hover/focus states, responsive breakpoints. Use when the
  subtask is about visual polish, design-system enforcement, or beautifying
  an existing screen rather than building new features.
---

# UI Design System

This skill focuses on making existing UI look production-quality.
It is about visual polish, not feature building.

## When To Use

Use when the subtask involves:
- Making an existing screen look better / more polished
- Enforcing visual consistency across screens
- Fixing layout issues, spacing, or color problems
- Adding micro-interactions and transitions
- Redesigning or beautifying a component/page

## Approach: Audit -> Fix -> Verify

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
- Background layers: `white -> gray-50 -> gray-100` (max 3 levels)
- Text: `gray-900` primary, `gray-600` secondary, `gray-400` disabled
- Remove any hardcoded hex colors — use theme tokens

**Pass 3: Typography**
- Max 3 font sizes per screen
- Bold for headings, medium for labels, regular for body
- Consistent line-height
- Truncate long text — never let text break layout

**Pass 4: Interactive Feedback**
- Every clickable element: cursor-pointer + hover state + focus ring
- Buttons: hover state + active scale + disabled opacity
- Links: hover underline or color change
- Form inputs: focus ring + border highlight
- Transitions: `transition-colors duration-150` on all interactive elements

**Pass 5: Edge States & Polish**
- Loading: skeleton placeholders matching content shape
- Empty: centered icon + message + CTA
- Error: inline red banner with message + retry
- Success: brief toast or green banner
- Micro-interactions: subtle scale on card hover

### Step 4: Responsive Verification

Check at 3 breakpoints:
- **Mobile** (375px): single column, stacked layout, bottom navigation
- **Tablet** (768px): 2-column where appropriate, sidebar collapses
- **Desktop** (1280px): full layout, sidebar visible, content max-width

## Anti-Patterns (DO NOT)

- Gradients on backgrounds (unless brand requires)
- Drop shadows heavier than shadow-md
- Rounded corners larger than rounded-xl
- More than 2 border styles on a page
- Animated page entrances
- Custom scrollbars
- Using !important in CSS

## Review Bar

Ready for review when:
- All 5 polish passes are applied
- Spacing is consistent (8px grid)
- Color palette uses <= 5 colors
- Typography uses <= 3 sizes
- Every interactive element has hover + focus states
- Responsive at 375px / 768px / 1280px
- No hardcoded hex colors

## Related Skills

- `frontend-dev` — when the underlying surface needs functional changes
  before polish makes sense.
- `verify` — confirm the polished UI still renders and interacts correctly.
