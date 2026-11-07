// Minimal Electron host that shows a URL at a fixed content size; used only for reference rendering.
const { app, BrowserWindow, nativeTheme } = require("electron");
app.whenReady().then(() => {
  nativeTheme.themeSource = process.env.REF_SCHEME === "dark" ? "dark" : "light";
  const w = new BrowserWindow({ width: Number(process.env.REF_W || 1100), height: Number(process.env.REF_H || 900), useContentSize: true, show: true, webPreferences: { sandbox: true, contextIsolation: true } });
  w.loadURL(process.env.REF_URL);
});
app.on("window-all-closed", () => app.quit());
