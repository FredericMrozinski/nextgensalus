// Run with: node --test sdk/js
import assert from "node:assert/strict";
import { test } from "node:test";
import { ChannelMessage, HttpError, Payload, PayloadError, Peer, Salus, SalusError, SalusTimeout, packMeta, unpackMeta } from "./salus_sdk.js";

const ORIGIN = "http://salus.test";
const FE = 30001;

function fakeEnvironment(search = `?fe_process_id=${FE}`) {
  const posted = [];
  const listeners = new Set();
  const parent = { postMessage: (data, origin) => posted.push({ data, origin }) };
  const window = {
    parent,
    location: { origin: ORIGIN, search },
    addEventListener: (type, fn) => type === "message" && listeners.add(fn),
    removeEventListener: (type, fn) => type === "message" && listeners.delete(fn),
  };
  // What the Salus frontend does for backend -> plugin messages.
  const deliver = (frame, overrides = {}) =>
    listeners.forEach((fn) => fn({ source: parent, origin: ORIGIN, data: frame, ...overrides }));
  return { window, posted, deliver, listenerCount: () => listeners.size };
}

const frame = (channel, payload, extra = {}) => ({
  frontend_process_id: FE,
  message_id: 1,
  flags: 0,
  logical_channel: channel,
  payload: Array.from(payload),
  ...extra,
});

const bytes = (text) => new TextEncoder().encode(text);

// --- Payload ---------------------------------------------------------------

test("payload scalars roundtrip and are little endian", () => {
  assert.deepEqual([...Payload.u32(0x01020304).asBytes()], [4, 3, 2, 1]);
  assert.equal(Payload.u32(0xFFFFFFFF).asU32(), 0xFFFFFFFF);
  assert.equal(Payload.i16(-2).asI16(), -2);
  assert.equal(Payload.u64(2n ** 63n).asU64(), 2n ** 63n);
  assert.equal(Payload.i64(-5).asI64(), -5n);
  assert.equal(Payload.f64(1.5).asF64(), 1.5);
  assert.equal(Payload.bool(true).asBool(), true);
});

test("payload scalars reject out-of-range values and wrong sizes", () => {
  assert.throws(() => Payload.u8(256), PayloadError);
  assert.throws(() => Payload.u8(1.5), PayloadError);
  assert.throws(() => Payload.u64(-1), PayloadError);
  assert.throws(() => Payload.u32(1).asU8(), PayloadError);
  assert.throws(() => new Payload([2]).asBool(), PayloadError);
});

test("payload str and json", () => {
  assert.equal(Payload.str("héllo").asStr(), "héllo");
  assert.deepEqual(Payload.json({ a: [1, 2] }).asJson(), { a: [1, 2] });
  assert.throws(() => new Payload([0xFF, 0xFE]).asStr(), PayloadError);
  assert.throws(() => bytes("{") && new Payload(bytes("{")).asJson(), PayloadError);
  assert.throws(() => Payload.json(NaN), PayloadError);
  assert.throws(() => Payload.json(undefined), PayloadError);
});

test("compound records via builder and reader", () => {
  const payload = Payload.builder().u8(7).str("name").bytes([1, 2, 3]).json({ k: true }).f32(0.5).build();
  const reader = payload.reader();
  assert.equal(reader.u8(), 7);
  assert.equal(reader.str(), "name");
  assert.deepEqual([...reader.bytes()], [1, 2, 3]);
  assert.deepEqual(reader.json(), { k: true });
  assert.equal(reader.f32(), 0.5);
  reader.expectEnd();
  assert.throws(() => reader.u8(), PayloadError);
});

test("reader reports trailing bytes", () => {
  const reader = new Payload([1, 2]).reader();
  reader.u8();
  assert.throws(() => reader.expectEnd(), PayloadError);
  assert.deepEqual([...reader.rest()], [2]);
});

// --- Connection ------------------------------------------------------------

test("connect needs an iframe and a frontend id", () => {
  const env = fakeEnvironment("");
  assert.throws(() => Salus.connect({ window: env.window }), SalusError);
  assert.equal(Salus.connect({ window: env.window, frontendId: 5 }).frontendId, 5);

  const top = fakeEnvironment();
  top.window.parent = top.window;
  assert.throws(() => Salus.connect({ window: top.window }), SalusError);
});

test("frontend id is read from the URL", () => {
  const env = fakeEnvironment("?x=1&fe_process_id=30042");
  assert.equal(Salus.connect({ window: env.window }).frontendId, 30042);
});

test("channel.send posts a complete frame to the parent", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  salus.channel("chat").send("hi");

  assert.equal(env.posted.length, 1);
  assert.equal(env.posted[0].origin, ORIGIN);
  assert.deepEqual(env.posted[0].data, {
    frontend_process_id: FE,
    message_id: 0,
    flags: 0,
    logical_channel: "ws://chat",
    payload: [0x68, 0x69],
  });
});

test("send accepts strings, bytes, buffers and payloads; ws:// prefix is optional", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  salus.send("a", "x");
  salus.send("ws://a", new Uint8Array([1]));
  salus.send("a", new Uint8Array([2]).buffer);
  salus.send("a", Payload.u8(3));
  salus.send("a", new Uint16Array([0x0504]));
  assert.deepEqual(env.posted.map((p) => p.data.payload), [[0x78], [1], [2], [3], [4, 5]]);
  assert.ok(env.posted.every((p) => p.data.logical_channel === "ws://a"));
  assert.throws(() => salus.send("a", { not: "sendable" }), TypeError);
  assert.throws(() => salus.send("a://b", "x"), TypeError);
});

test("message ids increase per message", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  salus.send("a", "1");
  salus.send("a", "2");
  assert.deepEqual(env.posted.map((p) => p.data.message_id), [0, 1]);
});

test("large messages are fragmented under one message id and reassemble on the other side", () => {
  const sender = fakeEnvironment();
  const salus = Salus.connect({ window: sender.window, maxFragmentPayload: 4 });
  salus.send("big", "0123456789");

  const frames = sender.posted.map((p) => p.data);
  assert.deepEqual(frames.map((f) => f.flags), [1, 1, 0]);
  assert.equal(new Set(frames.map((f) => f.message_id)).size, 1);
  assert.deepEqual(frames.map((f) => String.fromCharCode(...f.payload)), ["0123", "4567", "89"]);

  const receiver = fakeEnvironment();
  const peer = Salus.connect({ window: receiver.window });
  const received = [];
  peer.channel("big", (m) => received.push(m.payload.asStr()));
  frames.forEach((f) => receiver.deliver(f));
  assert.deepEqual(received, ["0123456789"]);
});

test("an empty message is one frame", () => {
  const env = fakeEnvironment();
  Salus.connect({ window: env.window }).send("a", "");
  assert.equal(env.posted.length, 1);
  assert.deepEqual(env.posted[0].data.payload, []);
});

// --- Receiving -------------------------------------------------------------

test("handler receives messages for its channel", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const received = [];
  salus.channel("chat", (m) => received.push(m));
  salus.channel("other", () => assert.fail("wrong channel"));

  env.deliver(frame("ws://chat", bytes("hello"), { message_id: 9 }));

  assert.equal(received.length, 1);
  assert.ok(received[0] instanceof ChannelMessage);
  assert.equal(received[0].channel, "chat");
  assert.equal(received[0].messageId, 9);
  assert.equal(received[0].payload.asStr(), "hello");
});

test("payload may arrive as a Uint8Array", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const received = [];
  salus.channel("c", (m) => received.push(m.payload.asStr()));
  env.deliver({ ...frame("ws://c", []), payload: bytes("typed") });
  assert.deepEqual(received, ["typed"]);
});

test("async iteration", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const channel = salus.channel("stream");
  const seen = [];
  const consumer = (async () => {
    for await (const m of channel) seen.push(m.payload.asStr());
  })();

  env.deliver(frame("ws://stream", bytes("a")));
  env.deliver(frame("ws://stream", bytes("b"), { message_id: 2 }));
  await new Promise((r) => setImmediate(r));
  channel.close();
  await consumer;
  assert.deepEqual(seen, ["a", "b"]);
});

test("messages are only accepted from the parent window and its origin", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const received = [];
  salus.channel("c", (m) => received.push(m));

  env.deliver(frame("ws://c", bytes("x")), { source: {} });
  env.deliver(frame("ws://c", bytes("x")), { origin: "http://evil.test" });
  assert.equal(received.length, 0);
  env.deliver(frame("ws://c", bytes("x")));
  assert.equal(received.length, 1);
});

test("junk and frames for other frontends are ignored", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const received = [];
  salus.channel("c", (m) => received.push(m));
  const quiet = console.warn;
  console.warn = () => {};
  try {
    env.deliver("string");
    env.deliver(null);
    env.deliver({ type: "something-else" });
    env.deliver(frame("ws://c", bytes("x"), { frontend_process_id: FE + 1 }));
    env.deliver(frame("ws://c", bytes("x"), { flags: 0x80 }));
  } finally {
    console.warn = quiet;
  }
  assert.equal(received.length, 0);
});

test("fragments of interleaved messages reassemble independently", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const received = [];
  salus.channel("c", (m) => received.push(m.payload.asStr()));

  env.deliver(frame("ws://c", bytes("A1"), { message_id: 1, flags: 1 }));
  env.deliver(frame("ws://c", bytes("B1"), { message_id: 2, flags: 1 }));
  env.deliver(frame("ws://c", bytes("small"), { message_id: 3 }));
  env.deliver(frame("ws://c", bytes("B2"), { message_id: 2 }));
  env.deliver(frame("ws://c", bytes("A2"), { message_id: 1 }));
  assert.deepEqual(received, ["small", "B1B2", "A1A2"]);
});

test("a fragment switching channel drops the whole message", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const received = [];
  salus.channel("a", (m) => received.push(m));
  salus.channel("b", (m) => received.push(m));
  const quiet = console.warn;
  console.warn = () => {};
  try {
    env.deliver(frame("ws://a", bytes("1"), { flags: 1 }));
    env.deliver(frame("ws://b", bytes("2"), { flags: 1 }));
    env.deliver(frame("ws://a", bytes("3"), { flags: 0 }));
  } finally {
    console.warn = quiet;
  }
  assert.equal(received.length, 0);
});

test("oversized reassembly is dropped", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window, maxMessageSize: 3 });
  const received = [];
  salus.channel("c", (m) => received.push(m));
  const quiet = console.warn;
  console.warn = () => {};
  try {
    env.deliver(frame("ws://c", bytes("12"), { flags: 1 }));
    env.deliver(frame("ws://c", bytes("34"), { flags: 0 }));
  } finally {
    console.warn = quiet;
  }
  assert.equal(received.length, 0);
});

test("unclaimed messages go to onUnclaimed", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const unclaimed = [];
  const off = salus.onUnclaimed((m) => unclaimed.push(m));
  env.deliver(frame("ws://nobody", bytes("x")));
  env.deliver(frame("custom://thing", bytes("y")));
  assert.deepEqual(unclaimed.map((m) => [m.channel, m.payload.asStr()]), [["ws://nobody", "x"], ["custom://thing", "y"]]);
  off();
});

test("a throwing or rejecting handler does not break delivery", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const received = [];
  const channel = salus.channel("c", () => {
    throw new Error("boom");
  });
  channel.onMessage(async () => {
    throw new Error("async boom");
  });
  channel.onMessage((m) => received.push(m));
  const quiet = console.error;
  console.error = () => {};
  try {
    env.deliver(frame("ws://c", bytes("x")));
    await new Promise((r) => setImmediate(r));
  } finally {
    console.error = quiet;
  }
  assert.equal(received.length, 1);
});

test("duplicate channels are rejected; closing frees the name", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const channel = salus.channel("c");
  assert.throws(() => salus.channel("ws://c"), SalusError);
  channel.close();
  assert.throws(() => channel.send("x"), SalusError);
  salus.channel("c");
});

test("close stops listening", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  assert.equal(env.listenerCount(), 1);
  salus.close();
  assert.equal(env.listenerCount(), 0);
  assert.throws(() => salus.send("a", "x"), SalusError);
});

// --- HTTP ------------------------------------------------------------------

function httpEnvironment(respond = () => new Response("ok")) {
  const calls = [];
  const env = fakeEnvironment();
  const salus = Salus.connect({
    window: env.window,
    fetch: async (url, init) => {
      calls.push({ url, init });
      return respond(url, init);
    },
  });
  return { salus, calls };
}

test("http urls are namespaced by frontend id", () => {
  const { salus } = httpEnvironment();
  assert.equal(salus.http.url("/doc/42"), `/plugin-api/${FE}/doc/42`);
  assert.equal(salus.http.url("/"), `/plugin-api/${FE}`);
  assert.equal(salus.http.url("/img", { w: 10, tag: ["a", "b"], skip: undefined, none: null }), `/plugin-api/${FE}/img?w=10&tag=a&tag=b`);
  assert.equal(salus.http.url("/x?a=1", { b: "2 3" }), `/plugin-api/${FE}/x?a=1&b=2+3`);
  assert.throws(() => salus.http.url("doc"), TypeError);
});

test("http request passes method, headers and body through", async () => {
  const { salus, calls } = httpEnvironment();
  await salus.http.post("/doc", { body: "raw", headers: { "x-a": "1" }, query: { q: 1 } });
  await salus.http.put("/doc", { body: Payload.u8(9) });
  await salus.http.delete("/doc");

  assert.equal(calls[0].url, `/plugin-api/${FE}/doc?q=1`);
  assert.equal(calls[0].init.method, "POST");
  assert.equal(calls[0].init.body, "raw");
  assert.equal(calls[0].init.headers["x-a"], "1");
  assert.equal(calls[0].init.credentials, "same-origin");
  assert.deepEqual([...calls[1].init.body], [9]);
  assert.equal(calls[2].init.method, "DELETE");
  assert.equal(calls[2].init.body, undefined);
});

test("json option encodes the body and sets content-type unless given", async () => {
  const { salus, calls } = httpEnvironment();
  await salus.http.post("/a", { json: { n: 1 } });
  await salus.http.post("/a", { json: [1], headers: { "Content-Type": "application/vnd.x+json" } });
  assert.equal(calls[0].init.body, '{"n":1}');
  assert.equal(calls[0].init.headers["content-type"], "application/json");
  assert.equal(calls[1].init.headers["content-type"], undefined);
  assert.throws(() => salus.http.post("/a", { json: 1, body: "x" }), TypeError);
});

test("http.json parses bodies, tolerates empty ones and throws HttpError on failure", async () => {
  const { salus } = httpEnvironment((url) => {
    if (url.endsWith("/ok")) return Response.json({ a: 1 });
    if (url.endsWith("/empty")) return new Response(null, { status: 204 });
    return new Response("nope", { status: 404, statusText: "Not Found" });
  });
  assert.deepEqual(await salus.http.json("GET", "/ok"), { a: 1 });
  assert.equal(await salus.http.json("DELETE", "/empty"), undefined);
  await assert.rejects(salus.http.json("GET", "/missing"), (e) => {
    assert.ok(e instanceof HttpError);
    assert.equal(e.status, 404);
    assert.equal(e.body, "nope");
    return true;
  });
});

test("plain http calls do not throw on error statuses", async () => {
  const { salus } = httpEnvironment(() => new Response("bad", { status: 502 }));
  const response = await salus.http.get("/x");
  assert.equal(response.status, 502);
});

// --- Requests, dependencies and peers -----------------------------------------

const metaFrame = (channel, meta, body = [], extra = {}) => frame(channel, packMeta(meta, new Uint8Array(body)), extra);
const tick = () => new Promise((resolve) => setTimeout(resolve, 5));

test("meta container roundtrip", () => {
  const packed = packMeta({ id: 1, a: "x" }, new Uint8Array([9]));
  const { meta, body } = unpackMeta(packed);
  assert.deepEqual(meta, { id: 1, a: "x" });
  assert.deepEqual([...body], [9]);
  assert.deepEqual([...packMeta({})], [2, 0, 0, 0, 0x7b, 0x7d]);
  assert.throws(() => unpackMeta([1, 0]), SalusError);
  assert.throws(() => unpackMeta(packMeta([1])), SalusError);
});

test("request sends meta with an id and resolves with the reply", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const reply = salus.request("some/topic", { a: 1 });

  const sent = env.posted[0].data;
  assert.equal(sent.logical_channel, "salus://some/topic");
  const { meta } = unpackMeta(sent.payload);
  assert.deepEqual(meta, { id: 1, a: 1 });

  env.deliver(metaFrame("salus://some/topic", { id: 1, ok: true, extra: "y" }, [7]));
  const result = await reply;
  assert.equal(result.meta.extra, "y");
  assert.deepEqual([...result.body.asBytes()], [7]);
});

test("request rejects with the framework's error, on timeout and on close", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });

  const refused = salus.request("t");
  env.deliver(metaFrame("salus://t", { id: 1, ok: false, error: "nope" }));
  await assert.rejects(refused, (e) => e instanceof SalusError && e.message === "nope");

  await assert.rejects(salus.request("t", {}, { timeout: 10 }), SalusTimeout);

  const pending = salus.request("t");
  salus.close();
  await assert.rejects(pending, SalusError);
  await assert.rejects(salus.request("t"), SalusError);
  assert.throws(() => salus.request("t", { id: 5 }), TypeError);
});

test("late or unknown replies are ignored", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const quiet = console.warn;
  console.warn = () => {};
  try {
    env.deliver(metaFrame("salus://t", { id: 99, ok: true }));
  } finally {
    console.warn = quiet;
  }
  await assert.rejects(salus.request("t", {}, { timeout: 5 }), SalusTimeout);
});

test("openDependency resolves with a peer once the dependency reported ready", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const opening = salus.openDependency("org.example.controls");

  const { meta } = unpackMeta(env.posted[0].data.payload);
  assert.equal(env.posted[0].data.logical_channel, "salus://dependency/open");
  assert.equal(meta.plugin, "org.example.controls");

  env.deliver(metaFrame("salus://dependency/open", { id: meta.id, ok: true, frontend_id: 30050 }));
  await tick();
  let resolved = false;
  opening.then(() => (resolved = true));
  await tick();
  assert.equal(resolved, false); // still waiting for the dependency

  env.deliver(frame("peer://30050/salus.ready", [], { message_id: 7 }));
  const peer = await opening;
  assert.ok(peer instanceof Peer);
  assert.equal(peer.id, 30050);
  assert.equal(salus.peer(30050), peer);
});

test("openDependency copes with the ready message arriving first", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const opening = salus.openDependency("x");
  const { meta } = unpackMeta(env.posted[0].data.payload);
  env.deliver(frame("peer://30051/salus.ready", []));
  env.deliver(metaFrame("salus://dependency/open", { id: meta.id, ok: true, frontend_id: 30051 }));
  assert.equal((await opening).id, 30051);
});

test("openDependency surfaces refusals and a dependency that never loads", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });

  const refused = salus.openDependency("x");
  const first = unpackMeta(env.posted[0].data.payload).meta;
  env.deliver(metaFrame("salus://dependency/open", { id: first.id, ok: false, error: "not declared" }));
  await assert.rejects(refused, /not declared/);

  const stuck = salus.openDependency("y", { timeout: 20 });
  const second = unpackMeta(env.posted[1].data.payload).meta;
  env.deliver(metaFrame("salus://dependency/open", { id: second.id, ok: true, frontend_id: 30052 }));
  await assert.rejects(stuck, SalusTimeout);
});

test("peer.send addresses the peer and peer channels receive from it", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const peer = salus.peer(30060);

  peer.send("view", Payload.json({ op: "zoom" }));
  assert.equal(env.posted[0].data.logical_channel, "peer://30060/view");
  assert.equal(env.posted[0].data.frontend_process_id, FE);
  assert.throws(() => peer.send("", "x"), TypeError);

  const received = [];
  const channel = peer.channel("state", (m) => received.push(m));
  assert.equal(channel.wireName, "peer://30060/state");
  env.deliver(frame("peer://30060/state", bytes("hi")));
  env.deliver(frame("peer://30061/state", bytes("other peer")));
  env.deliver(frame("ws://state", bytes("backend")));
  assert.equal(received.length, 1);
  assert.equal(received[0].sender, 30060);
  assert.equal(received[0].channel, "state");
  assert.equal(received[0].payload.asStr(), "hi");

  channel.send("ack");
  assert.equal(env.posted[1].data.logical_channel, "peer://30060/state");
});

test("ws and peer channels of the same name do not clash", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const seen = [];
  salus.channel("view", () => seen.push("ws"));
  salus.peer(30070).channel("view", () => seen.push("peer"));
  env.deliver(frame("ws://view", []));
  env.deliver(frame("peer://30070/view", []));
  assert.deepEqual(seen, ["ws", "peer"]);
  assert.throws(() => salus.peer(30070).channel("view"), SalusError);
});

test("fragments from different senders with the same message id reassemble separately", () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const received = [];
  salus.peer(30080).channel("c", (m) => received.push(["a", m.payload.asStr()]));
  salus.peer(30081).channel("c", (m) => received.push(["b", m.payload.asStr()]));

  env.deliver(frame("peer://30080/c", bytes("A1"), { message_id: 1, flags: 1 }));
  env.deliver(frame("peer://30081/c", bytes("B1"), { message_id: 1, flags: 1 }));
  env.deliver(frame("peer://30081/c", bytes("B2"), { message_id: 1 }));
  env.deliver(frame("peer://30080/c", bytes("A2"), { message_id: 1 }));
  assert.deepEqual(received, [["b", "B1B2"], ["a", "A1A2"]]);
});

test("a frontend opened as a dependency exposes its parent and announces itself", async () => {
  const env = fakeEnvironment(`?fe_process_id=${FE}&parent_fe_process_id=30001`);
  const salus = Salus.connect({ window: env.window });
  assert.equal(salus.parent.id, 30001);
  assert.equal(Salus.connect({ window: fakeEnvironment().window }).parent, null);

  salus.parent.channel("late"); // registered synchronously, before the announcement goes out
  assert.equal(env.posted.length, 0);
  await tick();
  assert.equal(env.posted.length, 1);
  assert.equal(env.posted[0].data.logical_channel, "peer://30001/salus.ready");
});

// --- Components, params and files -----------------------------------------------

test("component and params come from the URL", () => {
  const params = encodeURIComponent(JSON.stringify({ file: "/data/a b.svs" }));
  const env = fakeEnvironment(`?fe_process_id=${FE}&component=editor&params=${params}`);
  const salus = Salus.connect({ window: env.window });
  assert.equal(salus.component, "editor");
  assert.deepEqual(salus.params, { file: "/data/a b.svs" });

  const plain = Salus.connect({ window: fakeEnvironment().window });
  assert.equal(plain.component, null);
  assert.equal(plain.params, null);
  const broken = Salus.connect({ window: fakeEnvironment(`?fe_process_id=${FE}&params=%7Bnope`).window });
  assert.equal(broken.params, null);
});

test("openComponent requests component/open and waits for the new component to be ready", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const opening = salus.openComponent("editor", { params: { dataset: "d1" }, reuse: true });

  const sent = env.posted[0].data;
  assert.equal(sent.logical_channel, "salus://component/open");
  const { meta } = unpackMeta(sent.payload);
  assert.deepEqual({ component: meta.component, params: meta.params, reuse: meta.reuse }, { component: "editor", params: { dataset: "d1" }, reuse: true });

  env.deliver(metaFrame("salus://component/open", { id: meta.id, ok: true, frontend_id: 30100, component: "editor", reused: false }));
  await tick();
  let done = false;
  opening.then(() => (done = true));
  await tick();
  assert.equal(done, false); // waits for the component's ready announcement

  env.deliver(frame("peer://30100/salus.ready", []));
  const peer = await opening;
  assert.equal(peer.id, 30100);
  assert.equal(peer.component, "editor");
});

test("a reused component resolves immediately", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const opening = salus.openComponent("editor", { reuse: true });
  const { meta } = unpackMeta(env.posted[0].data.payload);
  env.deliver(metaFrame("salus://component/open", { id: meta.id, ok: true, frontend_id: 30101, component: "editor", reused: true }));
  assert.equal((await opening).id, 30101);
});

test("openDependency can name a component and pass params", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const opening = salus.openDependency("org.example.other", { component: "viewer", params: { x: 1 } });
  const { meta } = unpackMeta(env.posted[0].data.payload);
  assert.equal(env.posted[0].data.logical_channel, "salus://dependency/open");
  assert.deepEqual({ plugin: meta.plugin, component: meta.component, params: meta.params }, { plugin: "org.example.other", component: "viewer", params: { x: 1 } });
  env.deliver(metaFrame("salus://dependency/open", { id: meta.id, ok: true, frontend_id: 30102, component: "viewer", reused: true }));
  assert.equal((await opening).component, "viewer");
});

test("components() lists the open components of the plugin as peers", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });
  const listing = salus.components("editor");
  const { meta } = unpackMeta(env.posted[0].data.payload);
  assert.equal(env.posted[0].data.logical_channel, "salus://component/list");
  assert.equal(meta.component, "editor");
  env.deliver(metaFrame("salus://component/list", {
    id: meta.id, ok: true, components: [{ frontend_id: 30200, component: "editor" }, { frontend_id: 30201, component: "editor" }],
  }));
  const peers = await listing;
  assert.deepEqual(peers.map((p) => [p.id, p.component]), [[30200, "editor"], [30201, "editor"]]);
  peers[0].send("show", "x");
  assert.equal(env.posted.at(-1).data.logical_channel, "peer://30200/show");
});

test("openFile asks Salus to open the file and surfaces refusals", async () => {
  const env = fakeEnvironment();
  const salus = Salus.connect({ window: env.window });

  const opening = salus.openFile("/data/slide.svs");
  const { meta } = unpackMeta(env.posted[0].data.payload);
  assert.equal(env.posted[0].data.logical_channel, "salus://file/open");
  assert.equal(meta.file, "/data/slide.svs");
  env.deliver(metaFrame("salus://file/open", { id: meta.id, ok: true, frontend_id: 30300 }));
  assert.equal(await opening, undefined);

  const refused = salus.openFile("/data/x.unknown");
  const second = unpackMeta(env.posted[1].data.payload).meta;
  env.deliver(metaFrame("salus://file/open", { id: second.id, ok: false, error: "no viewer is installed for '.unknown' files" }));
  await assert.rejects(refused, /no viewer/);

  await assert.rejects(salus.openFile(""), TypeError);
});
