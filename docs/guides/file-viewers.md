# File viewers

Plugins can display files for each other without knowing about each other. A plugin that wants to show a file just asks Salus to open it;
Salus finds a component that can display that file type.

## Opening a file

```js
await salus.openFile("/data/slides/case-17.svs");
```

That is all. Salus

1. looks at the file's extension (case-insensitive),
2. finds the components that declare they can display it,
3. opens it directly if there is one, **asks the user with a picker if there are several**, or rejects with a `SalusError` if there is none,
4. opens the chosen component in its `target-panel` and gives it the file.

You never see which viewers exist. The call rejects with `SalusError` if no viewer is installed or the user cancels the picker.

!!! note "Paths"
    The path is passed on exactly as you give it, so use one the viewer's backend can open. Today that is a path on the server. Treat paths as opaque
    and keep the code that builds or interprets them in one place: when Salus restricts which files a plugin may see, how a file is handed over will change.

## Becoming a viewer

Declare the file types in the manifest, on the component that displays them:

```toml
[[frontend-component]]
component-name = "viewer"
entrypoint = "frontend/index.html"
target-panel = "center"
file_viewer_for = ["svs", "tif", "tiff"]
```

Extensions are lowercase and have no dot. A viewer-only plugin usually has no `entry-component`, so it is not in the **+** list; it opens only when a file is opened.

When Salus opens your component for a file, the file is in the page's parameters:

```js
const salus = Salus.connect();
const file = salus.params.file;          // "/data/slides/case-17.svs"
```

Forward it to your backend, for example `salus.http.json("GET", "/open", { query: { file } })`, and show the result. Each opened file gets its own
instance of the viewer.

## Several viewers for the same type

If two plugins (or two components of one plugin) declare the same extension, Salus shows a chooser dialog whenever a file of that type is opened, listing each
viewer by its `title`. Plugins that open files are not involved in the choice.
