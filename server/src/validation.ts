/**
 * Aplanar un fallo de zod para el log del servidor — nunca se descarta un
 * mensaje WS inválido en silencio.
 *
 * # Por qué el tipo NO es `ZodError`
 *
 * Hay **dos zod en el monorepo**: `packages/shared` declara `^3.24.1` (resuelve
 * 3.25.76) y es de donde salen los schemas (`InputMessageSchema`,
 * `ProgressDeltaSchema`, `JoinOptionsSchema`); el servidor tiene 4.4.3. Un
 * `ZodError` de la v3 no tiene `type` ni `_zod`, así que no es asignable al
 * `ZodError` de la v4 — y como en zod 4 ese parámetro genérico además es
 * INVARIANTE, ni siquiera `ZodError<T>` salvaba las tres llamadas de
 * `SurvivalRoom`. El build del servidor (`tsc`) no compilaba por eso.
 *
 * Acá sólo se lee `issues`, que las dos versiones tienen igual. Pidiendo esa
 * forma y nada más, la función deja de tener opinión sobre qué zod la llama y el
 * problema no puede volver cuando una de las dos suba de mayor.
 *
 * (Alinear las dos versiones es la otra mitad del arreglo y es un cambio de
 * dependencias, no de este archivo.)
 */
export interface FalloDeValidacion {
  issues: readonly { path: PropertyKey[]; code: string; message: string }[];
}

export function formatZodFailure(error: FalloDeValidacion): {
  issues: { path: string; code: string; message: string }[];
} {
  return {
    issues: error.issues.map((i) => ({
      path: i.path.join(".") || "(root)",
      code: i.code,
      message: i.message,
    })),
  };
}
