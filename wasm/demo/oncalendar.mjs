const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

export async function load(bytes) {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const wasm = instance.exports;
  const capacity = wasm.input_capacity() >>> 0;

  const read = (length) =>
    decoder.decode(
      new Uint8Array(wasm.memory.buffer, wasm.output_buffer() >>> 0, length >>> 0),
    );

  return {
    zones() {
      return read(wasm.zones()).split("\n");
    },
    evaluate(localZone, base, expression, count) {
      const request = encoder.encode(`${localZone}\t${base}\t${expression}`);
      if (request.length > capacity) {
        return { status: "bad-request", message: `The request is longer than ${capacity} bytes.` };
      }
      new Uint8Array(wasm.memory.buffer, wasm.input_buffer() >>> 0, capacity).set(request);
      return parseResponse(read(wasm.evaluate(request.length, count)));
    },
  };
}

function parseResponse(text) {
  const [head, ...lines] = text.split("\n");
  const [status, ...fields] = head.split("\t");
  switch (status) {
    case "ok":
      return {
        status,
        normalized: fields[0],
        elapses: lines.map((line) => {
          const [utc, local] = line.split("\t");
          return { utc, local };
        }),
      };
    case "invalid":
      return { status, offset: Number(fields[0]), message: fields[1] };
    case "unknown-zone":
      return { status, normalized: fields[0], message: fields[1] };
    default:
      return { status: "bad-request", message: fields.join("\t") };
  }
}
