import {
  convertUnit,
  type FormatUnit,
  serializeUnit,
  simplifyUnit,
  type Unit,
} from "./units.js";

export function capitalise0(name: undefined): undefined;
export function capitalise0(name: string): string;
export function capitalise0(name?: string): string | undefined {
  return name && (name[0]?.toUpperCase() ?? "") + name.slice(1);
}

type Style = "currency" | "percent" | "decimal";

function formatNumber(
  value: number,
  style: Style,
  maximumFractionDigits: number,
  minimumFractionDigits: number,
  language?: string,
): string {
  const minimum =
    style === "currency" &&
    maximumFractionDigits >= 2 &&
    minimumFractionDigits === 0 &&
    !Number.isInteger(value)
      ? 2
      : minimumFractionDigits;
  return Intl.NumberFormat(language, {
    style,
    currency: "EUR",
    maximumFractionDigits,
    minimumFractionDigits: minimum,
  }).format(value);
}

function formatWithUnit(
  nodeValue: number,
  {
    precision,
    language,
    formatUnit,
    unit,
  }: {
    precision: number;
    language: string;
    formatUnit?: FormatUnit;
    unit?: Unit | string;
  },
): string {
  const serialized = unit
    ? serializeUnit(unit, nodeValue, formatUnit)
    : undefined;
  switch (serialized) {
    case "€":
      return formatNumber(nodeValue, "currency", precision, 0, language);
    case "%":
      return formatNumber(nodeValue / 100, "percent", precision, 0, language);
    default:
      return (
        formatNumber(nodeValue, "decimal", precision, 0, language) +
        (typeof serialized === "string" ? ` ${serialized}` : "")
      );
  }
}

const booleans: Record<string, Record<"true" | "false", string>> = {
  fr: { true: "oui", false: "non" },
  en: { true: "yes", false: "no" },
};

type Formattable =
  | number
  | { nodeValue: number | string | boolean | null | undefined; unit?: Unit }
  | undefined;

export function formatValue(
  value: Formattable,
  {
    language = "fr",
    displayedUnit,
    formatUnit,
    precision = 2,
  }: {
    language?: string;
    displayedUnit?: string;
    precision?: number;
    formatUnit?: FormatUnit;
  } = {},
): string | undefined {
  let nodeValue =
    typeof value === "number" || value == null ? value : value.nodeValue;
  if (typeof nodeValue === "number" && Number.isNaN(nodeValue)) {
    return "Erreur dans le calcul du nombre";
  }
  if (nodeValue === undefined) return "Pas encore défini";
  if (nodeValue === null) return "Non applicable";
  if (typeof nodeValue === "string") return nodeValue.replace("\\n", "\n");
  if (typeof nodeValue === "boolean") {
    return booleans[language]?.[`${nodeValue}`];
  }
  let unit =
    typeof value === "object" && value !== null && "unit" in value
      ? value.unit
      : undefined;
  if (unit) {
    const simplified = simplifyUnit(unit);
    nodeValue = convertUnit(unit, simplified, nodeValue);
    unit = simplified;
  }
  return formatWithUnit(nodeValue, {
    precision,
    language,
    formatUnit,
    unit: displayedUnit ?? unit,
  }).trim();
}
