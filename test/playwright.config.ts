import { defineConfig } from "@playwright/test";
import fs from "fs";
import path from "path";

// Remove stale test database so each run starts fresh
const testDb = path.join(__dirname, "..", "test_documents.db");
try {
  fs.unlinkSync(testDb);
} catch {}

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  expect: {
    timeout: 10_000,
  },
  fullyParallel: false, // tests share one server, run sequentially
  retries: 1,
  reporter: "list",
  use: {
    baseURL: "http://localhost:8081",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
  webServer: {
    command:
      "cd .. && cargo run --bin ws-server --release -- --ws-address 0.0.0.0:8081 --database-path test_documents.db",
    port: 8081,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000, // cargo build may take a while first time
  },
});
