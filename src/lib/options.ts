// Select and segmented controls report their choice as a plain string. This
// narrows it back to one of the options the control was built from.
export function pickOption<T extends string>(
  options: readonly (readonly [T, string])[],
  value: string,
): T | undefined {
  return options.find(([id]) => id === value)?.[0];
}
