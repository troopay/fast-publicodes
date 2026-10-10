import { PublicodesError } from "./error.js";

export interface Unit {
  numerators: string[];
  denominators: string[];
}

export type FormatUnit = (unit: string, count: number) => string;

/** Each base unit with its power, the power written as trailing digits. */
function unitCounts(units: string[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const unit of units) {
    const power = unit.match(/(\d+)(?!.*[A-Za-z])/g)?.[0];
    const base = power === undefined ? unit : (unit.split(power)[0] ?? "");
    counts.set(
      base,
      (counts.get(base) ?? 0) + (power === undefined ? 1 : +power),
    );
  }
  return counts;
}

export function parseUnit(
  text: string,
  getUnitKey: (writtenUnit: string) => string = (x) => x,
): Unit {
  if (text.includes("/ ")) {
    throw new Error(
      `L'unité "${text}" ne doit pas contenir d'espace après "/"`,
    );
  }
  const [numerators = "", ...denominators] = text.split("/");
  const split = (part: string) =>
    [
      ...unitCounts(
        part.trim().split(".").filter(Boolean).map(getUnitKey),
      ).entries(),
    ].flatMap(([unit, power]) => Array<string>(power).fill(unit));
  return {
    numerators: split(numerators),
    denominators: denominators.flatMap(split),
  };
}

/** Cancels the units found both as numerator and denominator. */
function simplify(unit: Unit, same: (a: string, b: string) => boolean): Unit {
  let { numerators, denominators } = unit;
  const removeOnce = (list: string[], item: string) => {
    const index = list.findIndex((e) => same(e, item));
    return list.filter((_, i) => i !== index);
  };
  for (const next of [...unit.numerators, ...unit.denominators]) {
    if (
      numerators.some((u) => same(next, u)) &&
      denominators.some((u) => same(next, u))
    ) {
      numerators = removeOnce(numerators, next);
      denominators = removeOnce(denominators, next);
    }
  }
  return { numerators, denominators };
}

const plural = 2;

export function serializeUnit(
  rawUnit: Unit | undefined | string,
  count: number = plural,
  formatUnit: FormatUnit = (x) => x,
): string | undefined {
  if (rawUnit === null || typeof rawUnit !== "object") {
    return typeof rawUnit === "string" ? formatUnit(rawUnit, count) : rawUnit;
  }
  const { numerators = [], denominators = [] } = simplify(
    rawUnit,
    (a, b) => a === b,
  );
  const print = (units: string[], n: number) =>
    [...unitCounts(units.map((unit) => formatUnit(unit, n))).entries()]
      .map(([unit, power]) => (power > 1 ? `${unit}${power}` : unit))
      .join(".");
  if (!numerators.length && !denominators.length) return "";
  if (!denominators.length) return print(numerators, count);
  if (!numerators.length) return `/${print(denominators, 1)}`;
  return `${print(numerators, plural)}/${print(denominators, 1)}`;
}

const conversions: Record<string, number> = {
  "mois/an": 12,
  "jour/an": 365,
  "jour/mois": 365 / 12,
  "trimestre/an": 4,
  "mois/trimestre": 3,
  "jour/trimestre": (365 / 12) * 3,
  "€/k€": 10 ** 3,
  "g/kg": 10 ** 3,
  "mg/g": 10 ** 3,
  "mg/kg": 10 ** 6,
  "m/km": 10 ** 3,
  "cm/m": 10 ** 2,
  "mm/cm": 10 ** 1,
  "mm/m": 10 ** 3,
  "cm/km": 10 ** 5,
  "mm/km": 10 ** 6,
};

/** Units that convert into one another. */
const classes: Set<string>[] = [];
for (const ratio of Object.keys(conversions)) {
  const [a, b] = ratio.split("/") as [string, string];
  const withA = classes.find((c) => c.has(a));
  const withB = classes.find((c) => c.has(b));
  if (!withA && !withB) classes.push(new Set([a, b]));
  else (withA ?? withB)?.add(withA ? b : a);
}

const sameClass = (a: string, b: string) =>
  a === b || classes.some((c) => c.has(a) && c.has(b));

const removePercentages = (unit: Unit): Unit => ({
  numerators: unit.numerators.filter((u) => u !== "%"),
  denominators: unit.denominators.filter((u) => u !== "%"),
});

export function simplifyUnit(unit: Unit): Unit {
  const { numerators, denominators } = simplify(unit, sameClass);
  if (numerators.length && numerators.every((u) => u === "%")) {
    return { numerators: ["%"], denominators };
  }
  return removePercentages({ numerators, denominators });
}

const round = (value: number) => +value.toFixed(16);

function factor(from: string, to: string): number | undefined {
  const direct = conversions[`${to}/${from}`];
  if (direct) return direct;
  const inverse = conversions[`${from}/${to}`];
  return inverse && 1 / inverse;
}

function conversionFactor(from: string[], to: string[]): number {
  const percents = (units: string[]) => units.filter((u) => u === "%").length;
  let result = 100 ** (percents(to) - percents(from));
  for (const unit of from) {
    const target = to.find((t) => factor(unit, t) !== undefined);
    result *= (target !== undefined && factor(unit, target)) || 1;
  }
  return result;
}

function convertible(a: Unit | undefined, b: Unit | undefined): boolean {
  if (a == null || b == null) return true;
  const countByClass = (units: string[]) => {
    const counts: Record<string, number> = {};
    for (const unit of units) {
      const index = classes.findIndex((c) => c.has(unit));
      const key = index === -1 ? unit : `${index}`;
      counts[key] = (counts[key] ?? 0) + 1;
    }
    return counts;
  };
  const [numA, denA, numB, denB] = [
    a.numerators,
    a.denominators,
    b.numerators,
    b.denominators,
  ].map(countByClass) as [
    Record<string, number>,
    Record<string, number>,
    Record<string, number>,
    Record<string, number>,
  ];
  const keys = new Set(
    [numA, denA, numB, denB].flatMap((counts) => Object.keys(counts)),
  );
  return [...keys].every(
    (key) =>
      key === "%" ||
      (numA[key] ?? 0) - (denA[key] ?? 0) ===
        (numB[key] ?? 0) - (denB[key] ?? 0),
  );
}

const equivalents: Record<string, string> = { "kW.h": "kWh", "mn/h": "noeud" };

export function convertUnit<Value extends number | undefined | null>(
  from: Unit | undefined,
  to: Unit | undefined,
  value: Value,
): Value {
  const serializedFrom = serializeUnit(from);
  const serializedTo = serializeUnit(to);
  const equivalent =
    !!serializedFrom &&
    !!serializedTo &&
    (serializedFrom === serializedTo ||
      serializedFrom === equivalents[serializedTo] ||
      serializedTo === equivalents[serializedFrom]);
  if (!equivalent && !convertible(from, to)) {
    throw new PublicodesError(
      "EngineError",
      `Impossible de convertir l'unité '${serializedFrom}' en '${serializedTo}'`,
      {},
    );
  }
  if (!value || from === undefined) return value;
  const simplified = (unit: Unit): [Unit, number] => [
    simplify(removePercentages(unit), sameClass),
    round(conversionFactor(unit.numerators, unit.denominators)),
  ];
  const [fromSimplified, factorTo] = simplified(from);
  const [toSimplified, factorFrom] = simplified(
    to ?? { numerators: [], denominators: [] },
  );
  return round(
    ((value * factorTo) / factorFrom) *
      conversionFactor(fromSimplified.numerators, toSimplified.numerators) *
      conversionFactor(toSimplified.denominators, fromSimplified.denominators),
  ) as Value;
}
