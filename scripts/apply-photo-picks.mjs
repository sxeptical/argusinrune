#!/usr/bin/env bun
/**
 * Authoring-time: apply scripts/data/photo-picks.json to the current static
 * bundles in data/ without refetching any source. The fetch scripts apply
 * the same picks on every rebuild; run this after editing the picks file.
 */
import { readFile, writeFile } from "node:fs/promises";
import { ARCHIVAL_PHOTO_OPTIONS } from "./lib/commons-photos.mjs";
import { applyPhotoPicks } from "./lib/photo-picks.mjs";

// layer → bundle file, plus the array within it for multi-layer bundles.
const LAYERS = [
  { layer: "hawkers", file: "data/amenities.json", key: "hawkers" },
  { layer: "malls", file: "data/amenities.json", key: "malls" },
  { layer: "community-clubs", file: "data/community-clubs.json" },
  { layer: "healthcare", file: "data/healthcare.json" },
  { layer: "heritage", file: "data/heritage.json", photoOptions: ARCHIVAL_PHOTO_OPTIONS },
  { layer: "hotels", file: "data/hotels.json" },
  { layer: "libraries", file: "data/libraries.json" },
];

const bundles = new Map();
for (const { layer, file, key, photoOptions } of LAYERS) {
  if (!bundles.has(file)) bundles.set(file, JSON.parse(await readFile(file, "utf8")));
  const bundle = bundles.get(file);
  const places = key ? bundle[key] : bundle;
  const { places: picked, applied, unresolved } = await applyPhotoPicks(layer, places, photoOptions);
  if (key) bundle[key] = picked;
  else bundles.set(file, picked);
  console.log(`${layer}: picks applied to ${applied} places${unresolved.length ? `; unresolved: ${unresolved.join("; ")}` : ""}`);
}
for (const [file, bundle] of bundles) await writeFile(file, JSON.stringify(bundle));
