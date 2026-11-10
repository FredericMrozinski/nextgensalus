# Theming

Salus comes with several themes, and the user picks one in the settings (**Settings → Salus → Theme**). Plugins follow it **without any code**: Salus adds
a stylesheet to every plugin page and sets the `data-theme` attribute of the page's `<html>` element to the theme's id, and changes it when the user picks another theme.
The change applies at once, in every open page.

| Theme id | Look |
|---|---|
| `dark` | Neutral dark with a blue accent. The default. |
| `light` | Neutral light with a purple accent. |

Every theme defines the same variables below, so a plugin that only uses the variables looks right in all of them. If you need to treat light and dark
themes differently, use `color-scheme` (`getComputedStyle(document.documentElement).colorScheme` is `light` or `dark`) rather than the theme ids, because themes can be added.

## What you get without any CSS

The stylesheet styles plain HTML, so a page with no CSS of its own already has the Salus look:

- the **page** is a panel: `--salus-depth-background-1`, the theme's text color and font;
- **buttons** are raised (a rounded outline, light at the top and dark at the bottom). `class="primary"` fills one with the
  accent color for the main action, `class="danger"` colors a destructive one's text, `class="primary danger"` fills it red;
- **fields** (`input`, `select`, `textarea`) are sunk into what they sit on (`--salus-depth-default`);
- an **`<hr>`** is a groove: a dark line over a light one.

## Building blocks

For the rest, put one of these classes on an element. You never need to name a color, and the page follows the theme
and any later change of the look.

| Class | What it is | Looks like |
|---|---|---|
| `salus-header` | The bar at the top of a panel (title, buttons, search) | `foreground-1`, a groove below |
| `salus-footer` | The bar at the bottom of a panel (status line, actions) | `foreground-1`, a groove above |
| `salus-section` | A side bar, or a block of controls | `foreground-1` |
| `salus-title` | The tiny title at the top of a section or card | small, uppercase, muted |
| `salus-list` | A list of rows | the panel's `background-1` |
| `salus-item` | A row of a list. Add `selected` (or `aria-selected="true"`, `aria-current`) to the current one | `default` with a groove below; hover `foreground-1`; selected `foreground-2` with an accent bar at its left |
| `salus-well` | Content set into the panel: a text or details pane | `default` |
| `salus-sunken` | Something one level below what it sits in (a thread under a row) | `background-1` |
| `salus-card` | A card | `foreground-1` with a raised outline like a button's |
| `salus-vseparator` | A vertical line between side-by-side sections | a vertical groove |
| `salus-vseparator salus-arrow` (with an empty `<span>` inside) | The same, with a tip at its middle pointing right: the section on the right follows from the one on the left | set `--salus-left` and `--salus-right` to the two sections' backgrounds (`foreground-1` by default) |

```html
<header class="salus-header"><button>Refresh</button><input placeholder="Filter…"></header>
<ul class="salus-list">
  <li class="salus-item selected">first.svs</li>
  <li class="salus-item">second.svs</li>
</ul>
<footer class="salus-footer">2 images</footer>
```

## Depth

Backgrounds follow one rule: what sits in front is lighter, what lies deeper is darker, in light and dark themes alike.
There are six levels, from front to back:

| Level | Variable | Used for |
|---|---|---|
| foreground-2 | `--salus-depth-foreground-2` | The selected row; hover in menus and dialogs |
| foreground-1 | `--salus-depth-foreground-1` | Header and footer bars, side bars, sections, cards, menus, dialogs; hover on rows |
| default | `--salus-depth-default` | List rows, fields, wells (text, tables); Salus' dock and tab bars |
| background-1 | `--salus-depth-background-1` | The panel itself (the page); something sunk into a row |
| background-2 | `--salus-depth-background-2` | Tracks of progress bars, badges, an image's backdrop |
| background-3 | `--salus-depth-background-3` | The gap between the panels (black in the dark theme); Salus uses it itself |

The building blocks above use these levels; use the variables yourself only for what they do not cover.

## Separators

A horizontal separator is a groove: a dark 1px line over a light one. `salus-header`, `salus-footer`, `salus-item` and
`<hr>` have it already. Elsewhere, draw it as a 2px border painted from the theme's gradient:

```css
.header { border-bottom: 2px solid var(--salus-separator-dark); border-image: var(--salus-separator-below); }
.footer { border-top: 2px solid var(--salus-separator-dark); border-image: var(--salus-separator-above); }
.both   { border-top: 2px solid var(--salus-separator-dark); border-bottom: 2px solid var(--salus-separator-dark);
          border-image: var(--salus-separator-both); }
```

`border-image` does not work on table cells with collapsed borders and ignores `border-radius`, so keep it to straight
lines. The colors are translucent black and white, so the background under a separator decides its exact shade.

## Buttons

The raised outline is part of the button's background: a translucent gradient in the 1px border, laid over the fill, so it
tints the button's own colour rather than whatever is behind the button. To recolor a button and keep the
outline, set its fill rather than its background:

```css
.my-toggle.on { --salus-button-fill: var(--salus-accent); color: var(--salus-accent-fg); }
```

The fill must be opaque (a transparent fill shows the outline's gradient across the button). Setting `background`
removes the outline, which is right for flat icon buttons (`background: none; border-color: transparent`).

## Variables

| Variable | Use |
|---|---|
| `--salus-depth-*` | Backgrounds by depth; see [Depth](#depth) |
| `--salus-fg`, `--salus-fg-muted`, `--salus-fg-faint` | Text, secondary text, hints |
| `--salus-fg-hint` | Hint text over the default level, e.g. a field's placeholder (fields get it without any CSS) |
| `--salus-accent`, `--salus-accent-fg` | Primary action color and text on it |
| `--salus-danger`, `--salus-warning`, `--salus-success` | Status colors |
| `--salus-border` | Outlines of boxes, fields, menus |
| `--salus-separator-dark`, `--salus-separator-light`; `--salus-separator-below`, `--salus-separator-above`, `--salus-separator-both` | Separators; see [Separators](#separators) |
| `--salus-button-fill`; `--salus-button-edge` (from `--salus-button-edge-top`, `-side`, `-bottom`) | Buttons; see [Buttons](#buttons) |
| `--salus-gap` | The workspace between the panels (`--salus-depth-background-3`) |
| `--salus-radius`, `--salus-space-1` … `--salus-space-4` | Corner radius and spacing steps (4, 8, 12, 16 px) |
| `--salus-font`, `--salus-font-mono`, `--salus-font-size` | Typography |
| `--salus-bg`, `--salus-surface`, `--salus-surface-alt`, `--salus-surface-2`, `--salus-surface-3`, `--salus-border-button` | **Old**: the colors before the depth levels. Still defined so older plugins keep working; use the depth levels and building blocks instead. |

## Your CSS always wins

Everything Salus adds is in a low-priority **cascade layer** (`@layer salus-base`), so any normal CSS of your plugin overrides it. You can restyle freely;
you just won't follow the theme for the colors you hard-code.

## Component libraries

The variables work with any framework. Prefer libraries that are styled with CSS (headless components such as Radix or React Aria, or plain
CSS) and map their colors to the `--salus-*` variables. Libraries with their own JavaScript theme objects are harder to connect.

The stylesheet is at `/salus/theme.css` if you need to read it. You can ignore it entirely and style your plugin yourself; it is applied automatically but never forced.

!!! note "Not final"
    The stylesheet is added once the page has loaded, so a plugin may show the default colors very briefly.
