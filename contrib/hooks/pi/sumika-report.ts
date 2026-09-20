import { spawn } from "node:child_process";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

function report(payload: Record<string, string>) {
  try {
    const child = spawn("sumika", ["hook-report"], {
      stdio: ["pipe", "ignore", "ignore"],
    });
    child.on("error", () => {});
    child.stdin?.end(JSON.stringify(payload));
  } catch {
    // Fail open: a missing binary or spawn error must not stop Pi.
  }
}

export default function (pi: ExtensionAPI) {
  pi.on("agent_end", async () => {
    report({ type: "agent_end" });
  });
  pi.on("agent_settled", async () => {
    report({ type: "agent_settled" });
  });
  try {
    pi.on("permissions:ask", async () => {
      report({ type: "permissions:ask" });
    });
  } catch {
    // Optional: only fires when @pi-lab/permissions is installed.
  }
}
