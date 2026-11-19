// The app's shared/sphere.ts (its joining, keys and assertions), as the rig uses it, loaded the way
// `npm run test:sphere` loads it: through the app's own jiti. Node's TypeScript loader can't resolve
// its `import … from './policy'`, which has no extension (#2493, RIG-26). appDir: apps/electron,
// its packages installed. Answers the module's exports, named, or on `default` where a loader puts
// them there; throws when neither has them.
import { join } from "node:path";
import { pathToFileURL } from "node:url";

export async function loadSphereSpec(appDir) {
  const { createJiti } = await import(pathToFileURL(join(appDir, "node_modules/jiti/lib/jiti.mjs")).href);
  const mod = await createJiti(import.meta.url).import(join(appDir, "src/shared/sphere.ts"));
  const spec = typeof mod.publicOf === "function" ? mod : mod.default;
  if (typeof spec?.publicOf !== "function") throw new Error(`${join(appDir, "src/shared/sphere.ts")} loaded without its exports`);
  return spec;
}
