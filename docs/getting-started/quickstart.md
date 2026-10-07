# Quickstart

In this quickstart you build **Echo**, a plugin whose frontend sends a line of text to its backend, gets
it echoed back over a channel, and also calls a small HTTP route.

You need:

- a running Salus server (`dx serve` in `packages/web` of the Salus repository),
- Python 3.10 or newer for the backend,
- the two SDK files from the Salus repository: `sdk/python/salus_sdk.py` and `sdk/js/salus_sdk.js`.

## 1. Create the plugin folder

A plugin is a folder. Its **name is the plugin's identifier**, so use a reverse-domain style name.

```text
org.example.echo/
├── manifest.toml
├── backend/
│   ├── plugin.py          (executable)
│   └── salus_sdk.py       (copy of sdk/python/salus_sdk.py)
└── frontend/
    ├── index.html
    ├── script.js
    └── salus_sdk.js       (copy of sdk/js/salus_sdk.js)
```

### `manifest.toml`

```toml
[description]
name = "Echo"
description = "Echoes what you type."
developer = "Example Org"
contact = "dev@example.org"
version = "0.1.0"

[meta]
api-version = 3
entry-component = "main"              # the component the "+" list opens

[backend]
entrypoint = "backend/plugin.py"
lifetime = "session"

[[frontend-component]]
component-name = "main"
entrypoint = "frontend/index.html"
target-panel = "center"
```

### `backend/plugin.py`

```python
#!/usr/bin/env python3
import salus_sdk as salus

app = salus.Plugin()


@app.channel("echo")                      # messages the frontend sends on "echo"
async def echo(msg):
    await app.send(salus.Payload.str("Echo: " + msg.payload.as_str()))   # reply to the sender


@app.route("GET", "/hello/{name}")        # an HTTP route the frontend can call
async def hello(req):
    return {"greeting": f"Hello, {req.params['name']}!"}


if __name__ == "__main__":
    app.run()
```

Make it executable, because Salus starts the entrypoint directly:

```sh
chmod +x org.example.echo/backend/plugin.py
```

### `frontend/index.html`

```html
<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>Echo</title></head>
<body>
  <form id="form"><input id="text" autocomplete="off"><button>Send</button></form>
  <pre id="log"></pre>
  <script type="module" src="script.js"></script>
</body>
</html>
```

### `frontend/script.js`

```js
import { Salus } from "./salus_sdk.js";

const salus = Salus.connect();             // must run inside Salus
const log = (line) => (document.getElementById("log").textContent += line + "\n");

// Channel: bidirectional messages with the backend.
const echo = salus.channel("echo");
echo.onMessage((message) => log(message.payload.asStr()));

document.getElementById("form").addEventListener("submit", (event) => {
  event.preventDefault();
  const input = document.getElementById("text");
  echo.send(input.value);
  input.value = "";
});

// HTTP: a plain request to a route the backend opened.
const reply = await salus.http.json("GET", "/hello/Salus");
log(reply.greeting);
```

## 2. Install and run

Copy the folder into the Salus plugins directory and **restart the server** (manifests are read at startup):

| OS | Plugins directory |
|---|---|
| macOS | `~/Library/Application Support/org.fredericmrozinski.salus/plugins/` |
| Linux | `~/.local/share/salus/plugins/` |

Open Salus in the browser, press **+** on a panel, and pick **Echo** (the list shows plugins that name an `entry-component`). You should see
`Hello, Salus!` appear immediately, and every line you send comes back as `Echo: ...`.

## What just happened

- Salus started `plugin.py` and passed it the path of a Unix socket as the first argument; `salus.Plugin()` connected to it.
- The page served `index.html` in an iframe and added `?fe_process_id=...` to its URL; `Salus.connect()` read that id.
- `echo.send(...)` became a `ws://echo` message, travelled to the framework and into the backend's `@app.channel("echo")` handler.
- `app.send(...)` inside the handler replied to *that* frontend.
- `salus.http.json("GET", "/hello/Salus")` was a normal HTTP request to the framework, which forwarded it to the
  route the backend registered with `@app.route`.

## Next

- [Plugin anatomy](plugin-anatomy.md) explains every file and manifest key.
- [Frontend components](../guides/components.md) shows how to split a plugin's interface into several pages that open and message each other.
- The [Python backend](../guides/backend.md) and [JavaScript frontend](../guides/frontend.md) guides cover everything the SDKs offer.

!!! tip "If something does not show up"
    Check that the plugin folder contains a valid `manifest.toml` (the server log lists every plugin it
    loaded and prints manifest errors), that `plugin.py` is executable, and that `python3` on the server's
    `PATH` is 3.10 or newer. The backend writes its own log to `salus_sdk_log_*.log` in the system temp folder.
