#!/usr/bin/env node

import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const baseUrl = process.env.KIZASHI_UI_URL ?? "http://localhost:8093";
const debuggerUrl = process.env.CHROME_DEBUGGER_URL ?? "http://127.0.0.1:9222";
const outputDir = resolve("docs/ui-guide-assets");

const pages = [
  ["01-overview", "/overview", "Console overview"],
  ["02-work", "/work", "Work queue"],
  ["03-workflows", "/workflows", "Workflow exceptions"],
  ["04-events", "/events", "Events"],
  ["05-incidents", "/incidents", "Incidents"],
  ["06-actions", "/actions", "Actions"],
  ["07-triggers", "/triggers", "Triggers"],
  ["08-build", "/build", "Build Studio"],
  ["09-data-sources", "/build/data-sources", "Build Studio — Data Sources"],
  ["10-pipelines", "/build/pipelines", "Build Studio — Pipelines"],
  ["11-ontology", "/ontology", "Ontology / Models"],
  ["12-data", "/data", "Data records"],
  ["13-apps", "/apps", "Apps"],
  ["14-reports", "/reports", "Reports"],
  ["15-search", "/search", "Search"],
  ["16-security", "/security", "Security overview"],
  ["17-users", "/users", "Users"],
  ["18-api-keys", "/api-keys", "API keys"],
  ["19-sensors", "/sensors", "Sensors"],
  ["20-retention", "/retention-policies", "Retention policies"],
  ["21-health", "/health", "Platform health"],
  ["22-configuration", "/configuration", "Configuration"],
];

const sleep = (milliseconds) => new Promise((resolveSleep) => setTimeout(resolveSleep, milliseconds));

async function createClient() {
  const target = await fetch(`${debuggerUrl}/json/new?about:blank`, { method: "PUT" }).then((response) => response.json());
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolveOpen, rejectOpen) => {
    socket.addEventListener("open", resolveOpen, { once: true });
    socket.addEventListener("error", rejectOpen, { once: true });
  });
  let id = 0;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    message.error ? request.reject(new Error(message.error.message)) : request.resolve(message.result);
  });
  return {
    async send(method, params = {}) {
      const requestId = ++id;
      const response = new Promise((resolveRequest, rejectRequest) => pending.set(requestId, { resolve: resolveRequest, reject: rejectRequest }));
      socket.send(JSON.stringify({ id: requestId, method, params }));
      return response;
    },
    close() { socket.close(); },
  };
}

async function navigate(client, path) {
  await client.send("Page.navigate", { url: `${baseUrl}${path}` });
  await sleep(900);
}

async function main() {
  await mkdir(outputDir, { recursive: true });
  const client = await createClient();
  await client.send("Page.enable");
  await client.send("Runtime.enable");
  await client.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false });

  await navigate(client, "/login");
  await client.send("Runtime.evaluate", { expression: `
    document.querySelector('[name=tenant_name]').value = 'acme';
    document.querySelector('[name=username]').value = 'demo';
    document.querySelector('[name=password]').value = 'kizashi-local-demo-password';
    document.querySelector('form.login').submit();
  ` });
  await sleep(1200);

  for (const [file, path, title] of pages) {
    await navigate(client, path);
    const image = await client.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false, fromSurface: true });
    await writeFile(resolve(outputDir, `${file}.png`), Buffer.from(image.data, "base64"));
    console.log(`captured ${title}`);
  }

  const sections = pages.map(([file, path, title]) => `
    <section>
      <h1>${title}</h1><p>${path}</p>
      <img src="ui-guide-assets/${file}.png" alt="${title}">
    </section>`).join("\n");
  await writeFile(resolve("docs/ui-guide-screenshots.html"), `<!doctype html><html><head><meta charset="utf-8"><title>Kizashi Console UI screenshots</title><style>
    @page { size: A4 landscape; margin: 10mm; }
    * { box-sizing: border-box; } body { font-family: Arial, sans-serif; color: #101418; margin: 0; }
    section { break-after: page; page-break-after: always; } section:last-child { break-after: auto; page-break-after: auto; }
    h1 { margin: 0; font-size: 19pt; } p { color: #4b5563; margin: 3mm 0 5mm; font-family: monospace; }
    img { width: 100%; max-height: 162mm; object-fit: contain; object-position: top; border: 1px solid #d1d5db; }
  </style></head><body>${sections}</body></html>`);
  client.close();
}

main().catch((error) => { console.error(error); process.exit(1); });
