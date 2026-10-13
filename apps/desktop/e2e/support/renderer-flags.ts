/**
 * What keeps the app's renderer running on a hosted runner (apps-panel replay, Evidence, and the browser specs read the page while the
 * window is up, never in front of anyone). A window the session reports as covered, minimised or unfocused is "hidden" to Chromium:
 * animation frames stop and timers are held to one tick a second, and xterm.js, which paints in a frame and parses in timer slices,
 * shows its text seconds late. The run must not depend on the runner's window stacking, so the test launch turns that off in the
 * renderer and the main process. Production keeps Chromium's behaviour: the main process reads `MODBIT_E2E_NO_THROTTLE`, which only
 * the harness sets. `MODBIT_E2E_HARDEN=off` restores the old launch (the CI stress run compares the two).
 */
export const hardened = (): boolean => process.env.MODBIT_E2E_HARDEN !== "off";

/** Chromium switches for the test launch. */
export function rendererArgs(): string[] {
  const features = ["CalculateNativeWinOcclusion", ...(hardened() ? ["IntensiveWakeUpThrottling"] : [])];
  return [`--disable-features=${features.join(",")}`, "--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding", ...(hardened() ? ["--disable-background-timer-throttling"] : [])];
}

/** The environment the main process reads to create its windows without background throttling (the test build only). */
export function rendererEnv(): Record<string, string> {
  return hardened() ? { MODBIT_E2E_NO_THROTTLE: "1" } : {};
}
