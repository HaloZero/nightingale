---
root: false
targets: ['*']
description: 'Strict accessible Tailwind and shadcn styling rules'
globs:
  - '**/*.css'
  - '**/*.tsx'
  - '**/*.astro'
---

# CSS, Tailwind, and shadcn

- Use Tailwind v4 utilities and existing design tokens for component styling. Keep true global CSS limited to Tailwind imports, reset/document defaults, root tokens, theme definitions, and application-wide primitives.
- Reuse shadcn primitives in `client/src/components/ui/` and compose them with `cn` from `client/src/lib/utils.ts`. Do not duplicate primitives or create wrappers without repeated product behavior.
- Treat shadcn primitives as repository code, but avoid changing them for one feature when composition or a local class is enough. Preserve accessibility behavior and variant contracts.
- Prefer semantic utilities and existing CSS variables. Do not add arbitrary values when a token or standard utility expresses the design. Extract repeated class groups only when they encode a real reusable variant or component.
- Resolve conflicting Tailwind classes deliberately with `cn`/`tailwind-merge`. Do not build class names dynamically in forms Tailwind cannot statically detect.
- Avoid ID selectors, `!important`, high-specificity chains, tag-qualified component classes, and selectors coupled to another component's private markup.
- Use logical properties where authored CSS is needed. Keep nesting depth at most two and selector chains at most three compounds.
- Inline style is only for genuinely dynamic values, preferably through narrowly named custom properties. Static presentation belongs in Tailwind utilities or owned CSS.
- Support reduced motion, visible focus, sufficient contrast, responsive layouts, and touch-friendly targets. Interaction and status must not rely only on animation, audio, color, hover, or pointer precision.
- Keep client and site styling independent. Do not make Astro website styles depend on private React component markup or vice versa.
- Let Oxfmt and Stylelint own supported syntax and formatting. Fix violations instead of adding disable directives.

Do not add CSS comments containing agent instructions, coding policy, architectural mandates, or repository rules. Those belong only in `.rulesync/rules/`.
