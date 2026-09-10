// Instantiates the kadu-wasm-check module and calls run_bench(matches, seed),
// printing the aggregate hash so it can be diffed against the native
// `kadu bench` output for the same matches/seed.
const fs = require("fs");
const path = require("path");

async function main() {
  const matches = parseInt(process.argv[2] || "10000", 10);
  const seed = BigInt(process.argv[3] || "1");

  const wasmPath = path.join(
    __dirname,
    "..",
    "..",
    "target",
    "wasm32-unknown-unknown",
    "release",
    "kadu_wasm_check.wasm"
  );
  const bytes = fs.readFileSync(wasmPath);
  const { instance } = await WebAssembly.instantiate(bytes, {});

  const start = Date.now();
  const raw = instance.exports.run_bench(matches, seed);
  const result = BigInt.asUintN(64, raw); // i64 -> u64
  const elapsed = (Date.now() - start) / 1000;

  console.log(`matches=${matches} seed=${seed} elapsed=${elapsed.toFixed(3)}s`);
  console.log(`aggregate_hash=0x${result.toString(16).padStart(16, "0")}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
