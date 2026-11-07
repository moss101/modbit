import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";
const r = (p: string) => fileURLToPath(new URL(p, import.meta.url));
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: [
      { find: "@/i18n/IntlProvider.js", replacement: r("./src/stub/i18n/IntlProvider.tsx") },
      { find: "@/ControlShortcutBadge.js", replacement: r("./src/stub/ControlShortcutBadge.tsx") },
      { find: "@/components/lib/utils.js", replacement: r("./src/components/lib/utils.ts") },
    ],
  },
  server: { host: "127.0.0.1", port: 5199, strictPort: true },
});
