// Instantiates the kadu-wasm-check module and calls run_bench(matches, seed),
// printing the aggregate hash. With --expect <path>, reads
// `expected_aggregate = "0x...."` out of a determinism/expected.toml-style
// file (same trivial parse kadu-cli's own --expect flag uses) and exits
// non-zero on mismatch - this is the actual CI gate for the wasm32 leg, the
// only leg that exercises a genuinely different code generator rather than
// just a different OS on the same x86_64/LLVM path.
const fs = require("fs");
const path = require("path");

function readExpectedAggregate(expectPath) {
  const text = fs.readFileSync(expectPath, "utf8");
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("expected_aggregate")) {
      const rhs = trimmed.split("=")[1].trim();
      const hex = rhs.replace(/"/g, "").replace(/^0x/, "");
      return BigInt("0x" + hex);
    }
  }
  throw new Error(`no expected_aggregate key found in ${expectPath}`);
}

function parseArgs(argv) {
  const positional = [];
  let expectPath = null;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--expect") {
      expectPath = argv[++i];
    } else {
      positional.push(argv[i]);
    }
  }
  return { positional, expectPath };
}

async function main() {
  const { positional, expectPath } = parseArgs(process.argv.slice(2));
  const matches = parseInt(positional[0] || "10000", 10);
  const seed = BigInt(positional[1] || "1");

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
  const computed = BigInt.asUintN(64, raw); // i64 -> u64
  const elapsed = (Date.now() - start) / 1000;

  console.log(`matches=${matches} seed=${seed} elapsed=${elapsed.toFixed(3)}s`);
  console.log(`aggregate_hash=0x${computed.toString(16).padStart(16, "0")}`);

  if (expectPath) {
    const expected = readExpectedAggregate(expectPath);
    if (computed !== expected) {
      console.error(`FAIL: aggregate mismatch against ${expectPath}`);
      console.error(`  expected: 0x${expected.toString(16).padStart(16, "0")}`);
      console.error(`  computed: 0x${computed.toString(16).padStart(16, "0")}`);
      process.exit(1);
    }
    console.log(`OK: aggregate matches ${expectPath}`);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
