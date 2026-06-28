#!/usr/bin/env node
/** Print first non-internal IPv4 (WiFi/LAN) for EXPO_PUBLIC_SERVER_HOST / UPDATE_BASE_URL. */
import os from "node:os";

for (const ifaces of Object.values(os.networkInterfaces())) {
  for (const iface of ifaces ?? []) {
    if (iface.family === "IPv4" && !iface.internal) {
      process.stdout.write(iface.address);
      process.exit(0);
    }
  }
}
process.stdout.write("127.0.0.1");
