# Frontend components

A plugin has **one backend** and one or more **frontend components**. Each component is its own web page in its own iframe, so a plugin
can put a browser in the left panel, a list on the right and an editor in the center, all talking to the same backend.

```toml
[meta]
api-version = 3
entry-component = "browser"          # what the "+" list opens

[[frontend-component]]
component-name = "browser"
entrypoint = "frontend/browser/index.html"
target-panel = "left"

[[frontend-component]]
component-name = "editor"
entrypoint = "frontend/editor/index.html"
target-panel = "center"
```

Every running component has its **own frontend id**, like any frontend. In your page, `salus.component` is its name and
`salus.frontendId` its id. In the [Python backend](backend.md), `app.frontend_component(frontend_id)` tells you which component
a frontend is (it is known when the frontend attaches).

## Opening a component

```js
const editor = await salus.openComponent("editor", { params: { dataset: "d1" }, reuse: true });
```

- The component opens in **its own `target-panel`** and the call resolves with a [`Peer`](#messaging-between-components) once the page has loaded.
- You need no `[dependencies]` entry for components of your own plugin.
- **`params`** is any JSON value. The new component reads it as `salus.params`. It is only used when the component starts.
- **`reuse: true`** returns the instance that is already running (same plugin, same user) and **focuses its tab** instead of opening a second
  one. Use it for components that exist once, such as an editor that switches between documents. Tell a reused component what to show with a message,
  because `params` only reach a component when it starts.
- Components of other plugins are opened with [`openDependency`](dependencies.md), which takes the same options plus a `component` name.

## Messaging between components

`openComponent` returns a `Peer`. Peers send messages to named channels and receive them the same way as [channels to the backend](frontend.md#channels):

=== "Opener"

    ```js
    const editor = await salus.openComponent("editor", { reuse: true });
    editor.send("show", Payload.json({ dataset: "d1", entry: 7 }));
    ```

=== "Opened component"

    ```js
    const browser = salus.parent;            // the component that opened this one (null if opened by the user)
    browser?.channel("show", (message) => render(message.payload.asJson()));
    ```

Components of one plugin can reach **any other open component of the same plugin**, not only the one that opened them. Find them with:

```js
const editors = await salus.components("editor");   // Peers of the open "editor" components
const all = await salus.components();                // every other open component of this plugin
```

Messages between components of one plugin are **delivered inside the browser** by the Salus page; they never go through the server or the
backend. Messages are not buffered: register channels when your page starts, because messages for a channel nobody listens to are dropped.
Components that need to share *state* (a list that must refresh after an edit) can also go through the backend: it can broadcast to every
attached component with `await app.send(data, channel="events", to=salus.ALL)`.

## The "+" list and entry components

Only a plugin's **entry component** (`entry-component` in `[meta]`) is offered when the user presses **+** on a panel. Other components are opened by your
code. A plugin without an entry component does not appear in the list at all.

## Tabs stay alive, closing ends a component

All tabs of a panel stay loaded; only the selected one is visible. Switching tabs never reloads a component, so it keeps its state and can still receive
messages while hidden.

Closing a tab (or closing or reloading the browser page) **ends that component's frontend process**: the backend gets a `detached` event, and a backend
with `lifetime = "panel"` ends together with the plugin's last open component. A component reopened later is a fresh instance.
