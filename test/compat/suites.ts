import { createHash } from "node:crypto";
import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { basename } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import PublicodesEngine from "publicodes";
import { parse, parseDocument } from "yaml";
import Engine, { type Rules, type Situation } from "../../src/index.js";
import { evaluateAll, type Outcomes } from "./outcome.js";

/** A rule set with named situations, loaded from `suites/<name>.yaml`. */
export interface Suite {
  name: string;
  file: string;
  rules: Rules;
  situations: Record<string, Situation>;
  oracle(): PublicodesEngine;
  candidate(): Engine;
  cacheKey: string;
}

const SUITES_DIR = new URL("suites/", import.meta.url);
const CACHE_DIR = new URL("../../node_modules/.cache/compat/", import.meta.url);
const PUBLICODES_PACKAGE = new URL(
  "../../node_modules/publicodes/package.json",
  import.meta.url,
);
const silent = { log() {}, warn() {}, error() {} };

export async function loadSuites(): Promise<Suite[]> {
  const harnessHash = await hashHarness();
  const { version } = JSON.parse(await readFile(PUBLICODES_PACKAGE, "utf8"));
  const files = (await readdir(SUITES_DIR)).filter((f) => f.endsWith(".yaml"));
  return Promise.all(
    files.sort().map(async (file) => {
      const path = fileURLToPath(new URL(file, SUITES_DIR));
      const source = parse(await readFile(path, "utf8"));
      const rules: Rules =
        typeof source.rules === "string"
          ? (await import(source.rules)).default
          : source.rules;
      return {
        name: basename(file, ".yaml"),
        file: path,
        rules,
        situations: source.situations ?? {},
        oracle: once(
          () =>
            new PublicodesEngine(
              rules as ConstructorParameters<typeof PublicodesEngine>[0],
              { logger: silent },
            ),
        ),
        candidate: once(() => new Engine(rules)),
        cacheKey: hash(version, harnessHash, JSON.stringify(rules)),
      };
    }),
  );
}

/** Every rule of the suite evaluated by publicodes in `situation`, cached on disk. */
export function expectedOutcomes(
  suite: Suite,
  situation: Situation,
): Promise<Outcomes> {
  return cached(hash(suite.cacheKey, JSON.stringify(situation)), () => {
    const oracle = suite.oracle();
    return evaluateAll(oracle, situation, Object.keys(oracle.getParsedRules()));
  });
}

export async function cached<T>(key: string, compute: () => T): Promise<T> {
  const path = new URL(`${key}.json`, CACHE_DIR);
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch {
    const value = compute();
    await mkdir(CACHE_DIR, { recursive: true });
    await writeFile(path, JSON.stringify(value));
    return value;
  }
}

/** Name of the suite situation equal to `situation`, if any. */
export function findSituation(
  suite: Suite,
  situation: Situation,
): string | undefined {
  return Object.entries(suite.situations).find(([, s]) =>
    isDeepStrictEqual(s, situation),
  )?.[0];
}

export async function saveSituation(
  suite: Suite,
  name: string,
  situation: Situation,
) {
  const document = parseDocument(await readFile(suite.file, "utf8"));
  document.setIn(["situations", name], document.createNode(situation));
  await writeFile(suite.file, document.toString());
  suite.situations[name] = situation;
}

export function hash(...parts: string[]): string {
  const digest = createHash("sha256");
  for (const part of parts) digest.update(part).update("\0");
  return digest.digest("hex").slice(0, 32);
}

async function hashHarness(): Promise<string> {
  const dir = new URL(".", import.meta.url);
  const files = (await readdir(dir))
    .filter((f) => f.endsWith(".ts") && !f.endsWith(".test.ts"))
    .sort();
  return hash(
    ...(await Promise.all(files.map((f) => readFile(new URL(f, dir), "utf8")))),
  );
}

function once<T>(create: () => T): () => T {
  let value: T | undefined;
  return () => {
    value ??= create();
    return value;
  };
}
