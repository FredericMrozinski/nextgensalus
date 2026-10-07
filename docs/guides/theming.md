# Theming

Salus has a light and a dark theme that the user can switch. Plugins follow it **without any code**: Salus adds a stylesheet to every plugin page
and sets `data-theme="dark"` or `"light"` on the page's `<html>` element, and changes it when the user switches.

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
    The stylesheet is added once the page has loaded, so a plugin may show the default colors very briefly. And the theme switch currently lives in the bottom
    right corner of the workspace.
