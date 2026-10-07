# Payloads

Messages and HTTP bodies are bytes. A `Payload` is an immutable wrapper with typed accessors, available in both
SDKs with the same names (Python `as_str()`, JavaScript `asStr()`), so a record written on one side can be read
on the other.

## Text, JSON and bytes

=== "Python"

    ```python
    p = salus.Payload.str("hello")
    p.as_str()                                   # "hello"

    j = salus.Payload.json({"a": [1, 2]})
    j.as_json()                                  # {"a": [1, 2]}

    raw = salus.Payload(b"\x00\x01")
    raw.as_bytes()
    ```

=== "JavaScript"

    ```js
    const p = Payload.str("hello");
    p.asStr();                                   // "hello"

    const j = Payload.json({ a: [1, 2] });
    j.asJson();                                  // { a: [1, 2] }

    const raw = new Payload([0, 1]);
    raw.asBytes();                               // Uint8Array
    ```

`as_str` and `as_json` raise `PayloadError` if the bytes are not valid UTF-8 or JSON. JSON is strict: `NaN` and
`Infinity` are rejected.

## Numbers

Whole payloads that are a single number use fixed-width little-endian integers and floats:
`u8 i8 u16 i16 u32 i32 u64 i64 f32 f64 bool`.

=== "Python"

    ```python
    salus.Payload.u32(7).as_u32()
    salus.Payload.f64(2.5).as_f64()
    ```

=== "JavaScript"

    ```js
    Payload.u32(7).asU32();
    Payload.f64(2.5).asF64();
    Payload.u64(2n ** 40n).asU64();     // 64 bit integers are BigInt
    ```

Out-of-range values raise `PayloadError`.

## Records

For messages with several fields, build and read a **record**: fields back to back, where strings, byte blobs and JSON
inside a record carry a `u32` length prefix.

=== "Python"

    ```python
    payload = (salus.Payload.builder()
               .u8(1).str("name").f64(2.5).bytes(b"...").json({"k": True})
               .build())

    r = payload.reader()
    r.u8(); r.str(); r.f64(); r.bytes(); r.json()
    r.expect_end()                 # raises if bytes are left over
    ```

=== "JavaScript"

    ```js
    const payload = Payload.builder()
      .u8(1).str("name").f64(2.5).bytes(new Uint8Array([1, 2])).json({ k: true })
      .build();

    const r = payload.reader();
    r.u8(); r.str(); r.f64(); r.bytes(); r.json();
    r.expectEnd();                 // throws if bytes are left over
    ```

Fields must be read in the order they were written. `reader.rest()` returns whatever is left, which is handy for a
header followed by a large body.

## Which format should I use?

- **JSON** for almost everything: easy to debug and evolve.
- **Records** when you move many numbers or binary blobs, for example image regions or sample arrays, and want to
  avoid text encoding.
- **Plain bytes** when the data already has a format (JPEG, PNG, DICOM, ...). For bulk data prefer an
  [HTTP route](http.md) over a channel.
