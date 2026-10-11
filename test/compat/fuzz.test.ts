import { describe, test } from "vitest";
import { stringify } from "yaml";
import { generate, inputsOf, shrink } from "./fuzz.js";
import { diff, evaluateAll, report, rulesOf } from "./outcome.js";
import {
  expectedOutcomes,
  findSituation,
  loadSuites,
  saveSituation,
} from "./suites.js";

const firstSeed = Number(process.env.FUZZ_SEED ?? 0);
const runs = Number(process.env.FUZZ_RUNS ?? 20);
const save = Boolean(process.env.FUZZ_SAVE);

for (const suite of await loadSuites()) {
  describe(suite.name, () => {
    for (let seed = firstSeed; seed < firstSeed + runs; seed++) {
      test(`seed ${seed}`, async () => {
        const situation = generate(await inputsOf(suite), seed);
        const expected = await expectedOutcomes(suite, situation);
        const names = rulesOf(expected);
        const mismatches = diff(
          expected,
          evaluateAll(suite.candidate(), situation, names),
        );
        const [first] = mismatches;
        if (!first) return;

        const shrunk = shrink(suite, situation, first.name);
        const known = findSituation(suite, shrunk);
        const name = `fuzz ${seed}`;
        if (save && !known) await saveSituation(suite, name, shrunk);
        const outcome = known
          ? `Shrunk situation for "${first.name}" is regression situation "${known}".`
          : save
            ? `Saved shrunk situation for "${first.name}" as regression situation "${name}".`
            : `Shrunk situation for "${first.name}" (FUZZ_SAVE=1 adds it to ${suite.file}):\n${stringify({ [name]: shrunk })}`;
        throw new Error(
          `${report(mismatches, names.length)}\n\n${outcome}\n\nFull situation:\n${stringify(situation)}`,
        );
      });
    }
  });
}
