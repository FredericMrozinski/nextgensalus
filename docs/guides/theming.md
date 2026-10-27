# Theming

Salus comes with several themes, and the user picks one in the settings (**Settings → Salus → Theme**). Plugins follow it **without any code**: Salus adds
a stylesheet to every plugin page and sets the `data-theme` attribute of the page's `<html>` element to the theme's id, and changes it when the user picks another theme.
The change applies at once, in every open page.

| Theme id | Look |
|---|---|
| `dark` | Neutral dark with a blue accent. The default. |
| `light` | Neutral light with a purple accent. |
| `light-red` | Light with a red accent. |
| `dark-green` | Dark with a green accent. |
| `dark-oled` | Pure black (for OLED screens) with purple-tinted surfaces and borders and an intense bright green accent. |
| `midnight` | Deep blue with a cyan accent. |

Every theme defines the same variables below, so a plugin that only uses the variables looks right in all of them. If you need to treat light and dark
themes differently, use `color-scheme` (`getComputedStyle(document.documentElement).colorScheme` is `light` or `dark`) rather than the theme ids, because themes can be added.

## Use the variables

The stylesheet defines CSS variables. Use them in your own CSS instead of fixed colors:

```css
.card {
  background: var(--salus-surface);
  color: var(--salus-fg);
  border: 1px solid var(--salus-border);
  border-radius: var(--salus-radius);
  padding: var(--salus-space-3);
}
```

| Variable | Use |
|---|---|
| `--salus-bg` | Page background |
| `--salus-surface`, `--salus-surface-alt`, `--salus-surface-2`, `--salus-surface-3` | Cards, panels, hover and pressed states (increasing emphasis) |
| `--salus-gap` | The workspace background between the panels (a bit darker than `--salus-bg`). Salus uses it itself; plugins rarely need it. |
| `--salus-border` | Borders and separators |
| `--salus-fg`, `--salus-fg-muted`, `--salus-fg-faint` | Text, secondary text, hints |
| `--salus-accent`, `--salus-accent-fg` | Primary action color and text on it |
| `--salus-danger`, `--salus-warning`, `--salus-success` | Status colors |
| `--salus-radius`, `--salus-space-1` … `--salus-space-4` | Corner radius and spacing steps (4, 8, 12, 16 px) |
| `--salus-font`, `--salus-font-mono`, `--salus-font-size` | Typography |

The stylesheet also gives plain HTML sensible defaults (page background and text color, buttons, inputs, tables), so a page with no CSS already looks right.

## Your CSS always wins

Everything Salus adds is in a low-priority **cascade layer** (`@layer salus-base`), so any normal CSS of your plugin overrides it. You can restyle freely;
you just won't follow the theme for the colors you hard-code.

## Component libraries

The variables work with any framework. Prefer libraries that are styled with CSS (headless components such as Radix or React Aria, or plain
CSS) and map their colors to the `--salus-*` variables. Libraries with their own JavaScript theme objects are harder to connect.

The stylesheet is at `/salus/theme.css` if you need to read it. You can ignore it entirely and style your plugin yourself; it is applied automatically but never forced.

!!! note "Not final"
    The stylesheet is added once the page has loaded, so a plugin may show the default colors very briefly.
