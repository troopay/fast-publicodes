export interface PublicodesErrorTypes {
  InternalError: { dottedName?: string };
  EngineError: Record<string, never>;
  SyntaxError: { dottedName: string };
  EvaluationError: { dottedName: string };
  SituationError: { dottedName: string };
  UnknownRule: { dottedName: string };
  PrivateRule: { dottedName: string };
  TypeError: { dottedName: string };
}

const titles: Partial<Record<string, string>> = {
  SyntaxError: "Erreur syntaxique",
  EvaluationError: "Erreur d'évaluation",
  SituationError: "Erreur lors de la mise à jour de la situation",
  UnknownRule: "Règle inconnue",
  PrivateRule: "Règle privée",
};

function buildMessage(
  name: string,
  message: string,
  info?: { dottedName?: string },
  originalError?: Error,
): string {
  const isError = /error/i.test(name);
  return (
    `\n[ ${titles[name] ?? name} ]` +
    (info?.dottedName ? `\n➡️  Dans la règle "${info.dottedName}"` : "") +
    `\n${isError ? "✖️" : "⚠️"}  ${message}` +
    (originalError
      ? `\n${isError ? "    " : "ℹ️  "}${originalError.message}`
      : "")
  );
}

export class PublicodesError<
  Name extends keyof PublicodesErrorTypes = keyof PublicodesErrorTypes,
> extends Error {
  override name: Name;
  info: PublicodesErrorTypes[Name];

  constructor(
    name: Name,
    message: string,
    info: PublicodesErrorTypes[Name],
    originalError?: Error,
  ) {
    super(buildMessage(name, message, info, originalError));
    this.name = name;
    this.info = info;
  }
}

export function isPublicodesError<Name extends keyof PublicodesErrorTypes>(
  error: unknown,
  name?: Name,
): error is PublicodesError<Name> {
  return (
    error instanceof PublicodesError &&
    (name === undefined || error.name === name)
  );
}

export interface NativeError {
  name: keyof PublicodesErrorTypes;
  message: string;
  dottedName: string;
}

/** The error the engine reports, with the message it already wrote. */
export function fromNative({
  name,
  message,
  dottedName,
}: NativeError): PublicodesError {
  const info = (dottedName ? { dottedName } : {}) as never;
  const error = new PublicodesError(name, "", info);
  error.message = message;
  return error;
}
