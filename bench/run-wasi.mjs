// Runs a WASI build (wasm32-wasip1) of diagcalc-bench or spike-replay under Node, with the
// current directory and /tmp visible to it:
//   node bench/run-wasi.mjs target/wasm32-wasip1/release/diagcalc-bench.wasm --quick
import { readFile } from "node:fs/promises";
import { WASI } from "node:wasi";
import { argv, cwd, exit } from "node:process";

const [wasmPath, ...args] = argv.slice(2);
const wasi = new WASI({
  version: "preview1",
  args: [wasmPath, ...args],
  env: {},
  preopens: { ".": cwd(), "/tmp": "/tmp" },
  returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(wasmPath));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
exit(wasi.start(instance));
