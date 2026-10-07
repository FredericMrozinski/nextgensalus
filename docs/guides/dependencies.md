# Plugin dependencies

A plugin can open **another plugin** next to itself and exchange messages with it. This is how the image viewer opens
a separate controls plugin that zooms and moves its image.

## 1. Declare the dependency

In the manifest of the plugin that opens the other one, list the other plugin's identifier (its folder name):

```toml
[dependencies]
plugins = ["org.salus.imagecontrols"]
```

A plugin can only open plugins it lists here. Salus refuses everything else.

## 2. Open it from the frontend

```js
const controls = await salus.openDependency("org.salus.imagecontrols");
```

Salus starts the other plugin (backend and frontend) and shows it in the `target-panel` of the component it opens: the plugin's
`entry-component`, or its only component if it has just one. The call resolves with a `Peer` once the other plugin has finished loading,
so the first message you send will arrive.

```js
await salus.openDependency("org.example.other", {
  component: "viewer",          // a specific component instead of the default
  params: { id: 7 },            // available to it as salus.params
  reuse: true,                  // use the running instance (and focus its tab) instead of opening another
});
```

To open another component of **your own** plugin use [`openComponent`](components.md); to show a file use [`openFile`](file-viewers.md).

## 3. Talk to it

A `Peer` has channels, like `salus` itself. A channel name only has to match on both sides, and direction matters:
what one side `send`s to a name, the other side receives on that name.

=== "Opener (image viewer)"

    ```js
    // receive commands sent to "view"
    controls.channel("view", (message) => {
      const command = message.payload.asJson();
      ...
    });

    // send something to the other plugin's "state" channel
    controls.send("state", Payload.json({ zoom: 2 }));
    ```

=== "Opened plugin (controls)"

    ```js
    const viewer = salus.parent;           // null if the user opened this plugin directly

    viewer.send("view", Payload.json({ op: "zoom", factor: 1.25 }));
    viewer.channel("state", (message) => { ... });
    ```

In the opened plugin, `salus.parent` is the `Peer` of the frontend that opened it, or `null` when the user opened
the plugin on its own. Handle both so the plugin is usable standalone, as the controls example does by disabling its buttons.

`message.sender` is the peer's frontend id.

## Rules

- Messages between plugins go only between a frontend and the one that opened it (or the ones it opened). Siblings and unrelated
  plugins cannot reach each other. (Components of the *same* plugin can reach each other freely, see [Frontend components](components.md).)
- Messages for a channel the receiver has not registered yet are dropped. Register channels at startup.
- Messages travel through the server, which checks the relationship; they are not buffered.
- The opened plugin has its own backend, started as for any plugin. The opener's backend does not take part in the peer messages.

## Under the hood

`openDependency` is a `salus://dependency/open` request, answered by the Salus page after the server checked the
manifest and the session. Peer messages use channels named `peer://<frontend id>/<name>`. Details in the
[wire protocol](../reference/protocol.md).
