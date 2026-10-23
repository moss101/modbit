/**
 * M10.2 (docs/70 "Desktop update"): the real packaged app updates itself from a
 * signed manifest served over real HTTP, and refuses what it must refuse.
 * Two real packages (0.0.1 and 0.0.2) are built with the same public key
 * pinned; the app under test is a real extracted .app that spawns its own
 * bundled Core. Failure cases are real hostile feeds, a real SIGKILL mid-download,
 * and a real helper process that fails to install.
 *
 * macOS only for now: the Windows (NSIS) and Linux (AppImage) installers have
 * their own helper paths that join this suite when their CI jobs prove them.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { FeedServer, bundleVersion, buildZip, extractApp, makeKeys, scratch, signedManifest, writeText, type Keys } from "./update-fixture.js";

test.skip(process.platform !== "darwin", "the packaged update suite runs on macOS until the NSIS and AppImage helper paths have their own CI proof");
test.describe.configure({ mode: "serial" });
test.setTimeout(600_000);

const OLD = "0.0.1";
const NEW = "0.0.2";

let work: string;
let keys: Keys;
let stranger: Keys;
let oldZip: string;
let newZip: string;
let installed: { app: string; exe: string };
let dataDir: string;
const feed = new FeedServer();

type Launched = { app: ElectronApplication; page: Page };

async function launch(extraEnv: Record<string, string> = {}): Promise<Launched> {
  const env = {
    ...process.env,
    MODBIT_DATA_DIR: dataDir,
    MODBIT_UPDATE_FEED: feed.url,
    MODBIT_UPDATE_NO_RELAUNCH: "1",
    ...extraEnv,
  } as Record<string, string>;
  delete env.MODBIT_CORE_BIN;
  const app = await electron.launch({ executablePath: installed.exe, env });
  const page = await app.firstWindow();
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

const state = (p: Page) => p.getByTestId("update-state");
const updateDir = () => join(dataDir, "update");
const pendingFile = () => join(updateDir(), "pending.json");

async function check(p: Page): Promise<void> {
  await p.getByTestId("update-check").click();
}

function publish(zip: string, version: string, o: Partial<Parameters<typeof signedManifest>[2]> = {}): void {
  const manifest = signedManifest(feed.url, work, { version, zip, keys, ...o });
  feed.set("manifest.json", manifest);
  feed.set(`releases/${version}/manifest.json`, manifest);
  feed.set(o.artifactName ?? `Modbit-${version}.zip`, readFileSync(zip));
}

test.beforeAll(async () => {
  work = scratch("modbit-update-e2e-");
  keys = makeKeys(work, "primary");
  stranger = makeKeys(work, "stranger");
  oldZip = buildZip(OLD, keys, join(work, "out-old"));
  newZip = buildZip(NEW, keys, join(work, "out-new"));
  installed = extractApp(oldZip, join(work, "installed"));
  expect(bundleVersion(installed.app)).toBe(OLD);
  await feed.start();
});

test.afterAll(async () => {
  await feed.stop();
});

test.beforeEach(() => {
  dataDir = scratch("modbit-update-data-");
});

test("a build with no pinned key verifies nothing and offers no update", async () => {
  // The unpackaged Electron has no update-keys.json beside it: disabled, fail closed.
  const app = await electron.launch({
    args: [join(process.cwd(), "dist", "main", "main.cjs")],
    env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_UPDATE_FEED: feed.url } as Record<string, string>,
  });
  try {
    const page = await app.firstWindow();
    await expect(state(page)).toHaveAttribute("data-state", "disabled");
    await expect(state(page)).toContainText("Updates off");
    await expect(page.getByTestId("update-check")).toHaveCount(0);
    expect(feed.requests.filter((r) => r === "manifest.json").length, "a keyless build never asks the feed").toBe(0);
  } finally {
    await app.close();
  }
});

test("hostile feeds are refused with their own reason and nothing is staged", async () => {
  const { app, page } = await launch();
  try {
    await expect(state(page)).toHaveAttribute("data-version", OLD);
    const refuses = async (stateName: string, text: RegExp) => {
      await check(page);
      await expect(state(page)).toHaveAttribute("data-state", stateName, { timeout: 30_000 });
      await expect(state(page)).toContainText(text);
      expect(existsSync(pendingFile()), "nothing staged").toBe(false);
      expect(readdirSync(join(updateDir(), "downloads")).filter((n) => !n.endsWith(".part"))).toEqual([]);
    };

    // The artifact's bytes differ from what the signed manifest promised.
    publish(newZip, NEW);
    const real = readFileSync(newZip);
    const flipped = Buffer.from(real);
    flipped[flipped.length - 1] = flipped[flipped.length - 1]! ^ 0xff;
    feed.set(`Modbit-${NEW}.zip`, flipped);
    await refuses("error", /ARTIFACT_DIGEST_MISMATCH/);

    // Truncated in transit.
    feed.set(`Modbit-${NEW}.zip`, real.subarray(0, 1024));
    await refuses("error", /ARTIFACT_SIZE_MISMATCH/);

    // The manifest was altered after signing.
    publish(newZip, NEW);
    const signed = JSON.parse(feed.routes.get("manifest.json")!.body.toString());
    signed.payload = signed.payload.replace('"rollout_percent":100', '"rollout_percent":99');
    feed.set("manifest.json", JSON.stringify(signed));
    await refuses("error", /SIGNATURE_INVALID/);

    // Signed by a key this build does not pin.
    publish(newZip, NEW, { keys: stranger });
    await refuses("error", /SIGNATURE_UNKNOWN_KEY/);

    // Staged rollout: this install is outside 0%.
    publish(newZip, NEW, { rolloutPercent: 0 });
    await refuses("held", /staged for a later rollout/);

    // The database on disk is older than this update can migrate from, and newer than it writes.
    publish(newZip, NEW, { minDbSchemaVersion: 99 });
    await refuses("refused", /DB_TOO_OLD/);
    publish(newZip, NEW, { schemaVersion: 1, minDbSchemaVersion: 1 });
    await refuses("refused", /DB_NEWER_THAN_UPDATE/);

    // Only one thing was ever fetched from the feed that was not a manifest: tampered artifacts, never run.
    expect(bundleVersion(installed.app)).toBe(OLD);
  } finally {
    await app.close();
  }
});

test("the device policy's minimum_version is honored from the Core's own policy reader", async () => {
  const policy = join(work, "device-policy.json");
  writeText(policy, JSON.stringify({ device: { minimum_version: "9.0.0" } }));
  publish(newZip, NEW);
  const { app, page } = await launch({ MODBIT_DEVICE_POLICY: policy });
  try {
    await check(page);
    await expect(state(page)).toHaveAttribute("data-state", "refused", { timeout: 30_000 });
    await expect(state(page)).toContainText("BELOW_DEVICE_MINIMUM");
    expect(existsSync(pendingFile())).toBe(false);
  } finally {
    await app.close();
  }
});

test("a SIGKILL during the download leaves nothing that looks verified, and the next check succeeds", async () => {
  publish(newZip, NEW);
  const body = readFileSync(newZip);
  feed.set(`Modbit-${NEW}.zip`, body, { stallAfter: Math.floor(body.length / 2) });
  const { app, page } = await launch();
  await check(page);
  await expect.poll(() => feed.requests.includes(`Modbit-${NEW}.zip`), { timeout: 30_000 }).toBe(true);
  app.process().kill("SIGKILL");
  await app.waitForEvent("close").catch(() => {});
  feed.releaseStalled();
  expect(existsSync(pendingFile()), "a killed download stages nothing").toBe(false);

  feed.set(`Modbit-${NEW}.zip`, body);
  const again = await launch();
  try {
    expect(readdirSync(join(updateDir(), "downloads")).filter((n) => n.endsWith(".part")), "no half-download survives a restart").toEqual([]);
    await check(again.page);
    await expect(state(again.page)).toHaveAttribute("data-state", "staged", { timeout: 60_000 });
  } finally {
    await again.app.close();
  }
});

test("an install that cannot complete leaves the previous app in place and says why", async () => {
  // A correctly signed, correctly hashed artifact — but the bundle inside is 0.0.2, not the 0.0.3 the manifest promises.
  publish(newZip, "0.0.3", { artifactName: "Modbit-0.0.3.zip" });
  const first = await launch();
  await check(first.page);
  await expect(state(first.page)).toHaveAttribute("data-state", "staged", { timeout: 60_000 });
  await first.page.getByTestId("update-install").click();
  await first.app.waitForEvent("close").catch(() => {});
  const resultFile = join(updateDir(), "apply-result.json");
  await expect.poll(() => existsSync(resultFile), { timeout: 60_000 }).toBe(true);
  const result = JSON.parse(readFileSync(resultFile, "utf8")) as { ok: boolean; error?: string };
  expect(result.ok).toBe(false);
  expect(result.error).toMatch(/promised 0\.0\.3/);
  expect(bundleVersion(installed.app), "the app on disk is still the old one").toBe(OLD);
  expect(readdirSync(join(installed.app, "..")).filter((n) => n.includes(".old-") || n.includes(".new-")), "no half-swapped bundle left beside it").toEqual([]);

  const next = await launch();
  try {
    await expect(state(next.page)).toContainText("failed");
    await expect(state(next.page)).toHaveAttribute("data-version", OLD);
  } finally {
    await next.app.close();
  }
});

test("a signed update installs, the new version runs, and a rollback restores the old one", async () => {
  publish(newZip, NEW);
  // The rollback target's own signed manifest and artifact, as a feed keeps them.
  const oldManifest = signedManifest(feed.url, work, { version: OLD, zip: oldZip, keys });
  feed.set(`releases/${OLD}/manifest.json`, oldManifest);
  feed.set(`Modbit-${OLD}.zip`, readFileSync(oldZip));

  const first = await launch();
  await check(first.page);
  await expect(state(first.page)).toHaveAttribute("data-state", "staged", { timeout: 60_000 });
  await first.page.getByTestId("update-install").click();
  await first.app.waitForEvent("close").catch(() => {});

  const resultFile = join(updateDir(), "apply-result.json");
  await expect.poll(() => existsSync(resultFile), { timeout: 90_000 }).toBe(true);
  const applied = JSON.parse(readFileSync(resultFile, "utf8")) as { ok: boolean; version: string; error?: string };
  expect(applied, JSON.stringify(applied)).toMatchObject({ ok: true, version: NEW });
  expect(bundleVersion(installed.app)).toBe(NEW);

  const second = await launch();
  try {
    await expect(state(second.page)).toHaveAttribute("data-version", NEW);
    expect(await second.app.evaluate(({ app }) => app.getVersion())).toBe(NEW);
    // The new app opened the same profile: the Core connected on the database the old one left.
    await check(second.page);
    await expect(state(second.page)).toHaveAttribute("data-state", "up-to-date", { timeout: 30_000 });
    expect(readdirSync(join(installed.app, "..")).filter((n) => n.includes(".old-")), "the replaced bundle is released after a good start").toEqual([]);

    await second.page.getByTestId("update-rollback").click();
    await second.app.waitForEvent("close").catch(() => {});
  } finally {
    await second.app.close().catch(() => {});
  }
  await expect.poll(() => existsSync(resultFile), { timeout: 90_000 }).toBe(true);
  const rolled = JSON.parse(readFileSync(resultFile, "utf8")) as { ok: boolean; version: string; error?: string };
  expect(rolled, JSON.stringify(rolled)).toMatchObject({ ok: true, version: OLD });
  expect(bundleVersion(installed.app)).toBe(OLD);

  const third = await launch();
  try {
    await expect(state(third.page)).toHaveAttribute("data-version", OLD);
  } finally {
    await third.app.close();
  }
});
